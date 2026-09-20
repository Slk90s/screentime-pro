//! macOS 平台采集器实现
//!
//! 负责两件事：
//! 1. 获取当前「前台应用」（用户正在使用的那个 App）
//! 2. 获取用户「空闲秒数」（多久没操作键鼠）
//!
//! 关键 API：
//! - `NSWorkspace.frontmostApplication()` —— 当前前台应用（无需特殊权限即可拿到 App 名称/包名）
//! - `CGEventSourceSecondsSinceLastEventType()` —— 空闲检测，依赖「辅助功能」权限
//! - 窗口标题需要「屏幕录制」权限（后续扩展用）
//! - **截图**依赖同一张「屏幕录制」TCC 授权。v0.9.1 起截图侧把它做成了**强制闸门**，
//!   v0.9.3 起闸门整体迁到 `crate::screenshot::macos`（含授权失效根因诊断），
//!   本模块只保留两个权限**原语**供其复用：无授权时抓屏不会报错，只会拿到
//!   「只有桌面壁纸」的图，必须提前拦下来并引导用户授权。
//!
//! 注意：本采集器仅在 `target_os = "macos"` 下编译进二进制。

use crate::error::TrackerError;
use crate::tracker::platform::{PlatformTracker, RawApp};
use objc2_app_kit::NSWorkspace;
use std::os::raw::{c_char, c_void};
use std::path::Path;

// CoreFoundation-Sys（底层 FFI，跨版本稳定）：用于读取前台窗口标题（需「屏幕录制」权限）
// 注意：core-foundation 0.10 移除了 CFArray 的 get_len/get_index 高层 API，
// 故这里直接用 core_foundation_sys 的 C 函数，避免版本漂移导致编译失败。
use core_foundation_sys::array::{CFArrayGetCount, CFArrayGetValueAtIndex, CFArrayRef};
use core_foundation_sys::base::{CFIndex, CFTypeRef};
use core_foundation_sys::dictionary::{CFDictionaryGetValue, CFDictionaryRef};
use core_foundation_sys::number::{CFNumberGetValue, CFNumberRef, kCFNumberSInt64Type};
use core_foundation_sys::string::{
    CFStringGetCString, CFStringGetLength, CFStringRef, kCFStringEncodingUTF8,
};
use core_graphics::window::{
    CGWindowListCopyWindowInfo, CGWindowListOption, kCGNullWindowID, kCGWindowListExcludeDesktopElements,
    kCGWindowListOptionOnScreenOnly, kCGWindowName, kCGWindowOwnerPID,
};

/// macOS 采集器：基于 NSWorkspace 前台应用 + CoreGraphics 空闲检测
pub struct MacOSTracker;

impl PlatformTracker for MacOSTracker {
    /// 获取当前前台应用的信息
    ///
    /// 当 ScreenTime Pro 自身窗口处于最前台时，这里会返回本程序自身；
    /// 窗口隐藏到托盘后，macOS 会让真正在用的 App 成为前台应用，从而被正确记录。
    fn get_foreground_app(&self) -> Result<RawApp, TrackerError> {
        // 拿到系统工作区单例，读取「最前台的应用」
        let ws = NSWorkspace::sharedWorkspace();
        let app = ws
            .frontmostApplication()
            .ok_or(TrackerError::NoForeground)?;

        // 应用展示名（如「微信」「Safari」），拿不到则兜底为 Unknown
        let name = app
            .localizedName()
            .map(|s| s.to_string())
            .unwrap_or_else(|| "Unknown".to_string());
        // 包名（Bundle ID），如 com.tencent.xinwechat，用于稳定去重
        let bundle_id = app.bundleIdentifier().map(|s| s.to_string());
        // 可执行文件路径，如 /Applications/WeChat.app/Contents/MacOS/WeChat
        let exe_path = app
            .executableURL()
            .and_then(|url| url.path())
            .map(|p| p.to_string());

        // 进程名取可执行文件名（不含路径），作为同应用去重主键
        let process_name = exe_path
            .as_ref()
            .and_then(|p| Path::new(p).file_name())
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| name.clone());

        // 前台窗口标题（需「屏幕录制」权限；未授权时返回 None，分类回退到进程名）
        let pid = app.processIdentifier();
        let window_title = get_foreground_window_title(pid);

        Ok(RawApp {
            name,
            process_name,
            exe_path,
            bundle_id,
            window_title,
        })
    }

    /// 获取用户空闲秒数（多久没有键鼠输入）
    fn get_idle_seconds(&self) -> Result<u64, TrackerError> {
        Ok(cg_idle_seconds())
    }
}

/// 获取指定 PID 的前台窗口标题（用于「按窗口标题分类」）
///
/// 依赖「屏幕录制」权限（`CGPreflightScreenCaptureAccess`）。未授权或查询失败均返回 None，
/// 不影响主流程（分类回退到进程名）。内部用 CoreGraphics 的 `CGWindowListCopyWindowInfo`
/// 枚举前台窗口，按 OwnerPID 匹配后取 `kCGWindowName`。
pub fn get_foreground_window_title(pid: i32) -> Option<String> {
    if !is_screen_capture_trusted() {
        return None;
    }
    unsafe {
        let option: CGWindowListOption =
            kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements;
        // 注意：该 C 函数返回裸指针 CFArrayRef（非 Option），需手动判空
        let list: CFArrayRef = CGWindowListCopyWindowInfo(option, kCGNullWindowID);
        if list.is_null() {
            return None;
        }
        let count = CFArrayGetCount(list);
        for i in 0..count {
            let dict = CFArrayGetValueAtIndex(list, i) as CFDictionaryRef;
            if dict.is_null() {
                continue;
            }
            // 取窗口所属进程 PID，与当前前台应用比对
            let pid_value: CFTypeRef =
                CFDictionaryGetValue(dict, kCGWindowOwnerPID as *const c_void);
            if pid_value.is_null() {
                continue;
            }
            let mut owner_pid: i64 = 0;
            // CFNumberGetValue 返回 Boolean（bool），成功为 true；0/失败则跳过
            if !CFNumberGetValue(
                pid_value as CFNumberRef,
                kCFNumberSInt64Type,
                &mut owner_pid as *mut i64 as *mut c_void,
            ) {
                continue;
            }
            if owner_pid as i32 != pid {
                continue;
            }
            // 命中后取窗口标题
            let name_value: CFTypeRef = CFDictionaryGetValue(dict, kCGWindowName as *const c_void);
            if name_value.is_null() {
                continue;
            }
            let cf_str = name_value as CFStringRef;
            let len = CFStringGetLength(cf_str);
            if len == 0 {
                continue;
            }
            // UTF-8 最坏每个 UniChar 占 4 字节，+1 给结尾 NUL
            let mut buf: Vec<u8> = vec![0u8; (len as usize) * 4 + 1];
            // CFStringGetCString 返回 Boolean（u8），非 0 表示成功
            let ok = CFStringGetCString(
                cf_str,
                buf.as_mut_ptr() as *mut c_char,
                buf.len() as CFIndex,
                kCFStringEncodingUTF8,
            );
            if ok != 0 {
                let cstr = std::ffi::CStr::from_ptr(buf.as_ptr() as *const c_char);
                let s = cstr.to_string_lossy().to_string();
                if !s.is_empty() {
                    return Some(s);
                }
            }
        }
        None
    }
}

/// 辅助功能（Accessibility）权限是否已授予
///
/// 空闲检测依赖此权限。未授予时 `CGEventSource...` 会返回异常值，
/// 需要在系统设置中手动开启：隐私与安全性 → 辅助功能。
///
/// v0.4.1 修复：用 `AXIsProcessTrustedWithOptions` + `kAXTrustedCheckOptionPrompt=false`
/// 比旧的 `AXIsProcessTrusted()` 在以下场景更可靠：
/// - ad-hoc 签名应用：identifier 会随 binary 变，`AXIsProcessTrusted()` 有缓存；
///   `WithOptions` 不缓存，每次都查 TCC
/// - 应用从其他位置移动到 /Applications 后：旧 API 仍返回旧路径的授权状态，
///   新 API 重新校验当前位置
///
/// ⚠️ 关键：传 `kAXTrustedCheckOptionPrompt: kCFBooleanFalse`，否则会触发系统弹窗
/// （用户首次启动不希望立刻被打扰）。让用户通过 banner 主动点「前往系统设置」后再开。
pub fn is_accessibility_trusted() -> bool {
    use core_foundation::base::TCFType;
    use core_foundation::boolean::CFBoolean;
    use core_foundation::dictionary::CFDictionary;
    use core_foundation::string::CFString;

    // 应用服务（ApplicationServices）下的辅助功能 API
    #[link(name = "ApplicationServices", kind = "framework")]
    extern "C" {
        fn AXIsProcessTrustedWithOptions(options: *const c_void) -> bool;
    }

    // 用 core-foundation 高级 API 拼一个 CFDictionary：
    //   { "AXTrustedCheckOptionPrompt" : kCFBooleanFalse }
    // Foundation 的 NSDictionary 与 CoreFoundation 的 CFDictionary 互通
    // （Toll-Free Bridging），所以传 CFDictionaryRef 等价于 NSDictionary*。
    unsafe {
        let key = CFString::new("AXTrustedCheckOptionPrompt");
        let val = CFBoolean::false_value();
        let dict = CFDictionary::from_CFType_pairs(&[(key.as_CFType(), val.as_CFType())]);
        // CFDictionary<CFType, CFType> 内部就是 CFDictionaryRef，用 as_CFTypeRef 取指针
        let dict_ptr: *const c_void = dict.as_CFTypeRef();
        let trusted = AXIsProcessTrustedWithOptions(dict_ptr);
        // 离开作用域后 CFDictionary / CFString / CFBoolean 引用计数自动清零
        let _ = dict;
        // v0.4.2 日志：DEBUG 级记录权限状态（生产环境默认关，排查时 RUST_LOG=debug）
        tracing::debug!(trusted, "macOS 辅助功能权限检查");
        trusted
    }
}

/// 屏幕录制（Screen Recording）权限是否已授予
///
/// 后续如果要采集「窗口标题」级别的粒度，需要此权限。
/// 10.15+ 提供预检 API，无需弹窗即可查询。
pub fn is_screen_capture_trusted() -> bool {
    let trusted = unsafe { CGPreflightScreenCaptureAccess() };
    tracing::debug!(trusted, "macOS 屏幕录制权限检查");
    trusted
}

/// 主动请求「屏幕录制」权限（macOS 10.15+）
///
/// 与 [`is_screen_capture_trusted`] 的区别：后者只**查询**，本函数会
/// ① 把应用登记进「系统设置 → 隐私与安全性 → 屏幕录制」列表（未表态过的应用
///    不会出现在该列表里，用户想手动勾选也无从下手）；② 用户尚未表态时弹系统授权框。
///
/// 为什么必须有这个函数：截图用的 `CGWindowListCreateImage` 在**无授权时不报错** ——
/// 它返回一张「只有桌面壁纸」的图（macOS 隐私软化行为，见 xcap#123）。
/// 不主动请求，用户就只会看到「截图里应用全没了」，而系统设置列表里还找不到本应用。
///
/// ⚠️ 两条行为必须记住（调用方负责）：
/// - **会阻塞**：首次弹窗时同步等待用户点击，可能数秒到数十秒。必须放在阻塞线程里
///   （`spawn_blocking`），不要挂在 async 任务的直接路径上。
/// - **拒过就不再问**：用户点过「不允许」后本函数立即返回 `false` 且**不再弹窗**，
///   此时只能引导用户手动去系统设置勾选，且**必须重启应用**新授权才生效（TCC 按进程快照）。
pub fn request_screen_capture_access() -> bool {
    let granted = unsafe { CGRequestScreenCaptureAccess() };
    tracing::info!(granted, "macOS 屏幕录制权限请求");
    granted
}

// v0.9.3（2026-09-20）：原 `exe_diagnosis()` 与 `ensure_screen_capture_ready()` 已迁入
// `crate::screenshot::macos` —— 那才是「截图授权」的归属地，且在那边强化为
// 「官方预检 + 窗口标题探针」双信号（修掉预检返回**过期 false** 时误拦有效授权的问题），
// 并把运行路径 / App Translocation / 下载隔离标记 / 代码签名状态一并纳入错误文案。
// 本采集器只保留下面两个**权限原语**，供截图闸门复用。

// ⚠️ 为什么「升级后授权会失效」：本应用目前**没有代码签名**（见 `tauri.macos.conf.json`
// 未配置 `signingIdentity`），macOS 的 TCC 是按**二进制指纹**记录授权的 —— 每次换版本
// 指纹就变，旧的「已授权」记录虽仍显示为开，实际已不匹配。要根治只能给 macOS 包做
// 稳定签名（Developer ID + 公证），那需要 Apple Developer 账号。

// 链接 CoreGraphics.framework，调用 C 接口获取用户空闲秒数
#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    // 返回距上次任意输入事件经过的秒数
    fn CGEventSourceSecondsSinceLastEventType(state: u32, event_type: u32) -> f64;
    // 屏幕录制权限预检（macOS 10.15+）
    fn CGPreflightScreenCaptureAccess() -> bool;
    // 屏幕录制权限请求（macOS 10.15+）：首次调用弹系统授权框并把本应用登记进系统设置列表
    fn CGRequestScreenCaptureAccess() -> bool;
}

// kCGEventSourceStateCombinedSessionState = 0
// kCGAnyInputEventType = 0xFFFFFFFF
fn cg_idle_seconds() -> u64 {
    unsafe { CGEventSourceSecondsSinceLastEventType(0, 0xFFFF_FFFF) as u64 }
}
