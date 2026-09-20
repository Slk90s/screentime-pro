//! 统一日志系统（v0.4.2 引入）
//!
//! ## 设计目标
//! - **生产环境体积可控**：默认 INFO 级别、文件按日滚动（无单文件 size cap，按天自然分隔）、最多保留 14 个文件（约 14 天）
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
//! - macOS：`~/Library/Logs/com.screentime.pro/app.YYYY-MM-DD.log`
//! - Windows：`%LOCALAPPDATA%\com.screentime.pro\logs\app.YYYY-MM-DD.log`
//! - Linux：`~/.local/share/com.screentime.pro/logs/app.YYYY-MM-DD.log`
//!
//! ## 修改历史
//! - 2026-08-13 @v0.7.3: 修复 - init 在 build appender 前 create_dir_all 确保日志目录存在，修复 macOS 全新机无目录导致日志系统整体失效、无任何日志文件

use std::path::Path;
use std::sync::OnceLock;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::Rotation;
use tracing_subscriber::filter::filter_fn;
use tracing_subscriber::fmt::writer::MakeWriter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;
// `Layer::with_filter`（注意：不要 `use MakeWriterExt`——它也有同名 `with_filter`，会撞方法解析）
use tracing_subscriber::Layer;

// ===== 版本戳 / 会话 id / 用户行为审计 =====
//
// 这两个全局量在 `init` 里写入（进程启动一次），供 VersionWriter 与 audit 日志使用。
// 用 `OnceLock<&'static str>` 持有泄漏的字符串，避免每次写日志都分配。

/// 真实版本号（取自 `tauri.conf.json`，非 `Cargo.toml` 的 0.1.0 占位）
static APP_VERSION: OnceLock<&'static str> = OnceLock::new();
/// 本次进程会话 id：贯穿一次用户操作（截图→OCR→复制）跨多个事件，便于串联还原
static SESSION_ID: OnceLock<String> = OnceLock::new();

/// 读取当前版本号（供 VersionWriter / plugin-log 格式使用）
pub fn app_version() -> &'static str {
    APP_VERSION.get().copied().unwrap_or("unknown")
}

/// 读取当前会话 id（audit 日志字段）
pub fn session_id() -> &'static str {
    SESSION_ID.get().map(|s| s.as_str()).unwrap_or("")
}

/// 生成并设置会话 id（进程启动调用一次）
fn init_session() {
    if SESSION_ID.get().is_none() {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let pid = std::process::id();
        let _ = SESSION_ID.set(format!("s{nanos:x}{pid:x}"));
    }
}

/// 轻量用户行为审计（追溯用）。
///
/// 记 `action` + 非敏感参数，不含 `window_title`/正文/聊天内容等隐私字段。
/// 写入 target=`audit`，会被独立的 `audit.<date>.log` 文件捕获（见 `init`）。
pub fn audit(action: &str, detail: &str) {
    tracing::info!(target: "audit", session = %session_id(), action = action, "{}", detail);
}

/// 写日志时给每行前缀版本号（`vX.Y.Z `），方便按版本定位问题。
///
/// 选择「包装 Writer」而非 span/字段方案：span 上下文在跨线程 async 任务里会丢，
/// 而 Writer 在每条事件 write 时必然经过，跨线程/异步均可靠。
struct VersionWriter<W: std::io::Write> {
    inner: W,
    version: &'static str,
    at_line_start: bool,
}

impl<W: std::io::Write> std::io::Write for VersionWriter<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        for &b in buf {
            if self.at_line_start {
                self.inner.write_all(self.version.as_bytes())?;
                self.inner.write_all(b" ")?;
                self.at_line_start = false;
            }
            self.inner.write_all(&[b])?;
            if b == b'\n' {
                self.at_line_start = true;
            }
        }
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

/// 初始化日志系统
///
/// 必须在 `tauri::Builder::default()` 之前调用。
///
/// ## 参数
/// - `app_log_dir`：应用日志目录（一般来自 `app.path().app_log_dir()?`）
/// - `is_debug`：是否为 debug 构建（`cfg!(debug_assertions)`）
/// - `version`：应用真实版本号（取自 `tauri.conf.json`），用于日志每行版本戳
///
/// ## 返回
/// - `Ok(Some(vec))`：正常初始化，返回 guard 列表（主日志 + 审计日志）必须保留到进程结束
/// - `Ok(None)`：初始化失败已降级到 eprintln（理论上不应发生）
///
/// ## 体积控制（生产环境关键）
/// - 默认级别：INFO（生产）/ DEBUG（dev）
/// - 文件大小：按天滚动，无单文件 size cap（单日高频日志可能超过 5MB）
/// - 文件数量：保留 14 个（约 14 天，今天往前），更早自动删除
/// - 总上限：约 14 天（每天 1 个文件，靠按天滚动自然分隔）
pub fn init(app_log_dir: &Path, is_debug: bool, version: &str) -> std::io::Result<Option<Vec<WorkerGuard>>> {
    // ===== 确保日志目录存在（关键：否则滚动 appender 在 macOS 全新机上 build 失败 → 整个日志系统失效 =====
    // tracing_appender 的 build(dir) 只 create(true) 创建「文件」，不创建「父目录」；
    // macOS 上 ~/Library/Logs/com.screentime.pro 默认不存在 → init 直接返回 Err → 没有任何日志文件（"mac 无运行日志"根因）。
    // 这里主动 create_dir_all，跨平台都安全（Windows/Linux 同样受益）。
    let _ = std::fs::create_dir_all(app_log_dir);

    // 记录真实版本号与本次会话 id（供 VersionWriter / audit 使用）
    let v: &'static str = Box::leak(version.to_string().into_boxed_str());
    let _ = APP_VERSION.set(v);
    init_session();

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

    // 审计日志独立文件（target=audit），便于「用户操作追溯」单独导出/检索
    let audit_appender = tracing_appender::rolling::Builder::new()
        .rotation(Rotation::DAILY)
        .filename_prefix("audit")
        .filename_suffix("log")
        .build(app_log_dir)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
    let (audit_writer, audit_guard) = tracing_appender::non_blocking(audit_appender);

    // 版本戳：每行前缀 `vX.Y.Z `，方便按版本定位问题（用户要求）
    let version = app_version();

    // ===== 主 fmt layer：始终写文件 + stderr，每行带版本戳 =====
    // release 模式没有 stderr 终端（macOS app / Windows GUI），写到 stderr 自动丢失，
    // 所以无需为 dev/release 维护不同的 fmt::Layer 类型——永远同时挂两个 writer。
    // 用 `MakeWriterExt::and()` 把文件 writer 和 stderr 合成一个 Tee writer。
    // 注意：这里用全路径调用 `and`，不 `use MakeWriterExt`（其 `with_filter` 与 Layer 同名）。
    let combined_writer = tracing_subscriber::fmt::writer::MakeWriterExt::and(file_writer, std::io::stderr);
    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_target(true)
        .with_thread_ids(false)
        .with_line_number(is_debug) // 文件里不打行号，减少体积
        .with_ansi(is_debug) // 文件里不要 ANSI 颜色码
        .with_writer(move || VersionWriter {
            inner: combined_writer.make_writer(),
            version,
            at_line_start: true,
        })
        // 审计事件单独进 audit.<date>.log（见 audit_layer），此处排除，避免主日志重复
        .with_filter(filter_fn(|meta| meta.target() != "audit"));

    // ===== 审计 fmt layer：仅 target=audit 事件写入 audit.<date>.log，同样带版本戳 =====
    let audit_layer = tracing_subscriber::fmt::layer()
        .with_target(false)
        .with_ansi(false)
        .with_writer(move || VersionWriter {
            inner: audit_writer.make_writer(),
            version,
            at_line_start: true,
        })
        .with_filter(filter_fn(|meta| meta.target() == "audit"));

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
        .with(audit_layer)
        .try_init()
        .ok();

    // ===== 清理超出保留数量的旧日志文件 =====
    // ① 启动时清一次；② 运行时每小时再清一次（避免常驻超过保留期后日志堆积、超出保留数）
    let startup_dir = app_log_dir.to_path_buf();
    let keep = MAX_LOG_FILES;
    std::thread::spawn(move || {
        cleanup_old_logs(&startup_dir, keep);
    });
    let periodic_dir = app_log_dir.to_path_buf();
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_secs(3600));
        cleanup_old_logs(&periodic_dir, keep);
    });

    // 返回两个 guard（主日志 + 审计日志），必须持有到进程结束
    Ok(Some(vec![guard, audit_guard]))
}

/// 最多保留的日志文件数量（含今天）
/// - 14 = 约 14 天（今天往前每天 1 个文件）
/// - 配合 daily rotation ≈ 14 天日志；运行时每小时再清理一次超出保留数的旧文件
const MAX_LOG_FILES: usize = 14;

/// 删除超出保留数量的旧日志文件
///
/// 算法：
/// 1. 列出目录下所有 `app.YYYY-MM-DD.log` 滚动日志文件
/// 2. 按日期字符串（YYYY-MM-DD）倒序排序（字典序 = 时间序）
/// 3. 保留前 MAX_LOG_FILES 个，删除其余
///
/// 失败不报错（清理失败不能影响启动）
fn cleanup_old_logs(dir: &Path, keep: usize) {
    use std::fs;

    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    // 收集所有滚动日志文件（app.<date>.log），按日期排序
    let mut files: Vec<_> = entries
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            // 仅滚动日志走日期清理；tauri-plugin-log 的 ScreenTime Pro.log 是
            // 单文件、无日期，保留不动（其体积控制由 tauri-plugin-log 自己负责）。
            if name.starts_with("app.") && name.ends_with(".log") {
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

/// 识别「本程序写入的日志文件」
///
/// 真实文件名（tracing_appender DAILY + prefix="app", suffix="log"）：
///   `app.YYYY-MM-DD.log`
/// `tauri-plugin-log` 默认输出（写到 `app_log_dir`）：
///   `<productName>.log` → 如 `ScreenTime Pro.log`
///
/// **历史踩坑**（v0.6.2-beta.16）：旧版 `dir_size()` 用
/// `n.starts_with("app.log")` 过滤，但真实文件 `app.2026-07-25.log`
/// 第三字符是 `.` 而不是 `l`，filter 漏掉 → `get_log_size` 命令恒返回 0，
/// Settings 页始终显示「0 B」。同时 `cleanup_old_logs()` 的
/// `starts_with("app.log.")` 也错（差一个字符）。
///
/// **修正策略**：仅信任我们生成的两类文件名，**不**把目录里其他用户的
/// 文件（比如手动拖入的 `notes.log`）也算成我们的日志占用：
///   - 滚动日志：`app.<YYYY-MM-DD>.log`（日期恰好 10 字符 + 前后两个点）
///   - 插件日志：`ScreenTime Pro.log`（productName 拼 `.log`）
/// 识别「本程序写入的日志文件」（导出日志命令 export_logs 复用本函数，保证与 dir_size 一致）
pub fn is_our_log_file(name: &str) -> bool {
    // 1) 滚动日志 app.<YYYY-MM-DD>.log
    if let Some(rest) = name.strip_prefix("app.") {
        // 期望形如 "2026-07-25.log"（10 + 4 = 14 字符）
        if rest.len() == 14 && rest.ends_with(".log") {
            let date_part = &rest[..10];
            // YYYY-MM-DD 形式：4位-2位-2位，3 处 '-' 中后两处为分隔
            let bytes = date_part.as_bytes();
            if bytes[4] == b'-' && bytes[7] == b'-' {
                let all_digits = bytes
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| *i != 4 && *i != 7)
                    .all(|(_, c)| c.is_ascii_digit());
                if all_digits {
                    return true;
                }
            }
        }
    }
    // 2) tauri-plugin-log 默认输出
    if name == "ScreenTime Pro.log" {
        return true;
    }
    false
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
        if let Some(name) = entry.path().file_name().and_then(|n| n.to_str()) {
            if entry.path().is_file() && is_our_log_file(name) {
                total += entry.metadata()?.len();
            }
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