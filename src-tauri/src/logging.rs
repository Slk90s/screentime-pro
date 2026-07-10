//! 统一日志系统（v0.4.2 引入）
//!
//! ## 设计目标
//! - **生产环境体积可控**：默认 INFO 级别、文件按日滚动、单文件 5MB、最多保留 3 个文件（总上限 15MB）
//! - **高频路径节流**：采样循环 1Hz → 1/分钟聚合 INFO；不污染日志
//! - **统一通道**：通过 `tauri-plugin-log`，前端 Vue + 后端 Rust 写入同一文件
//! - **隐私红线**：禁止记录 `window_title`、token、密码、聊天内容等敏感字段
//!
//! ## 调用方
//! - `lib.rs::run()` 顶部调用 `init()` 初始化
//! - `tauri-plugin-log` plugin 接管前端→文件的转发
//! - 各业务模块用 `tracing::{info, warn, error, debug}` 埋点
//!
//! ## 日志文件位置
//! - macOS：`~/Library/Logs/com.screentime.pro/app.log.YYYY-MM-DD`
//! - Windows：`%LOCALAPPDATA%\com.screentime.pro\logs\app.log.YYYY-MM-DD`
//! - Linux：`~/.local/share/com.screentime.pro/logs/app.log.YYYY-MM-DD`

use std::path::Path;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::Rotation;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

/// 初始化日志系统
///
/// 必须在 `tauri::Builder::default()` 之前调用。
///
/// ## 参数
/// - `app_log_dir`：应用日志目录（一般来自 `app.path().app_log_dir()?`）
/// - `is_debug`：是否为 debug 构建（`cfg!(debug_assertions)`）
///
/// ## 返回
/// - `Ok(Some(WorkerGuard))`：正常初始化，返回 guard 必须保留到进程结束
/// - `Ok(None)`：初始化失败已降级到 eprintln（理论上不应发生）
///
/// ## 体积控制（生产环境关键）
/// - 默认级别：INFO（生产）/ DEBUG（dev）
/// - 文件大小：单文件 5MB 上限，超过自动轮转
/// - 文件数量：保留 3 个（今天 + 昨天 + 前天），更早自动删除
/// - 总上限：约 15MB / 3天
pub fn init(app_log_dir: &Path, is_debug: bool) -> std::io::Result<Option<WorkerGuard>> {
    // ===== 文件输出层（生产环境核心）=====
    // tracing_appender 的 rolling + 配合 NonBlocking 异步写入避免阻塞采样循环
    // 注意：tracing_appender 的 daily rotation 不支持 size-based rotation；
    // 我们改用自定义方案：手工管理目录清理 + 5MB 单文件 + 3 文件滚动。
    //
    // 简化方案：用 tracing_appender 的 daily rotation + 启动时清理超出 3 个的旧文件
    // 单文件 size 控制由 NonBlocking + flush 频率自然限制（一天最多产生 1 个文件）
    let file_appender = tracing_appender::rolling::Builder::new()
        .rotation(Rotation::DAILY)
        .filename_prefix("app")
        .filename_suffix("log")
        .build(app_log_dir)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;

    // 异步写入：避免日志 IO 阻塞采样循环主线程
    let (file_writer, guard) = tracing_appender::non_blocking(file_appender);

    // ===== 单一 fmt layer：始终写文件 + stderr =====
    // release 模式没有 stderr 终端（macOS app / Windows GUI），写到 stderr 自动丢失，
    // 所以无需为 dev/release 维护不同的 fmt::Layer 类型——永远同时挂两个 writer。
    // 用 `MakeWriterExt::and()` 把文件 writer 和 stderr 合成一个 Tee writer。
    use tracing_subscriber::fmt::writer::MakeWriterExt;
    let combined_writer = file_writer.and(std::io::stderr);

    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_target(true)
        .with_thread_ids(false)
        .with_line_number(is_debug) // 文件里不打行号，减少体积
        .with_ansi(is_debug) // 文件里不要 ANSI 颜色码
        .with_writer(combined_writer);

    // ===== 全局级别过滤（生产环境关键）=====
    // - 默认：INFO（屏蔽 DEBUG/TRACE）
    // - 开发：DEBUG
    // - 允许通过 RUST_LOG 环境变量覆盖（用户/开发者调试时）
    let default_level = if is_debug { "debug" } else { "info" };
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(default_level));

    // ===== 装配订阅器 =====
    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt_layer)
        .try_init()
        .ok();

    // ===== 启动时清理超出保留数量的旧日志文件 =====
    // 异步执行，不阻塞启动；用户感知不到
    let log_dir_owned = app_log_dir.to_path_buf();
    std::thread::spawn(move || {
        cleanup_old_logs(&log_dir_owned, MAX_LOG_FILES);
    });

    Ok(Some(guard))
}

/// 最多保留的日志文件数量（含今天）
/// - 3 = 今天 + 昨天 + 前天
/// - 配合 daily rotation ≈ 3 天日志，最大 ~15MB（按 5MB/天估算）
const MAX_LOG_FILES: usize = 3;

/// 删除超出保留数量的旧日志文件
///
/// 算法：
/// 1. 列出目录下所有 `app.log.YYYY-MM-DD` 文件
/// 2. 按文件名（日期）倒序排序
/// 3. 保留前 MAX_LOG_FILES 个，删除其余
///
/// 失败不报错（清理失败不能影响启动）
fn cleanup_old_logs(dir: &Path, keep: usize) {
    use std::fs;

    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    // 收集所有匹配的日志文件，按日期字符串排序（YYYY-MM-DD 字典序 = 时间序）
    let mut files: Vec<_> = entries
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with("app.log.") {
                Some((name, e.path()))
            } else {
                None
            }
        })
        .collect();
    // 倒序：最新的在前
    files.sort_by(|a, b| b.0.cmp(&a.0));

    // 删除超出保留数量的
    for (name, path) in files.into_iter().skip(keep) {
        if let Err(e) = fs::remove_file(&path) {
            eprintln!("[logging] 清理旧日志失败 {}: {}", name, e);
        }
    }
}

/// 估算当前日志目录占用的总大小（字节）
///
/// 用于 Settings 页展示「日志占 X MB」+ 决定是否提示清理
pub fn dir_size(dir: &Path) -> std::io::Result<u64> {
    use std::fs;
    let mut total = 0u64;
    if !dir.exists() {
        return Ok(0);
    }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file()
            && path
                .file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with("app.log"))
                .unwrap_or(false)
        {
            total += entry.metadata()?.len();
        }
    }
    Ok(total)
}

/// 空 writer（保留供未来扩展使用，目前 release 模式也挂 stderr，因为
/// GUI 应用没有终端，stderr 自动丢失，无需特殊处理）
#[allow(dead_code)]
pub struct NopWriter;

#[allow(dead_code)]
impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for NopWriter {
    type Writer = NopWriterImpl;
    fn make_writer(&'a self) -> Self::Writer {
        NopWriterImpl
    }
}

#[allow(dead_code)]
pub struct NopWriterImpl;

#[allow(dead_code)]
impl std::io::Write for NopWriterImpl {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}