//!
//! screenshot/macos.rs
//! macOS 截图专属逻辑（v0.9.3，2026-09-20）：屏幕录制授权闸门 + 授权失效根因诊断。
//!
//! ## 为什么单独拆一个文件
//! Windows 侧的截图链路（xcap → 遮罩窗 → 合成）是好的，macOS 的问题**全在授权**：
//! macOS 从 10.15 起把「抓别的应用的窗口内容」关进了 TCC（屏幕录制权限），
//! 而**未授权时抓屏 API 不报错** —— 它返回一张「只有桌面壁纸」的图。
//! 于是同一段抓屏代码在 Windows 上完全正常，在 macOS 上却会「截完应用全消失、只剩桌面」。
//! 这段平台差异（预检 API / 授权请求 / 二进制身份 / 诊断话术）全收在本文件里，
//! 让 `mod.rs` 保持纯跨平台流程。
//!
//! ## 本模块解决的两个具体问题
//!
//! **① `CGPreflightScreenCaptureAccess()` 会返回「过期的 false」。**
//! Apple 已知行为：该函数一旦返回 false，**在同一个进程内可能一直返回 false**，
//! 即使用户已经在系统设置里勾上了授权；而真正的抓屏操作其实已经生效。
//! v0.9.2 及以前只拿它当唯一闸门 → 会把「本来能截」的情况也拦下，
//! 用户看到的就是「我明明授权了，软件还说没权限」。本模块引入**独立第二信号**
//! [`TitlesProbe`]：窗口标题与窗口内容走的是同一张 TCC 授权，标题能读到 = 内容就能读到。
//! 两个信号任一为「有」即放行，不再被过期的预检卡死。
//!
//! **② 用户反复反馈「授权了还是不行」，光靠一句话提示解释不了。**
//! 真正的主因是**二进制身份不匹配**（见下），所以失败时把「查到的证据」直接写进错误文案：
//! 运行路径 / App Translocation / 下载隔离标记 / 代码签名状态（ad-hoc 或未签名）。
//!
//! ## 🔴 授权为什么会「反复失效」（本项目的 P0 根因，务必先读这段）
//! macOS 的 TCC 授权是按**二进制身份**记账的，身份 ≈ 可执行文件路径 + 代码签名指纹。
//! 本仓库没有配置 `signingIdentity`（见 `tauri.conf.json` 的 `bundle.macOS`），
//! CI 产出的 macOS 包是 **ad-hoc 签名**（Apple Silicon 上二进制必须有签名才能执行，
//! 链接器会自动打一个 ad-hoc 签名）。ad-hoc 签名的指纹里含 **cdhash**，
//! 而 cdhash 每次重新构建都会变 → 每个新版本在系统眼里都是「一个全新的 App」：
//!   - 系统设置里**旧的开关记录还在**（显示为开），但它对应的是上一版的指纹，对本版无效；
//!   - 用户以为自己授权了，实际没有。
//! Apple DTS 对此的说明（TN3127 / DevForums 819406）：ad-hoc 签名下 «the system treats
//! each build as a new app»。**根治只能靠 Developer ID 签名 + 公证**；本文件能做的，
//! 是把这条根因明确告诉用户 + 让「移除旧记录 → 重新授权」变成一键操作。
//!
//! 修改历史：
//!   - 2026-09-20 @v0.9.3: 初始创建 —— 从 `tracker/macos.rs::ensure_screen_capture_ready`
//!     迁入并重写：新增 `TitlesProbe` 独立信号（修「过期预检误拦」）、`collect` 采集完整
//!     诊断证据、`reset_permission` 一键 `tccutil reset`、错误文案按实际证据分档。
//!   - 2026-09-20 @v0.9.4: 改造 —— ① 失败文案**精简**：弹窗只收一句话 + 结构化原因码
//!     （`__PERM__:<reason>`，用户反馈 v0.9.3 的长文案「太多太丑」），完整证据改走日志；
//!     ② 新增权限状态快照 `status_snapshot` / 主动请求 `request_permission` /
//!     打开设置 `open_permission_settings` / 启动预请求 `schedule_startup_permission_request`，
//!     配合前端紧凑授权弹窗（轮询 + 引导重启）与启动期预登记。
//!

use core_foundation_sys::array::{CFArrayGetCount, CFArrayGetValueAtIndex, CFArrayRef};
use core_foundation_sys::base::{CFGetTypeID, CFIndex, CFRelease, CFTypeRef};
use core_foundation_sys::dictionary::{CFDictionaryGetValue, CFDictionaryRef};
use core_foundation_sys::number::{CFNumberGetTypeID, CFNumberGetValue, CFNumberRef, kCFNumberSInt64Type};
use core_foundation_sys::string::{
    CFStringGetCString, CFStringGetLength, CFStringGetTypeID, CFStringRef, kCFStringEncodingUTF8,
};
use core_graphics::window::{
    CGWindowListCopyWindowInfo, CGWindowListOption, kCGNullWindowID,
    kCGWindowListExcludeDesktopElements, kCGWindowLayer, kCGWindowListOptionOnScreenOnly,
    kCGWindowName, kCGWindowOwnerPID,
};
use std::os::raw::{c_char, c_void};
use tauri::{AppHandle, Emitter};

/// 本应用的 bundle id —— 必须与 `tauri.conf.json` 的 `identifier` 逐字一致，
/// 否则 `tccutil reset` 会去重置一个不存在的条目（失败但不报错，白忙一场）。
pub const BUNDLE_ID: &str = "com.screentime.pro";

/// 独立第二信号：能不能读到**别的进程**的窗口标题。
///
/// 为什么它能代表抓屏权限：macOS 把「窗口标题」和「窗口内容」放在同一张
/// TCC（`kTCCServiceScreenCapture`）授权之下 —— 未授权时
/// `CGWindowListCopyWindowInfo` 依然返回窗口列表（位置/大小/所属进程都有），
/// 但会把 `kCGWindowName` **整条抹掉**。所以「读不到任何外部窗口标题」
/// 等价于「抓不到任何窗口内容」，而「读得到」就说明授权**确实生效**。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TitlesProbe {
    /// 读到了外部进程的窗口标题 → 授权确实生效
    Readable,
    /// 屏上有外部普通窗口，但一个标题都读不到 → 标题被隐私机制抹掉，授权未生效
    Redacted,
    /// 屏上没有可判定的外部窗口（例如刚启动只有桌面）→ 该信号不可用，别下结论
    Inconclusive,
}

/// 一次「屏幕录制」授权检查采集到的全部证据（日志与错误文案共用）
#[derive(Debug, Clone)]
pub struct PermissionSignals {
    /// 官方预检 `CGPreflightScreenCaptureAccess()`。⚠️ 可能是**过期的 false**，不能单独用它下结论
    pub preflight: bool,
    /// 独立第二信号
    pub titles: TitlesProbe,
    /// 当前进程的可执行文件路径（TCC 就是按这个 + 签名指纹找授权的）
    pub exe_path: String,
    /// 从 exe 路径回溯出的 `.app` 路径（`tccutil` / `xattr` 诊断要用）
    pub bundle_path: Option<String>,
    /// 是否处于 App Translocation（从 DMG／下载目录直启时被挪到随机只读路径）
    pub translocated: bool,
    /// `.app` 是否带 com.apple.quarantine（浏览器下载的包默认会带）
    pub quarantined: bool,
    /// `Some(true)` = ad-hoc 签名或完全未签名（**授权反复失效的根因**）；`None` = 没查出来
    pub adhoc_unsigned: Option<bool>,
    /// 签名摘要（Identifier / TeamIdentifier / Authority / Signature），写进日志
    pub signature_summary: String,
}

impl PermissionSignals {
    /// 是否放行抓屏。
    ///
    /// 两个信号**取或**：预检可能是过期的 false，而标题探针一旦读到就说明授权真的生效了。
    /// 反过来，标题探针的 `Redacted` 只代表「没读到」，不能推翻预检为 true 的情况
    /// （例如屏幕上确实没有带标题的外部窗口），所以不放行逻辑里做与运算。
    pub fn permitted(&self) -> bool {
        self.preflight || self.titles == TitlesProbe::Readable
    }

    /// 单行诊断摘要（写日志用；错误文案另有 [`build_error`]）
    pub fn summary(&self) -> String {
        format!(
            "preflight={} titles={:?} translocated={} quarantined={} adhoc_unsigned={:?} exe={}",
            self.preflight,
            self.titles,
            self.translocated,
            self.quarantined,
            self.adhoc_unsigned,
            self.exe_path
        )
    }
}

// ===== 证据采集 =====

/// 采集一次完整的授权证据（含子进程诊断，失败路径才会用到，正常路径只有 preflight + 窗口枚举）
pub fn collect() -> PermissionSignals {
    let preflight = crate::tracker::macos::is_screen_capture_trusted();
    let exe_path = std::env::current_exe()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|e| format!("<无法获取: {e}>"));
    let translocated = exe_path.contains("/AppTranslocation/");

    // 只有预检为 false 时才值得花钱去做子进程诊断（正常路径零开销）
    if preflight {
        return PermissionSignals {
            preflight,
            titles: TitlesProbe::Readable,
            exe_path,
            bundle_path: None,
            translocated,
            quarantined: false,
            adhoc_unsigned: None,
            signature_summary: String::new(),
        };
    }

    let bundle_path = bundle_path_from_exe(&exe_path);
    let (adhoc_unsigned, signature_summary) = signature_info(bundle_path.as_deref());
    let quarantined = bundle_path
        .as_deref()
        .map(has_quarantine)
        .unwrap_or(false);

    PermissionSignals {
        preflight,
        titles: unsafe { probe_foreign_titles() },
        exe_path,
        bundle_path,
        translocated,
        quarantined,
        adhoc_unsigned,
        signature_summary,
    }
}

/// 从 `/Applications/X.app/Contents/MacOS/X` 回溯出 `/Applications/X.app`。
///
/// ⚠️ 开发态（`cargo run` / `tauri dev`）的 exe 在 `target/debug/` 下，回溯不出 `.app`，
/// 此时返回 `None` —— 调用方必须容忍（诊断信息里就不写 bundle 相关项）。
fn bundle_path_from_exe(exe: &str) -> Option<String> {
    let exe = std::path::Path::new(exe);
    let macos_dir = exe.parent()?; // .../Contents/MacOS
    let contents_dir = macos_dir.parent()?; // .../Contents
    let app_dir = contents_dir.parent()?; // .../X.app
    if app_dir.extension().and_then(|e| e.to_str()) != Some("app") {
        return None;
    }
    Some(app_dir.display().to_string())
}

/// 探针：屏幕上是否存在「读得到标题」的外部普通窗口。
///
/// 只统计 **`kCGWindowLayer == 0`** 的窗口 —— Dock、菜单栏、通知中心、壁纸、
/// 各类悬浮层都在其它 layer 上，它们的标题**不受**屏幕录制授权管控，
/// 不排除掉会造成「无授权却判定为已授权」的假阳性。
unsafe fn probe_foreign_titles() -> TitlesProbe {
    let me = std::process::id() as i64;
    let option: CGWindowListOption =
        kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements;
    let list: CFArrayRef = CGWindowListCopyWindowInfo(option, kCGNullWindowID);
    if list.is_null() {
        return TitlesProbe::Inconclusive;
    }
    let count = CFArrayGetCount(list);
    let mut foreign_normal_windows = 0usize;
    let mut titled = 0usize;
    for i in 0..count {
        let dict = CFArrayGetValueAtIndex(list, i) as CFDictionaryRef;
        if dict.is_null() {
            continue;
        }
        // layer != 0 的全部跳过（Dock / 菜单栏 / 悬浮层 / 壁纸）
        match dict_i64(dict, kCGWindowLayer as *const c_void) {
            Some(0) => {}
            _ => continue,
        }
        let owner_pid = match dict_i64(dict, kCGWindowOwnerPID as *const c_void) {
            Some(v) => v,
            None => continue,
        };
        if owner_pid == me {
            continue; // 自家窗口不算（遮罩窗本身就在 layer 0）
        }
        foreign_normal_windows += 1;
        if let Some(name) = dict_string(dict, kCGWindowName as *const c_void) {
            if !name.trim().is_empty() {
                titled += 1;
            }
        }
    }
    // CGWindowListCopyWindowInfo 的 "Copy" 语义 = 返回 +1 引用，调用方负责释放。
    // 不释放的话每次截图尝试都会漏一个 CFArray —— 这是 v0.9.2 之前
    // `tracker::macos::get_foreground_window_title` 里就存在的小泄漏，这里一并做对。
    CFRelease(list as CFTypeRef);

    if titled > 0 {
        TitlesProbe::Readable
    } else if foreign_normal_windows > 0 {
        TitlesProbe::Redacted
    } else {
        TitlesProbe::Inconclusive
    }
}

/// 从窗口信息字典里取一个整数字段；类型不是 CFNumber 或取不到则返回 None。
///
/// 先做 `CFGetTypeID` 校验再强转：窗口信息字典里的 value 理论上是固定类型，
/// 但直接裸转指针一旦类型不符就是 UB（崩在系统框架里，堆栈完全不可读）。
unsafe fn dict_i64(dict: CFDictionaryRef, key: *const c_void) -> Option<i64> {
    let value: CFTypeRef = CFDictionaryGetValue(dict, key);
    if value.is_null() || CFGetTypeID(value) != CFNumberGetTypeID() {
        return None;
    }
    let mut out: i64 = 0;
    if CFNumberGetValue(
        value as CFNumberRef,
        kCFNumberSInt64Type,
        &mut out as *mut i64 as *mut c_void,
    ) {
        Some(out)
    } else {
        None
    }
}

/// 从窗口信息字典里取一个字符串字段；类型不是 CFString / 空串则返回 None。
unsafe fn dict_string(dict: CFDictionaryRef, key: *const c_void) -> Option<String> {
    let value: CFTypeRef = CFDictionaryGetValue(dict, key);
    if value.is_null() || CFGetTypeID(value) != CFStringGetTypeID() {
        return None;
    }
    let cf_str = value as CFStringRef;
    let len = CFStringGetLength(cf_str);
    if len <= 0 {
        return None;
    }
    // UTF-8 最坏每个 UniChar 占 4 字节，+1 给结尾 NUL
    let mut buf: Vec<u8> = vec![0u8; (len as usize) * 4 + 1];
    let ok = CFStringGetCString(
        cf_str,
        buf.as_mut_ptr() as *mut c_char,
        buf.len() as CFIndex,
        kCFStringEncodingUTF8,
    );
    if ok == 0 {
        return None;
    }
    let cstr = std::ffi::CStr::from_ptr(buf.as_ptr() as *const c_char);
    Some(cstr.to_string_lossy().to_string())
}

/// 读代码签名状态：`(是否 ad-hoc 或未签名, 摘要)`。
///
/// 走 `/usr/bin/codesign` 子进程而不是 Security.framework 的 `SecCodeCopySigningInformation`：
/// 后者要多引一份 security-framework 依赖、还要处理 CFDictionary 解包，
/// 而这里只在**失败路径**上跑一次，子进程的几十毫秒完全可接受。
fn signature_info(bundle: Option<&str>) -> (Option<bool>, String) {
    let Some(bundle) = bundle else {
        return (None, "开发态运行（非 .app 包），跳过签名检查".to_string());
    };
    let out = std::process::Command::new("/usr/bin/codesign")
        .args(["-dvvv", bundle])
        .output();
    let Ok(out) = out else {
        return (None, format!("无法执行 codesign: {}", out.unwrap_err()));
    };
    // codesign 的信息全部写 stderr（stdout 通常为空），两边都收一下保险
    let mut text = String::from_utf8_lossy(&out.stderr).to_string();
    text.push_str(&String::from_utf8_lossy(&out.stdout));

    if !out.status.success() {
        if text.to_lowercase().contains("not signed") {
            return (Some(true), "未签名（code object is not signed at all）".to_string());
        }
        return (
            None,
            format!(
                "codesign 查询失败：{}",
                text.trim().lines().last().unwrap_or("无输出")
            ),
        );
    }
    // ad-hoc 签名的标志：`Signature=adhoc`，且没有 Authority= 行（没有证书链）
    let adhoc = text.contains("Signature=adhoc");
    let summary = text
        .lines()
        .filter(|l| {
            l.starts_with("Identifier=")
                || l.starts_with("TeamIdentifier=")
                || l.starts_with("Authority=")
                || l.starts_with("Signature=")
        })
        .collect::<Vec<_>>()
        .join(" | ");
    let summary = if summary.is_empty() {
        "codesign 未给出签名详情".to_string()
    } else {
        summary
    };
    (Some(adhoc), summary)
}

/// `.app` 是否带下载隔离标记（浏览器下载的包默认会带，会触发 App Translocation）
fn has_quarantine(bundle: &str) -> bool {
    std::process::Command::new("/usr/bin/xattr")
        .args(["-p", "com.apple.quarantine", bundle])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

// ===== 对外的两个动作 =====

/// 截图前的「屏幕录制」授权闸门。
///
/// `Ok(())` = 可以抓屏；`Err(原因)` = 不可抓屏，原因**可直接展示给用户**
/// （前端就是拿这个字符串弹提示的，所以文案按「用户能照着做」的顺序写）。
///
/// ⚠️ **会阻塞**：首次会调 `CGRequestScreenCaptureAccess()` 弹系统授权框并同步等用户点击，
/// 可能数秒～数十秒 → 调用方**必须**放在阻塞线程里（`spawn_blocking`）。
///
/// 放行判定见 [`PermissionSignals::permitted`]：预检为真 **或** 标题探针读到即可放行。
/// v0.9.2 及以前只看预检，会把「授权已生效但预检返回过期 false」的情况误拦下来，
/// 这是「我明明授权了它还说没权限」的直接成因之一。
pub fn ensure_ready() -> Result<(), String> {
    let mut sig = collect();
    if sig.permitted() {
        if !sig.preflight && sig.titles == TitlesProbe::Readable {
            // 预检说谎了，但授权确实生效 —— 记一条 warn 便于日后统计这个坑的复现率
            tracing::warn!(signals = %sig.summary(), "预检返回 false，但窗口标题可读 → 判定授权已生效（Apple 已知的过期预检行为）");
        }
        return Ok(());
    }

    // 预检说没有、独立信号也说没有 → 主动请求一次。
    // 首次未表态时：会把本应用登记进「系统设置 → 隐私与安全性 → 屏幕录制」列表 + 弹系统框；
    // 用户此前点过「不允许」则立即返回 false 且**不再弹框**（此时只能引导去系统设置手动勾）。
    crate::tracker::macos::request_screen_capture_access();
    sig = collect();
    if sig.permitted() {
        tracing::info!(signals = %sig.summary(), "请求授权后已生效，继续抓屏");
        return Ok(());
    }

    let msg = build_error(&sig);
    tracing::warn!(signals = %sig.summary(), evidence = %msg, "屏幕录制授权未生效，拒绝抓屏");
    crate::logging::audit("screenshot_permission_denied", &sig.summary());
    Err(short_error(&sig))
}

/// 组装**日志用**的完整证据（v0.9.4 起不再直接展示给用户——用户反馈「太多太丑」；
/// 完整证据走 `tracing::warn!` 的 evidence 字段进日志文件，排查时看日志即可）。
fn build_error(sig: &PermissionSignals) -> String {
    let mut m = String::new();
    m.push_str(
        "缺少「屏幕录制」权限：macOS 会拦截其他应用的窗口内容，截出来只剩桌面壁纸。\n\
         （不是没截到，是内容被系统主动抹掉了。截图前必须拿到这张授权。）\n\
         \n\
         ① 打开「系统设置 → 隐私与安全性 → 屏幕录制」，在本应用（ScreenTime Pro）前打勾；\n\
         ② 然后**完全退出**本应用再重新打开 —— 新授权只对新启动的进程生效，只开开关不重启没用；\n\
         ③ 若那里本来就是开着的：先点左下角「−」把本应用移除，再重新打开本应用授权。\n\
         \x20  （在本应用「设置 → 截图」点「重置权限并重启」等价于终端执行：\n",
    );
    m.push_str(&format!("       tccutil reset ScreenCapture {BUNDLE_ID}\n）\n"));

    m.push_str("\n—— 本次实际检测到的情况 ——\n");
    m.push_str(&format!("运行路径：{}\n", sig.exe_path));

    if sig.translocated {
        m.push_str(
            "⚠️ 检测到 App Translocation（从临时只读路径启动）：\n\
             从 DMG／下载目录里直接双击 .app 时，macOS 会先把它挪到一个**随机只读路径**再运行。\n\
             你刚才勾选授权的那份 App 与正在运行的这一份**不是同一个身份**，\n\
             而且每次启动路径还会变 —— 所以重启多少次都不会生效。\n\
             ✅ 处理：把 App 拖进「应用程序」文件夹，从那里启动，再重新授权。\n",
        );
    }
    if sig.quarantined {
        m.push_str("⚠️ 应用带下载隔离标记（com.apple.quarantine），会持续触发上面的临时路径运行。\n");
        if let Some(b) = &sig.bundle_path {
            m.push_str(&format!(
                "✅ 处理：终端执行 xattr -dr com.apple.quarantine \"{b}\" 后再重新授权。\n"
            ));
        }
    }
    if sig.adhoc_unsigned == Some(true) {
        m.push_str(
            "⚠️ 本应用未做开发者签名（ad-hoc／未签名）—— 这是「授权反复失效」的**根因**。\n\
             macOS 把授权记在「代码签名指纹」上，未签名应用的指纹每次重新构建都会变：\n\
             每个新版本在系统眼里都是一个全新的 App，而系统设置里那个「开」是**旧版本**的记录。\n\
             所以本版必须重新做上面 ③（移除 → 重新授权）。\n\
             根治办法：用 Apple Developer ID 对 macOS 包做签名 + 公证。\n",
        );
    }
    if sig.signature_summary.is_empty() {
        // 无签名信息可展示，跳过
    } else {
        m.push_str(&format!("签名信息：{}\n", sig.signature_summary));
    }
    match sig.titles {
        TitlesProbe::Readable => {}
        TitlesProbe::Redacted => m.push_str(
            "交叉验证：屏幕上其他应用的**窗口标题一个都读不到**（被系统隐私机制抹掉）\n\
             → 与预检一致，确认授权确实未生效。\n",
        ),
        TitlesProbe::Inconclusive => m.push_str(
            "交叉验证：屏幕上没有其他应用的普通窗口，本次无法二次确认（以官方预检为准）。\n",
        ),
    }
    m
}

/// 弹窗展示的**一句话**原因（v0.9.4）。
///
/// v0.9.3 的失败文案把三步操作 + Translocation / 隔离 / ad-hoc 证据全部糊在弹窗里，
/// 用户反馈「太多太丑」。v0.9.4 起弹窗只说一句话 + 一个结构化原因码（前端据此渲染
/// 紧凑授权弹窗与对应按钮），完整证据只进日志（[`build_error`]）。
///
/// 原因码优先级：Translocation > 隔离标记 > ad-hoc 指纹漂移 > 普通未授权。
/// （前两者是「重启多少次都没用」的硬问题，必须最优先提示。）
fn short_error(sig: &PermissionSignals) -> String {
    let reason = if sig.translocated {
        "app_translocated"
    } else if sig.quarantined {
        "quarantined"
    } else if sig.adhoc_unsigned == Some(true) {
        "adhoc_signature"
    } else {
        "not_granted"
    };
    format!("__PERM__:{reason}")
}

// ===== v0.9.4：权限状态快照 + 主动请求（供前端紧凑授权弹窗轮询） =====

/// 一次授权状态查询的结果（`screenshot_permission_status` IPC 的返回体）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct PermissionStatusSnapshot {
    /// 官方预检 `CGPreflightScreenCaptureAccess()`。⚠️ 可能是过期 false，不能单独采信
    pub preflight: bool,
    /// 独立第二信号（窗口标题探针）：`"readable"` / `"redacted"` / `"inconclusive"`
    pub titles: &'static str,
    /// 双信号取或后的最终判定：当前进程**现在**能不能抓屏
    pub permitted: bool,
    /// 结构化原因码（未放行时前端据此选择提示与按钮）：
    /// `app_translocated` / `quarantined` / `adhoc_signature` / `not_granted`；
    /// 已放行时为 `ok`
    pub reason: &'static str,
}

/// 采集当前授权状态快照（轻量：预检 + 窗口枚举；未放行才补子进程诊断）。
pub fn status_snapshot() -> PermissionStatusSnapshot {
    let sig = collect();
    let titles = match sig.titles {
        TitlesProbe::Readable => "readable",
        TitlesProbe::Redacted => "redacted",
        TitlesProbe::Inconclusive => "inconclusive",
    };
    let reason = if sig.permitted() {
        "ok"
    } else if sig.translocated {
        "app_translocated"
    } else if sig.quarantined {
        "quarantined"
    } else if sig.adhoc_unsigned == Some(true) {
        "adhoc_signature"
    } else {
        "not_granted"
    };
    PermissionStatusSnapshot {
        preflight: sig.preflight,
        titles,
        permitted: sig.permitted(),
        reason,
    }
}

/// 主动请求一次「屏幕录制」授权（弹系统框 / 登记进系统设置列表）。
///
/// ⚠️ **会阻塞**（首次弹框同步等用户点击）→ 只能从 `spawn_blocking` 里调。
/// 返回请求后的最新状态快照（前端轮询用同一结构）。
pub fn request_permission() -> PermissionStatusSnapshot {
    crate::tracker::macos::request_screen_capture_access();
    status_snapshot()
}

/// 打开「系统设置 → 隐私与安全性 → 屏幕录制」面板。
pub fn open_permission_settings() {
    let _ = std::process::Command::new("open")
        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture")
        .spawn();
    crate::logging::audit("open_screen_capture_settings", "Privacy_ScreenCapture");
}

/// 启动时预请求授权（Task 79）：把本应用登记进系统设置列表 + 首次弹系统授权框。
///
/// 为什么在启动做：v0.9.3 及以前，登记发生在**第一次截图**时 —— 用户第一次按快捷键，
/// 屏幕上突然弹出一个系统授权框，而遮罩窗流程已经开始走，体验割裂且容易误点「不允许」。
/// 启动后 3 秒在后台线程预请求，用户在主界面就能从容看到并处理这个框。
///
/// 只在「预检为 false」时请求（已授权的应用每次启动都弹框就是骚扰了）。
/// 结果只记日志，不弹任何 UI —— 前端紧凑弹窗（Task 81）会通过
/// `screenshot_permission_status` 轮询自行感知状态变化。
pub fn preflight_request_on_startup() {
    if crate::tracker::macos::is_screen_capture_trusted() {
        tracing::debug!("屏幕录制授权已生效，启动预请求跳过");
        return;
    }
    tracing::info!("启动预请求：向系统登记并请求「屏幕录制」授权");
    let granted = crate::tracker::macos::request_screen_capture_access();
    let sig = collect();
    tracing::info!(
        granted,
        signals = %sig.summary(),
        "启动预请求完成（新授权仅对新启动的进程生效）"
    );
    // 把状态变化广播给主窗：前端据此决定是否亮出紧凑授权引导弹窗（Task 81）
    let _ = STARTUP_HANDLE.get().map(|h| {
        let _ = h.emit("screenshot-permission-changed", status_snapshot());
    });
}

/// 启动期缓存的 AppHandle（`preflight_request_on_startup` 里发事件用）。
///
/// 用 `OnceLock` 存一份全局句柄：setup 阶段写入，之后任何线程都能安全读取。
static STARTUP_HANDLE: std::sync::OnceLock<AppHandle> = std::sync::OnceLock::new();

/// setup 阶段调用：缓存 AppHandle + 3 秒后在后台线程执行预请求。
///
/// 为什么延迟 3 秒：启动瞬间主窗还在初始化，立刻弹系统授权框容易和
/// 应用自身的首屏渲染抢焦点；延后几秒体验更从容。且**必须**在独立线程：
/// `CGRequestScreenCaptureAccess` 首次调用会同步等用户点击，绝不能占住主线程。
pub fn schedule_startup_permission_request(handle: AppHandle) {
    let _ = STARTUP_HANDLE.set(handle.clone());
    std::thread::Builder::new()
        .name("perm-preflight".into())
        .spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(3));
            preflight_request_on_startup();
        })
        .ok();
}

/// 重置本应用的「屏幕录制」授权记录（`tccutil reset ScreenCapture <bundle id>`）。
///
/// 为什么需要：授权记录一旦处于「指向旧版本指纹」的脏状态，**系统设置界面是清不掉的**
/// —— 用户把开关关了再开、把条目删了再加，记录的往往仍是旧身份。
/// `tccutil reset` 是 Apple 官方唯一支持的清除方式，清掉后下次请求会重新弹框、
/// 重新写入一条干净的记录。
///
/// ⚠️ 只重置**本应用**的条目（传了 bundle id），不会影响其它 App。
/// 正常情况不需要 sudo（用户级 TCC 条目）；失败时把原因原样返回给界面。
pub fn reset_permission() -> Result<String, String> {
    let out = std::process::Command::new("/usr/bin/tccutil")
        .args(["reset", "ScreenCapture", BUNDLE_ID])
        .output()
        .map_err(|e| format!("无法执行 tccutil: {e}"))?;
    let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
    if out.status.success() {
        crate::logging::audit("reset_screen_capture_permission", BUNDLE_ID);
        Ok(if stdout.is_empty() {
            "已清除本应用的「屏幕录制」授权记录，重新授权即可。".to_string()
        } else {
            stdout
        })
    } else {
        let detail = if stderr.is_empty() { stdout } else { stderr };
        Err(format!("重置失败：{detail}"))
    }
}
