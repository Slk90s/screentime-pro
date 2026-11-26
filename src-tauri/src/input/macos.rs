//! macOS 键盘事件监听：只产生 USB HID keyboard usage ID，不读取字符或上下文。

use core_foundation_sys::base::{CFRelease, CFTypeRef};
use core_foundation_sys::mach_port::{CFMachPortCreateRunLoopSource, CFMachPortRef};
use core_foundation_sys::runloop::{
    kCFRunLoopDefaultMode, kCFRunLoopEntry, CFRunLoopActivity, CFRunLoopAddObserver,
    CFRunLoopAddSource, CFRunLoopGetCurrent, CFRunLoopObserverContext, CFRunLoopObserverCreate,
    CFRunLoopObserverRef, CFRunLoopRef, CFRunLoopRemoveObserver, CFRunLoopRemoveSource,
    CFRunLoopRun, CFRunLoopSourceRef, CFRunLoopStop, CFRunLoopWakeUp,
};
use std::os::raw::c_void;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr;
use std::sync::mpsc::{self, SyncSender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};

const PERMISSION_REQUIRED: &str = "permission_required: 需要输入监听权限";

const CG_EVENT_KEY_DOWN: u32 = 10;
const CG_EVENT_TAP_DISABLED_BY_TIMEOUT: u32 = 0xffff_fffe;
const CG_EVENT_TAP_DISABLED_BY_USER_INPUT: u32 = 0xffff_ffff;
const CG_KEYBOARD_EVENT_AUTOREPEAT: u32 = 8;
const CG_KEYBOARD_EVENT_KEYCODE: u32 = 9;
const CG_HID_EVENT_TAP: u32 = 0;
const CG_HEAD_INSERT_EVENT_TAP: u32 = 0;
const CG_EVENT_TAP_OPTION_LISTEN_ONLY: u32 = 1;
const CG_EVENT_MASK_KEY_DOWN: u64 = 1_u64 << CG_EVENT_KEY_DOWN;

type KeyCallback = Arc<dyn Fn(i32) + Send + Sync>;
type CGEventRef = *mut c_void;
type CGEventTapProxy = *mut c_void;
type CGEventTapCallback = Option<
    unsafe extern "C" fn(
        proxy: CGEventTapProxy,
        event_type: u32,
        event: CGEventRef,
        user_info: *mut c_void,
    ) -> CGEventRef,
>;

#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGPreflightListenEventAccess() -> bool;
    fn CGRequestListenEventAccess() -> bool;
    fn CGEventTapCreate(
        tap: u32,
        place: u32,
        options: u32,
        events_of_interest: u64,
        callback: CGEventTapCallback,
        user_info: *mut c_void,
    ) -> CFMachPortRef;
    fn CGEventTapEnable(tap: CFMachPortRef, enable: bool);
    fn CGEventGetIntegerValueField(event: CGEventRef, field: u32) -> i64;
}

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    // 项目支持的 macOS 版本均高于该 API 的 10.7 引入版本。
    fn IsSecureEventInputEnabled() -> bool;
}

/// macOS 监听句柄；RunLoop 仅以地址形式跨线程传递，并在 join 前保持有效。
pub(crate) struct InputHookHandle {
    shared: Arc<SharedState>,
    thread: Option<JoinHandle<()>>,
}

impl InputHookHandle {
    /// 停止监听并回收线程；重复调用无副作用。
    pub(crate) fn stop(&mut self) {
        stop_run_loop(&self.shared);
        if let Some(thread) = self.thread.take() {
            join_hook_thread(thread);
        }
    }
}

impl Drop for InputHookHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

/// 启动 listen-only 键盘事件 tap。
///
/// 未获得「输入监听」权限时会请求系统授权，并始终返回权限错误，让调用方明确处理。
pub(crate) fn spawn(callback: KeyCallback) -> Result<InputHookHandle, String> {
    // SAFETY: 两个 API 均无指针参数；项目最低 macOS 版本覆盖其 10.15 可用性。
    if !unsafe { CGPreflightListenEventAccess() } {
        // SAFETY: 同上。即使系统立即授权，本次也返回明确错误，由用户重新开启采集。
        let granted = unsafe { CGRequestListenEventAccess() };
        tracing::debug!(granted, "已请求 macOS 输入监听权限，本次不启动键盘采集");
        return Err(PERMISSION_REQUIRED.to_string());
    }

    let shared = Arc::new(SharedState::default());
    let thread_shared = Arc::clone(&shared);
    let (startup_sender, startup_receiver) = mpsc::sync_channel::<Result<(), String>>(0);
    let thread = thread::Builder::new()
        .name("screentime-macos-keyboard".to_string())
        .spawn(move || hook_thread_main(callback, thread_shared, startup_sender))
        .map_err(|error| format!("启动 macOS 键盘事件线程失败: {error}"))?;

    match startup_receiver.recv() {
        Ok(Ok(())) => Ok(InputHookHandle {
            shared,
            thread: Some(thread),
        }),
        Ok(Err(error)) => {
            join_hook_thread(thread);
            Err(error)
        }
        Err(error) => {
            join_hook_thread(thread);
            Err(format!("macOS 键盘事件线程初始化异常: {error}"))
        }
    }
}

#[derive(Default)]
struct SharedState {
    // usize 避免裸指针影响句柄的 Send/Sync；锁同时保护 RunLoop 的借用有效期。
    run_loop: Mutex<Option<usize>>,
}

fn lock_run_loop(shared: &SharedState) -> MutexGuard<'_, Option<usize>> {
    match shared.run_loop.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

fn stop_run_loop(shared: &SharedState) {
    let mut slot = lock_run_loop(shared);
    if let Some(address) = slot.take() {
        let run_loop = address as CFRunLoopRef;
        // SAFETY: 地址来自存活线程的 CFRunLoopGetCurrent。线程清理也先取同一把锁，
        // 因此两个调用期间对象不会销毁；Stop 与 WakeUp 配对覆盖正在等待的情形。
        unsafe {
            CFRunLoopStop(run_loop);
            CFRunLoopWakeUp(run_loop);
        }
    }
}

fn join_hook_thread(thread: JoinHandle<()>) {
    if thread.join().is_err() {
        tracing::warn!("macOS 键盘事件线程异常退出");
    }
}

struct EventContext {
    callback: KeyCallback,
    run_loop: CFRunLoopRef, // CFRunLoopGetCurrent 的借用，不拥有、不释放。
}

struct StartupSignal {
    sender: SyncSender<Result<(), String>>,
}

/// 持有一个 Create/Copy 规则返回的 +1 CF 对象；Get 借用不得放入此包装。
struct OwnedCf(CFTypeRef);

impl OwnedCf {
    fn from_created(value: CFTypeRef) -> Option<Self> {
        if value.is_null() {
            None
        } else {
            Some(Self(value))
        }
    }
}

impl Drop for OwnedCf {
    fn drop(&mut self) {
        // SAFETY: OwnedCf 只包装非空 Create 返回值，拥有且仅释放一次 +1 引用。
        unsafe { CFRelease(self.0) };
    }
}

struct RunLoopResources {
    run_loop: CFRunLoopRef, // 借用，线程退出前始终有效。
    tap: OwnedCf,
    source: Option<OwnedCf>,
    observer: Option<OwnedCf>,
}

impl Drop for RunLoopResources {
    fn drop(&mut self) {
        // SAFETY: tap 是存活的本线程 Create 对象；先禁用再摘除调度源。
        unsafe { CGEventTapEnable(self.tap.0 as CFMachPortRef, false) };
        if let Some(observer) = self.observer.take() {
            // SAFETY: observer 与借用的 RunLoop 均存活；重复移除是无害空操作。
            unsafe {
                CFRunLoopRemoveObserver(
                    self.run_loop,
                    observer.0 as CFRunLoopObserverRef,
                    kCFRunLoopDefaultMode,
                )
            };
            drop(observer);
        }
        if let Some(source) = self.source.take() {
            // SAFETY: source 与借用的 RunLoop 均存活；先移除可确保释放后不再回调。
            unsafe {
                CFRunLoopRemoveSource(
                    self.run_loop,
                    source.0 as CFRunLoopSourceRef,
                    kCFRunLoopDefaultMode,
                )
            };
            drop(source);
        }
        // tap 最后由 OwnedCf 自动释放；RunLoop 是 Get 借用，绝不 CFRelease。
    }
}

fn hook_thread_main(
    callback: KeyCallback,
    shared: Arc<SharedState>,
    startup_sender: SyncSender<Result<(), String>>,
) {
    let startup_signal = Box::new(StartupSignal {
        sender: startup_sender,
    });
    // SAFETY: 当前线程必定有或可由系统创建 RunLoop；返回值是线程生命周期内的借用。
    let run_loop = unsafe { CFRunLoopGetCurrent() };
    if run_loop.is_null() {
        send_startup_error(
            &startup_signal,
            "获取 macOS 键盘事件 RunLoop 失败".to_string(),
        );
        return;
    }

    // Box 提供稳定地址；声明早于 resources，保证其比 tap/source/observer 后释放。
    let callback_context = Box::new(EventContext { callback, run_loop });
    let callback_context_ptr = &*callback_context as *const EventContext as *mut c_void;
    // SAFETY: HID 位置、头部插入和 ListenOnly 均为合法常量；掩码只含 KeyDown。
    // callback_context_ptr 指向的 Box 会一直存活到 RunLoop 和 tap 清理之后。
    let tap = unsafe {
        CGEventTapCreate(
            CG_HID_EVENT_TAP,
            CG_HEAD_INSERT_EVENT_TAP,
            CG_EVENT_TAP_OPTION_LISTEN_ONLY,
            CG_EVENT_MASK_KEY_DOWN,
            Some(event_tap_callback),
            callback_context_ptr,
        )
    };
    let Some(tap) = OwnedCf::from_created(tap as CFTypeRef) else {
        // 权限可能在 preflight 后被撤销，不能把 tap 创建失败误报为正常启动。
        let error = if unsafe { CGPreflightListenEventAccess() } {
            "创建 macOS 键盘事件 tap 失败".to_string()
        } else {
            PERMISSION_REQUIRED.to_string()
        };
        send_startup_error(&startup_signal, error);
        return;
    };
    let mut resources = RunLoopResources {
        run_loop,
        tap,
        source: None,
        observer: None,
    };

    // SAFETY: tap 为有效 Create 对象；空 allocator 使用默认值，order=0 合法。
    let source =
        unsafe { CFMachPortCreateRunLoopSource(ptr::null(), resources.tap.0 as CFMachPortRef, 0) };
    let Some(source) = OwnedCf::from_created(source as CFTypeRef) else {
        send_startup_error(
            &startup_signal,
            "创建 macOS 键盘事件 RunLoopSource 失败".to_string(),
        );
        return;
    };

    // 启动成功通知从 RunLoop Entry 发出，确保 spawn 返回后 stop 不会早于 RunLoop。
    let startup_signal_ptr = &*startup_signal as *const StartupSignal as *mut c_void;
    let mut observer_context = CFRunLoopObserverContext {
        version: 0,
        info: startup_signal_ptr,
        retain: None,
        release: None,
        copyDescription: None,
    };
    // SAFETY: context 在调用期间有效；info 指向的 Box 比 observer 活得更久。
    // 不设置 retain/release，因为这里只借用 startup_signal，不转移其所有权。
    let observer = unsafe {
        CFRunLoopObserverCreate(
            ptr::null(),
            kCFRunLoopEntry,
            0,
            0,
            run_loop_started,
            &mut observer_context,
        )
    };
    let Some(observer) = OwnedCf::from_created(observer as CFTypeRef) else {
        send_startup_error(
            &startup_signal,
            "创建 macOS 键盘 RunLoopObserver 失败".to_string(),
        );
        return;
    };

    let source_ptr = source.0 as CFRunLoopSourceRef;
    let observer_ptr = observer.0 as CFRunLoopObserverRef;
    resources.source = Some(source);
    resources.observer = Some(observer);
    // SAFETY: 三个对象与 default mode 均有效，且只在创建它们的线程中调度。
    unsafe {
        CFRunLoopAddSource(run_loop, source_ptr, kCFRunLoopDefaultMode);
        CFRunLoopAddObserver(run_loop, observer_ptr, kCFRunLoopDefaultMode);
        CGEventTapEnable(resources.tap.0 as CFMachPortRef, true);
    }

    {
        let mut slot = lock_run_loop(&shared);
        *slot = Some(run_loop as usize);
    }
    // SAFETY: 当前线程已装配 source；只由 stop 或 tap 失效回调退出。
    unsafe { CFRunLoopRun() };
    {
        // 与 stop_run_loop 使用同一把锁，防止 handle 在线程清理后访问悬空的 Get 借用。
        let mut slot = lock_run_loop(&shared);
        *slot = None;
    }
}

fn send_startup_error(signal: &StartupSignal, error: String) {
    if signal.sender.send(Err(error)).is_err() {
        tracing::debug!("macOS 键盘事件启动错误接收端已关闭");
    }
}

extern "C" fn run_loop_started(
    _observer: CFRunLoopObserverRef,
    _activity: CFRunLoopActivity,
    info: *mut c_void,
) {
    if info.is_null() {
        tracing::warn!("macOS 键盘 RunLoop 启动观察器缺少上下文");
        // SAFETY: 当前正在运行该线程的 RunLoop；Get 返回借用，Stop 不需要额外引用。
        unsafe {
            let run_loop = CFRunLoopGetCurrent();
            if !run_loop.is_null() {
                CFRunLoopStop(run_loop);
            }
        }
        return;
    }

    // SAFETY: info 由 hook_thread_main 设置为有效 StartupSignal Box，且生命周期覆盖本回调。
    let signal = unsafe { &*(info as *const StartupSignal) };
    if signal.sender.send(Ok(())).is_err() {
        // 接收端已放弃启动时不能留下无人管理的监听线程。
        // SAFETY: 当前 RunLoop 的 Get 借用在本回调及所属线程中有效。
        unsafe {
            let run_loop = CFRunLoopGetCurrent();
            if !run_loop.is_null() {
                CFRunLoopStop(run_loop);
            }
        }
    }
}

unsafe extern "C" fn event_tap_callback(
    _proxy: CGEventTapProxy,
    event_type: u32,
    event: CGEventRef,
    user_info: *mut c_void,
) -> CGEventRef {
    if event_type == CG_EVENT_TAP_DISABLED_BY_TIMEOUT
        || event_type == CG_EVENT_TAP_DISABLED_BY_USER_INPUT
    {
        tracing::warn!(
            event_type,
            "macOS 键盘事件 tap 已失效，停止采集（可能由权限撤销或超时导致）"
        );
        // SAFETY: user_info 正常指向 EventContext；异常为空时退回当前线程的 Get 借用。
        let run_loop = if user_info.is_null() {
            CFRunLoopGetCurrent()
        } else {
            (*(user_info as *const EventContext)).run_loop
        };
        if !run_loop.is_null() {
            CFRunLoopStop(run_loop);
        }
        return event;
    }

    if event_type != CG_EVENT_KEY_DOWN || event.is_null() || user_info.is_null() {
        return event;
    }

    // 安全输入期间不读取键码、也不计数，避免采集密码等敏感输入。
    if IsSecureEventInputEnabled() {
        return event;
    }
    if CGEventGetIntegerValueField(event, CG_KEYBOARD_EVENT_AUTOREPEAT) != 0 {
        return event;
    }

    let keycode = CGEventGetIntegerValueField(event, CG_KEYBOARD_EVENT_KEYCODE);
    let Some(usage) = mac_keycode_to_hid(keycode) else {
        return event;
    };
    // SAFETY: user_info 指向 hook_thread_main 中存活到 tap 清理之后的 EventContext。
    let callback = Arc::clone(&(*(user_info as *const EventContext)).callback);
    // 外部回调 panic 不得穿过 CoreGraphics FFI 边界。
    if catch_unwind(AssertUnwindSafe(move || callback(usage))).is_err() {
        tracing::warn!("macOS 键盘事件回调发生 panic，已隔离");
    }

    // event 是 CoreGraphics 借用的对象，listen-only 模式原样返回，不 retain/release。
    event
}

/// 将 macOS 硬件虚拟键码映射到 USB HID keyboard usage page 0x07。
///
/// 映射基于物理键位，不经过输入法或字符转换；Fn、音量等非本页常用键明确跳过。
fn mac_keycode_to_hid(keycode: i64) -> Option<i32> {
    let usage = match keycode {
        // ANSI 字母、数字与标点。
        0x00 => 0x04, // A
        0x01 => 0x16, // S
        0x02 => 0x07, // D
        0x03 => 0x09, // F
        0x04 => 0x0b, // H
        0x05 => 0x0a, // G
        0x06 => 0x1d, // Z
        0x07 => 0x1b, // X
        0x08 => 0x06, // C
        0x09 => 0x19, // V
        0x0a => 0x64, // ISO Section / Non-US Backslash
        0x0b => 0x05, // B
        0x0c => 0x14, // Q
        0x0d => 0x1a, // W
        0x0e => 0x08, // E
        0x0f => 0x15, // R
        0x10 => 0x1c, // Y
        0x11 => 0x17, // T
        0x12 => 0x1e, // 1
        0x13 => 0x1f, // 2
        0x14 => 0x20, // 3
        0x15 => 0x21, // 4
        0x16 => 0x23, // 6
        0x17 => 0x22, // 5
        0x18 => 0x2e, // =
        0x19 => 0x26, // 9
        0x1a => 0x24, // 7
        0x1b => 0x2d, // -
        0x1c => 0x25, // 8
        0x1d => 0x27, // 0
        0x1e => 0x30, // ]
        0x1f => 0x12, // O
        0x20 => 0x18, // U
        0x21 => 0x2f, // [
        0x22 => 0x0c, // I
        0x23 => 0x13, // P
        0x24 => 0x28, // Return
        0x25 => 0x0f, // L
        0x26 => 0x0d, // J
        0x27 => 0x34, // '
        0x28 => 0x0e, // K
        0x29 => 0x33, // ;
        0x2a => 0x31, // Backslash
        0x2b => 0x36, // ,
        0x2c => 0x38, // /
        0x2d => 0x11, // N
        0x2e => 0x10, // M
        0x2f => 0x37, // .
        0x30 => 0x2b, // Tab
        0x31 => 0x2c, // Space
        0x32 => 0x35, // Grave
        0x33 => 0x2a, // Backspace
        0x35 => 0x29, // Escape

        // 左右修饰键保持独立；Fn 没有对应的通用 keyboard page usage。
        0x36 => 0xe7, // Right Command / GUI
        0x37 => 0xe3, // Left Command / GUI
        0x38 => 0xe1, // Left Shift
        0x39 => 0x39, // Caps Lock
        0x3a => 0xe2, // Left Option / Alt
        0x3b => 0xe0, // Left Control
        0x3c => 0xe5, // Right Shift
        0x3d => 0xe6, // Right Option / Alt
        0x3e => 0xe4, // Right Control

        // 小键盘与扩展 F 键。
        0x40 => 0x6c, // F17
        0x41 => 0x63, // Keypad Decimal
        0x43 => 0x55, // Keypad *
        0x45 => 0x57, // Keypad +
        0x47 => 0x53, // Keypad Clear / Num Lock
        0x4b => 0x54, // Keypad /
        0x4c => 0x58, // Keypad Enter
        0x4e => 0x56, // Keypad -
        0x4f => 0x6d, // F18
        0x50 => 0x6e, // F19
        0x51 => 0x67, // Keypad =
        0x52 => 0x62, // Keypad 0
        0x53 => 0x59, // Keypad 1
        0x54 => 0x5a, // Keypad 2
        0x55 => 0x5b, // Keypad 3
        0x56 => 0x5c, // Keypad 4
        0x57 => 0x5d, // Keypad 5
        0x58 => 0x5e, // Keypad 6
        0x59 => 0x5f, // Keypad 7
        0x5a => 0x6f, // F20
        0x5b => 0x60, // Keypad 8
        0x5c => 0x61, // Keypad 9
        0x5d => 0x89, // JIS Yen / International 3
        0x5e => 0x87, // JIS Ro / International 1
        0x5f => 0x85, // JIS Keypad Comma

        // F1-F16（macOS 键码并非连续排列）。
        0x60 => 0x3e, // F5
        0x61 => 0x3f, // F6
        0x62 => 0x40, // F7
        0x63 => 0x3c, // F3
        0x64 => 0x41, // F8
        0x65 => 0x42, // F9
        0x66 => 0x91, // JIS Eisu / Language 2
        0x67 => 0x44, // F11
        0x68 => 0x90, // JIS Kana / Language 1
        0x69 => 0x68, // F13
        0x6a => 0x6b, // F16
        0x6b => 0x69, // F14
        0x6d => 0x43, // F10
        0x6f => 0x45, // F12
        0x71 => 0x6a, // F15

        // 编辑键、导航键与方向键。
        0x72 => 0x49, // Help / Insert
        0x73 => 0x4a, // Home
        0x74 => 0x4b, // Page Up
        0x75 => 0x4c, // Forward Delete
        0x76 => 0x3d, // F4
        0x77 => 0x4d, // End
        0x78 => 0x3b, // F2
        0x79 => 0x4e, // Page Down
        0x7a => 0x3a, // F1
        0x7b => 0x50, // Left Arrow
        0x7c => 0x4f, // Right Arrow
        0x7d => 0x51, // Down Arrow
        0x7e => 0x52, // Up Arrow
        _ => return None,
    };
    Some(usage)
}

#[cfg(test)]
mod tests {
    use super::mac_keycode_to_hid;

    #[test]
    fn letters_digits_and_punctuation_use_physical_positions() {
        assert_eq!(mac_keycode_to_hid(0x00), Some(0x04), "A");
        assert_eq!(mac_keycode_to_hid(0x0c), Some(0x14), "Q");
        assert_eq!(mac_keycode_to_hid(0x06), Some(0x1d), "Z");
        assert_eq!(mac_keycode_to_hid(0x12), Some(0x1e), "1");
        assert_eq!(mac_keycode_to_hid(0x1d), Some(0x27), "0");
        assert_eq!(mac_keycode_to_hid(0x29), Some(0x33), ";");
        assert_eq!(mac_keycode_to_hid(0x2c), Some(0x38), "/");
        assert_eq!(mac_keycode_to_hid(0x0a), Some(0x64), "ISO section");
    }

    #[test]
    fn function_keys_cover_noncontiguous_mac_keycodes() {
        assert_eq!(mac_keycode_to_hid(0x7a), Some(0x3a), "F1");
        assert_eq!(mac_keycode_to_hid(0x78), Some(0x3b), "F2");
        assert_eq!(mac_keycode_to_hid(0x63), Some(0x3c), "F3");
        assert_eq!(mac_keycode_to_hid(0x76), Some(0x3d), "F4");
        assert_eq!(mac_keycode_to_hid(0x60), Some(0x3e), "F5");
        assert_eq!(mac_keycode_to_hid(0x6f), Some(0x45), "F12");
        assert_eq!(mac_keycode_to_hid(0x69), Some(0x68), "F13");
        assert_eq!(mac_keycode_to_hid(0x5a), Some(0x6f), "F20");
    }

    #[test]
    fn left_and_right_modifiers_are_distinct() {
        assert_eq!(mac_keycode_to_hid(0x3b), Some(0xe0), "left Control");
        assert_eq!(mac_keycode_to_hid(0x3e), Some(0xe4), "right Control");
        assert_eq!(mac_keycode_to_hid(0x38), Some(0xe1), "left Shift");
        assert_eq!(mac_keycode_to_hid(0x3c), Some(0xe5), "right Shift");
        assert_eq!(mac_keycode_to_hid(0x3a), Some(0xe2), "left Option");
        assert_eq!(mac_keycode_to_hid(0x3d), Some(0xe6), "right Option");
        assert_eq!(mac_keycode_to_hid(0x37), Some(0xe3), "left Command");
        assert_eq!(mac_keycode_to_hid(0x36), Some(0xe7), "right Command");
    }

    #[test]
    fn editing_navigation_and_arrow_keys_are_mapped() {
        assert_eq!(mac_keycode_to_hid(0x33), Some(0x2a), "Backspace");
        assert_eq!(mac_keycode_to_hid(0x75), Some(0x4c), "Forward Delete");
        assert_eq!(mac_keycode_to_hid(0x72), Some(0x49), "Help / Insert");
        assert_eq!(mac_keycode_to_hid(0x73), Some(0x4a), "Home");
        assert_eq!(mac_keycode_to_hid(0x77), Some(0x4d), "End");
        assert_eq!(mac_keycode_to_hid(0x74), Some(0x4b), "Page Up");
        assert_eq!(mac_keycode_to_hid(0x79), Some(0x4e), "Page Down");
        assert_eq!(mac_keycode_to_hid(0x7b), Some(0x50), "Left");
        assert_eq!(mac_keycode_to_hid(0x7c), Some(0x4f), "Right");
        assert_eq!(mac_keycode_to_hid(0x7d), Some(0x51), "Down");
        assert_eq!(mac_keycode_to_hid(0x7e), Some(0x52), "Up");
    }

    #[test]
    fn keypad_digits_operators_and_enter_are_not_conflated() {
        assert_eq!(mac_keycode_to_hid(0x52), Some(0x62), "Keypad 0");
        assert_eq!(mac_keycode_to_hid(0x53), Some(0x59), "Keypad 1");
        assert_eq!(mac_keycode_to_hid(0x5c), Some(0x61), "Keypad 9");
        assert_eq!(mac_keycode_to_hid(0x4b), Some(0x54), "Keypad /");
        assert_eq!(mac_keycode_to_hid(0x43), Some(0x55), "Keypad *");
        assert_eq!(mac_keycode_to_hid(0x4e), Some(0x56), "Keypad -");
        assert_eq!(mac_keycode_to_hid(0x45), Some(0x57), "Keypad +");
        assert_eq!(mac_keycode_to_hid(0x4c), Some(0x58), "Keypad Enter");
        assert_eq!(mac_keycode_to_hid(0x24), Some(0x28), "Return");
        assert_eq!(mac_keycode_to_hid(0x51), Some(0x67), "Keypad =");
        assert_eq!(mac_keycode_to_hid(0x41), Some(0x63), "Keypad Decimal");
        assert_eq!(mac_keycode_to_hid(0x47), Some(0x53), "Keypad Clear");
    }

    #[test]
    fn unsupported_and_out_of_range_keycodes_are_skipped() {
        assert_eq!(mac_keycode_to_hid(0x3f), None, "Fn");
        assert_eq!(mac_keycode_to_hid(0x48), None, "Volume Up");
        assert_eq!(mac_keycode_to_hid(0x34), None, "unknown");
        assert_eq!(mac_keycode_to_hid(-1), None);
        assert_eq!(mac_keycode_to_hid(0x100), None);
    }
}
