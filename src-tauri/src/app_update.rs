//! 应用内自动升级（v1.2.11 新增）。
//!
//! # 这个模块为什么不复用 commands.rs 里已有的版本检查
//!
//! 两者**目的不同，不是重复实现**：
//!   - `commands::check_for_update` / `auto_check_update`（v1.2.4 起）：**只告知**。
//!     三通道（GitHub atom → REST → Gitee）抓版本号，前端弹横幅 + 跳浏览器手下载。
//!     它不碰安装包，也不校验签名。
//!   - 本模块（Tauri 官方 updater）：**真正替换**。拉 `latest.json` → 下载安装包 →
//!     用 ed25519 公钥验签 → 交给平台安装器。签名是强制项，不可关闭。
//!
//! 保留前者的原因：macOS 不参与应用内升级（见下），mac 用户只能走「浏览器下载」，
//! 那条链路仍需要能「告知有新版本」。所以两套并存，**不要**以为可以删掉旧的。
//!
//! # 平台策略（每一行都对应一条被源码证实的事实）
//!
//! | 平台 | 应用内升级 | 依据 |
//! |------|-----------|------|
//! | Windows (nsis) | ✅ 启用 | `install_inner` 用 ShellExecuteW 拉起安装包 |
//! | Linux (AppImage) | ✅ 启用 | `install_appimage` 原地覆盖自身，无需提权 |
//! | Linux (deb/rpm) | ❌ 关闭 | 走 pkexec/zenity/sudo 提权，每次弹授权框；应由包管理器接管 |
//! | macOS | ❌ 关闭 | 本项目 ad-hoc 签名（CI 无 APPLE_SIGNING_IDENTITY），签名身份每次构建都变 |
//!
//! ## 为什么 macOS 必须关闭
//!
//! 两个独立原因，任一条都足够：
//! ① **签名身份不稳定**。ad-hoc 签名的 cdhash 每次构建都不同，而 TCC 授权记录是按
//!    cdhash 绑定的 —— 用户升级后，屏幕录制 / 辅助功能授权会**全部失效**，
//!    又回到「系统设置里是开的、应用说没权限」的死循环（v1.2.11 刚修的就是这个坑）。
//!    自动升级会让这个坑**每升级一次复发一次**，比不自动升级更糟。
//! ② **缺 target 会硬报错**。若 macOS 在 latest.json 里没有 `darwin-aarch64` 条目，
//!    `check()` 返回 `Err(TargetsNotFound)` 而不是 `Ok(None)`（updater.rs:608-638），
//!    于是每次启动都往日志里刷一条错误。所以不产出 mac 更新包时，就不能让 mac 去查。
//!
//! ## 为什么 deb/rpm 关闭
//!
//! `install_inner`（updater.rs:1042-1051）按 `bundle_type()` 分流：AppImage 原地覆盖；
//! Deb/Rpm 则写临时包后 `pkexec dpkg -i` / `rpm -U`，失败退 zenity/kdialog 取密码，
//! 再退终端 sudo。**每次升级都要过一次提权框**，且依赖 polkit 可用。deb/rpm 用户
//! 本来就有包管理器这条正路，不该由应用越权代劳。
//!
//! # 三个必须知道的运行期行为
//!
//! 1. **Windows 装完进程立刻消失**。`install_inner` 在 ShellExecuteW 之后直接
//!    `std::process::exit(0)`（updater.rs:876），所以 `install()` **永不返回**。
//!    前端因此不能写「安装成功」的提示 —— 那句代码永远执行不到。见 `install_update`
//!    里对返回值的处理。
//! 2. **Linux 装完不会自动重启**。官方文档明确 "You need to relaunch the app"。
//!    所以 `install_update` 在 Linux 分支装完要自己 `app.restart()`。
//! 3. **开发态 `bundle_type()` 是 `None`**（二进制 patch 只在打包时写入
//!    `__TAURI_BUNDLE_TYPE`）。所以 `cargo run` 下 Linux 会被判为「非 AppImage」
//!    从而关闭 —— 这是**正确**的保守行为，不是 bug。

use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;
use tauri_plugin_updater::UpdaterExt;

// `bundle_type()` / `BundleType` 在 **Windows 与 Linux** 两个平台的分支里都用，
// 只有 macOS 用不到（macOS 直接拒绝）。所以把 cfg 收窄为「非 macOS」而不是只留 linux ——
// 写成 `#[cfg(target_os = "linux")]` 会让 Windows 分支引用到未导入的符号。
#[cfg(not(target_os = "macos"))]
use tauri::utils::{config::BundleType, platform::bundle_type};

/// 是否有一次下载/安装正在进行。
///
/// 为什么需要：`download_and_install` 可能持续几十秒（安装包 20+ MB），而前端按钮
/// 在下载期间若被连点，会并发起第二个下载 → 两份字节流、两次落盘、Linux 上两次覆盖
/// 自身（可能把 AppImage 写坏）。用一个进程级标志把并发挡在入口处。
///
/// 用 `AtomicBool` 而不是 `Mutex<()>`：这里只需要「有/无」的布尔语义，不需要排队。
/// `compare_exchange` 保证「检查 + 占位」是**一步原子操作** —— 若写成
/// `if !BUSY.load() { BUSY.store(true) }`，两个线程可以同时通过检查。
static BUSY: AtomicBool = AtomicBool::new(false);

/// 占用 BUSY 的 RAII 守卫：即便中途 panic 或提前 return 也会释放。
///
/// 手写 `BUSY.store(false)` 的写法只要漏掉任意一条返回路径（`install_update` 里
/// 有 5 条），就会永久卡在「安装进行中」，用户重启应用才能恢复。
struct BusyGuard;

impl BusyGuard {
    /// 尝试占用。已有任务在跑时返回 `None`。
    fn acquire() -> Option<Self> {
        BUSY.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .ok()
            .map(|_| BusyGuard)
    }
}

impl Drop for BusyGuard {
    fn drop(&mut self) {
        BUSY.store(false, Ordering::SeqCst);
    }
}

/// 当前平台是否支持应用内升级。
///
/// 返回值同时用作**前端是否渲染升级按钮**的依据 —— 不支持时前端根本不显示入口，
/// 而不是显示了再点出一句错误。所以这里返回 `Err` 的文案是给日志和极端情况兜底的，
/// 正常情况下用户看不到。
///
/// # 为什么 dev 一律判不支持
///
/// `createUpdaterArtifacts` 只在 `tauri build` 时生效，`cargo run` 产出的二进制
/// 没有 `__TAURI_BUNDLE_TYPE` patch（`bundle_type()` 返回 `None`），也没有可被
/// 覆盖的安装包外壳 —— Linux 上「原地覆盖自身」更是会去覆盖
/// `target/debug/screentime-pro`，把开发目录写坏。
/// 所以用 `cfg!(debug_assertions)` 直接拦掉，而不是靠 `bundle_type()` 兜底。
fn install_supported() -> Result<(), String> {
    if cfg!(debug_assertions) {
        return Err(
            "开发模式下不支持应用内升级（无签名产物，且会覆盖 target/ 下的调试二进制）".to_string(),
        );
    }

    #[cfg(target_os = "macos")]
    {
        // 见模块头注释「为什么 macOS 必须关闭」。
        Err("macOS 版暂不支持应用内升级，请从发布页下载新版安装包".to_string())
    }

    #[cfg(target_os = "linux")]
    {
        match bundle_type() {
            Some(BundleType::AppImage) => Ok(()),
            // None 也会走到这里：直接从 target/release 跑二进制、或被解包运行。
            Some(BundleType::Deb) => {
                Err("deb 包请用包管理器升级（apt upgrade），应用内升级仅支持 AppImage".to_string())
            }
            Some(BundleType::Rpm) => {
                Err("rpm 包请用包管理器升级（dnf upgrade），应用内升级仅支持 AppImage".to_string())
            }
            _ => Err("应用内升级仅在 AppImage 包中可用".to_string()),
        }
    }

    // Windows：nsis 与 msi 都由 updater 支持（前者 ShellExecuteW 拉起 setup.exe，
    // 后者 msiexec /i）。本项目只产 nsis，但不必因此拒绝 msi。
    //
    // 仍然校验 bundle_type()：若二进制不是从安装包启动（比如有人把 target/release
    // 下的 exe 单独拷出来跑），`bundle_type()` 是 None，此时「装完自动重启」会拉起
    // 一个错误的路径。返回 Ok 让用户点了升级却装不上，不如直接说明。
    #[cfg(target_os = "windows")]
    {
        match bundle_type() {
            Some(BundleType::Nsis) | Some(BundleType::Msi) => Ok(()),
            _ => {
                Err("应用内升级仅在安装版（NSIS / MSI）中可用，便携运行请手动下载新版".to_string())
            }
        }
    }

    // 其它平台（iOS/Android/BSD…）不参与。
    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    {
        Err("当前平台不支持应用内升级".to_string())
    }
}

/// 前端可见的升级状态。
#[derive(Serialize, Clone)]
pub struct AppUpdate {
    /// 当前运行版本。
    pub current: String,
    /// 远端版本。
    pub version: String,
    /// 释放说明（来自 latest.json 的 `notes`）。
    pub notes: String,
    /// 发布日（RFC3339；latest.json 未写时为空串）。
    pub date: String,
}

/// 查询是否有可安装的新版本。
///
/// 返回 `Ok(None)` 表示「已是最新」或「本平台不支持应用内升级」——
/// 两者对 UI 是同一个结果（不显示横幅），所以合并成一个分支，前端少一层判断。
///
/// ⚠️ 这里**吞掉**了「不支持」的 Err，只用 warn 记日志。原因：本命令会在启动时
/// 被自动调用一次，若把「macOS 不支持」当错误抛给前端，每次启动都会在主界面弹一条
/// 提示 —— 而 mac 用户本来就该走浏览器下载，那条路径由旧的 `auto_check_update` 负责。
#[tauri::command]
pub async fn check_app_update(app: tauri::AppHandle) -> Result<Option<AppUpdate>, String> {
    if let Err(why) = install_supported() {
        tracing::debug!(reason = %why, "应用内升级：本平台未启用，跳过检查");
        return Ok(None);
    }

    let updater = app.updater().map_err(|e| {
        tracing::warn!(error = %e, "应用内升级：updater 初始化失败（检查 tauri.conf.json 的 pubkey/endpoints）");
        format!("初始化升级组件失败：{e}")
    })?;

    match updater.check().await {
        Ok(Some(update)) => {
            tracing::info!(
                current = %update.current_version,
                latest = %update.version,
                "应用内升级：发现新版本"
            );
            Ok(Some(AppUpdate {
                current: update.current_version.clone(),
                version: update.version.clone(),
                notes: update.body.clone().unwrap_or_default(),
                date: update.date.map(|d| d.to_string()).unwrap_or_default(),
            }))
        }
        Ok(None) => {
            tracing::info!("应用内升级：已是最新版本");
            Ok(None)
        }
        // 不折叠成 Ok(None)：检查失败必须让用户看到原因（网络 / 签名 / 缺 target）。
        // 与 auto_check_update 的策略**故意相反** —— 那条是后台静默检查，不能打扰用户；
        // 这条是用户点了按钮，必须给明确结论。
        Err(e) => {
            tracing::warn!(error = %e, "应用内升级：检查失败");
            Err(format!("检查更新失败：{e}"))
        }
    }
}

/// 下载并安装新版本。
///
/// `on_progress` 由前端传入 channel 名（本仓库既有约定：`Channel<String>`）。
/// 这里用 `tauri::ipc::Channel<DownloadProgress>` 更直接 —— 见下方参数。
///
/// # 返回值语义（**平台不一致，调用方必须知道**）
///
/// - **Windows**：本函数**通常不会返回**。`install()` 内部拉起 setup.exe 后
///   立刻 `std::process::exit(0)`，整个进程消失。所以返回 `Ok(())` 的那条分支
///   实际只覆盖「下载完成但安装器未能启动」的极端情况。
/// - **Linux**：正常返回 `Ok(())`，且**函数内部已经调用了 `app.restart()`**。
///   前端收到 `Ok(())` 时进程正在重启，不要再更新 UI。
#[tauri::command]
pub async fn install_app_update(
    app: tauri::AppHandle,
    on_progress: tauri::ipc::Channel<DownloadProgress>,
) -> Result<(), String> {
    install_supported()?;

    let _guard = BusyGuard::acquire().ok_or_else(|| "已有一次更新正在进行，请稍候".to_string())?;

    let updater = app
        .updater()
        .map_err(|e| format!("初始化升级组件失败：{e}"))?;

    // check() 在这里**必须重做一次**，不能复用 check_app_update 的结果：
    // Updater 不持有「上次查到的 Update」句柄，且两次调用之间远端可能已变。
    // 代价是多一次 latest.json 请求（几 KB），换来的是不会拿着过期 URL 去下载。
    let update = match updater.check().await {
        Ok(Some(u)) => u,
        Ok(None) => return Err("已是最新版本，无需更新".to_string()),
        Err(e) => return Err(format!("检查更新失败：{e}")),
    };

    tracing::info!(version = %update.version, "应用内升级：开始下载");

    // 进度回调：downloaded 是**累计已下载字节**（不是增量），total 来自
    // Content-Length，服务端不给分块长度时为 None。
    let progress = on_progress.clone();
    let chunk_cb = move |downloaded: usize, total: Option<u64>| {
        // send 失败（前端窗口已关闭）不是错误，直接忽略 —— 不能因为发不出进度
        // 就中断下载。Channel::send 在接收端 drop 后返回 Err。
        let _ = progress.send(DownloadProgress {
            downloaded: downloaded as u64,
            total,
        });
    };

    let finish_cb = move || {
        tracing::info!("应用内升级：下载完成，开始校验签名并安装");
    };

    // download_and_install = download（含 ed25519 验签）+ install。
    // ⚠️ 验签发生在 `download()` 内部、`install()` 之前（updater.rs:742-748），
    // 也就是说**签名不对时根本不会走到安装**，磁盘上也不会留下可执行文件。
    match update.download_and_install(chunk_cb, finish_cb).await {
        Ok(()) => {
            tracing::info!("应用内升级：安装流程已提交");
            // Linux：装完必须自己重启（官方文档 "You need to relaunch the app"）。
            // Windows：走不到这里（进程已被 exit(0) 干掉）。
            #[cfg(target_os = "linux")]
            {
                tracing::info!("应用内升级：重启以加载新版本");
                app.restart();
            }
            Ok(())
        }
        Err(e) => {
            tracing::error!(error = %e, "应用内升级：下载或安装失败");
            Err(format!("下载或安装失败：{e}"))
        }
    }
}

/// 下载进度（推给前端渲染进度条）。
#[derive(Serialize, Clone)]
pub struct DownloadProgress {
    /// 累计已下载字节。
    pub downloaded: u64,
    /// 总字节数；服务端未给 Content-Length 时为 `null`。
    pub total: Option<u64>,
}

/// 供前端判断「是否该显示升级入口」。
///
/// 单独给一个命令而不是让前端猜，是因为判断依据在 Rust 侧（`bundle_type()` 是
/// 二进制里 patch 进去的静态变量，前端拿不到）。
#[tauri::command]
pub fn app_update_supported() -> bool {
    install_supported().is_ok()
}
