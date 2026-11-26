//! Windows 低级键盘钩子：只产生 USB HID keyboard usage ID，不读取字符或上下文。

use std::collections::HashSet;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, SyncSender};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::thread::{self, JoinHandle};
use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, PeekMessageW, PostThreadMessageW,
    SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx, HC_ACTION, HHOOK, KBDLLHOOKSTRUCT,
    LLKHF_EXTENDED, LLKHF_INJECTED, MSG, PM_NOREMOVE, WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP,
    WM_QUIT, WM_SYSKEYDOWN, WM_SYSKEYUP,
};

type KeyCallback = Arc<dyn Fn(i32) + Send + Sync>;

const FLAG_EXTENDED: u32 = LLKHF_EXTENDED.0;
const FLAG_INJECTED: u32 = LLKHF_INJECTED.0;
const VK_PAUSE: u32 = 0x13;

static NEXT_GENERATION: AtomicU64 = AtomicU64::new(1);
static HOOK_STATE: OnceLock<Mutex<HookState>> = OnceLock::new();

#[derive(Default)]
struct HookState {
    generation: u64,
    callback: Option<KeyCallback>,
    pressed: PressedState,
}

pub(crate) struct InputHookHandle {
    generation: u64,
    thread_id: Option<u32>,
    thread: Option<JoinHandle<()>>,
}

impl InputHookHandle {
    pub(crate) fn stop(&mut self) {
        if let Some(thread_id) = self.thread_id.take() {
            // SAFETY: thread_id 由钩子线程在消息队列创建后返回；其余参数为 WM_QUIT 的空参数。
            if let Err(error) =
                unsafe { PostThreadMessageW(thread_id, WM_QUIT, WPARAM(0), LPARAM(0)) }
            {
                // 线程若已自行退出，PostThreadMessageW 失败是正常竞态，join 负责回收。
                tracing::debug!(%error, "向 Windows 键盘钩子线程发送退出消息失败");
            }
        }
        if let Some(thread) = self.thread.take() {
            join_hook_thread(thread);
        }
        clear_hook_state(self.generation);
    }
}

impl Drop for InputHookHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

pub(crate) fn spawn(callback: KeyCallback) -> Result<InputHookHandle, String> {
    let generation = NEXT_GENERATION.fetch_add(1, Ordering::Relaxed);
    {
        let mut state = lock_hook_state();
        if state.callback.is_some() {
            return Err("Windows 键盘采集钩子已在运行".to_string());
        }
        state.generation = generation;
        state.callback = Some(callback);
        state.pressed = PressedState::default();
    }

    let (startup_sender, startup_receiver) = mpsc::sync_channel::<Result<u32, String>>(0);
    let thread = match thread::Builder::new()
        .name("screentime-keyboard-hook".to_string())
        .spawn(move || hook_thread_main(generation, startup_sender))
    {
        Ok(thread) => thread,
        Err(error) => {
            clear_hook_state(generation);
            return Err(format!("启动 Windows 键盘钩子线程失败: {error}"));
        }
    };

    match startup_receiver.recv() {
        Ok(Ok(thread_id)) => Ok(InputHookHandle {
            generation,
            thread_id: Some(thread_id),
            thread: Some(thread),
        }),
        Ok(Err(error)) => {
            join_hook_thread(thread);
            Err(error)
        }
        Err(error) => {
            clear_hook_state(generation);
            join_hook_thread(thread);
            Err(format!("Windows 键盘钩子线程初始化异常: {error}"))
        }
    }
}

fn hook_thread_main(generation: u64, startup_sender: SyncSender<Result<u32, String>>) {
    // Drop 覆盖正常退出及展开：先卸载钩子，再释放全局回调。
    let mut cleanup = ThreadCleanup {
        generation,
        hook: None,
    };

    // SAFETY: 该 API 不接收指针，仅返回当前 Win32 线程 ID。
    let thread_id = unsafe { GetCurrentThreadId() };
    let mut message = MSG::default();
    // SAFETY: message 是可写 MSG；PeekMessageW 同时确保线程消息队列已创建，消除 stop 竞态。
    unsafe {
        let _ = PeekMessageW(&mut message, None, 0, 0, PM_NOREMOVE);
    }

    // SAFETY: keyboard_hook_proc 具有 HOOKPROC 所需 ABI；低级键盘钩子不要求 DLL 模块句柄。
    let hook = match unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook_proc), None, 0) }
    {
        Ok(hook) => hook,
        Err(error) => {
            drop(startup_sender.send(Err(format!("安装 Windows 键盘钩子失败: {error}"))));
            return;
        }
    };
    cleanup.hook = Some(hook);

    if startup_sender.send(Ok(thread_id)).is_err() {
        return;
    }

    loop {
        // SAFETY: message 指向有效 MSG；NULL HWND 表示读取当前线程消息队列。
        let result = unsafe { GetMessageW(&mut message, None, 0, 0) }.0;
        if result == -1 || result == 0 {
            break;
        }
        // SAFETY: GetMessageW 已填充 message，且该消息属于当前线程。
        unsafe {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}

struct ThreadCleanup {
    generation: u64,
    hook: Option<HHOOK>,
}

impl Drop for ThreadCleanup {
    fn drop(&mut self) {
        if let Some(hook) = self.hook.take() {
            // SAFETY: hook 来自 SetWindowsHookExW，并在安装它的同一线程退出前卸载一次。
            if let Err(error) = unsafe { UnhookWindowsHookEx(hook) } {
                tracing::warn!(%error, "卸载 Windows 键盘钩子失败");
            }
        }
        clear_hook_state(self.generation);
    }
}

fn join_hook_thread(thread: JoinHandle<()>) {
    if thread.join().is_err() {
        tracing::warn!("Windows 键盘钩子线程异常退出");
    }
}

fn lock_hook_state() -> MutexGuard<'static, HookState> {
    let mutex = HOOK_STATE.get_or_init(|| Mutex::new(HookState::default()));
    match mutex.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

fn clear_hook_state(generation: u64) {
    let mut state = lock_hook_state();
    // 旧句柄的延迟 stop 不得清除新一代钩子的回调。
    if state.generation == generation {
        state.callback = None;
        state.pressed = PressedState::default();
        state.generation = 0;
    }
}

unsafe extern "system" fn keyboard_hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 && lparam.0 != 0 {
        let kind = match wparam.0 as u32 {
            WM_KEYDOWN | WM_SYSKEYDOWN => Some(KeyEventKind::Down),
            WM_KEYUP | WM_SYSKEYUP => Some(KeyEventKind::Up),
            _ => None,
        };
        if let Some(kind) = kind {
            // SAFETY: HC_ACTION 下 WH_KEYBOARD_LL 的 lparam 指向本次调用有效的 KBDLLHOOKSTRUCT。
            let data = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
            dispatch_event(KeyEvent {
                kind,
                scan_code: data.scanCode,
                vk_code: data.vkCode,
                flags: data.flags.0,
            });
        }
    }

    // SAFETY: 原样转发当前钩子的参数，不拦截输入，也不要求有效 HHOOK。
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

fn dispatch_event(event: KeyEvent) {
    let (callback, usage) = {
        let mut state = lock_hook_state();
        if state.callback.is_none() {
            return;
        }
        let Some(usage) = state.pressed.process(event) else {
            return;
        };
        let Some(callback) = state.callback.as_ref().map(Arc::clone) else {
            return;
        };
        (callback, usage)
    };

    // 不在全局锁内执行外部代码；panic 也不能穿过 Win32 FFI 边界。
    drop(catch_unwind(AssertUnwindSafe(move || callback(usage))));
}

#[derive(Clone, Copy)]
enum KeyEventKind {
    Down,
    Up,
}

#[derive(Clone, Copy)]
struct KeyEvent {
    kind: KeyEventKind,
    scan_code: u32,
    vk_code: u32,
    flags: u32,
}

#[derive(Default)]
struct PressedState {
    pressed: HashSet<i32>,
}

impl PressedState {
    fn process(&mut self, event: KeyEvent) -> Option<i32> {
        if event.flags & FLAG_INJECTED != 0 {
            return None;
        }
        let usage = hid_usage(
            event.scan_code,
            event.flags & FLAG_EXTENDED != 0,
            event.vk_code,
        )?;
        match event.kind {
            KeyEventKind::Down => {
                if self.pressed.insert(usage) {
                    Some(usage)
                } else {
                    None
                }
            }
            KeyEventKind::Up => {
                self.pressed.remove(&usage);
                None
            }
        }
    }
}

/// 将 Set 1 扫描码及 E0 扩展标记映射到 USB HID usage page 0x07。
/// 不把 Windows 扫描码直接当作 HID；不支持的扫描码明确跳过。
fn hid_usage(scan_code: u32, extended: bool, vk_code: u32) -> Option<i32> {
    // Pause 与 NumLock 共用 0x45，只能借助虚拟键码可靠区分。
    if scan_code == 0x45 && vk_code == VK_PAUSE {
        return Some(0x48);
    }

    let usage = if extended {
        match scan_code {
            0x1c => 0x58, // Keypad Enter
            0x1d => 0xe4, // Right Control
            0x35 => 0x54, // Keypad /
            0x37 => 0x46, // Print Screen
            0x38 => 0xe6, // Right Alt
            0x47 => 0x4a, // Home
            0x48 => 0x52, // Up
            0x49 => 0x4b, // Page Up
            0x4b => 0x50, // Left
            0x4d => 0x4f, // Right
            0x4f => 0x4d, // End
            0x50 => 0x51, // Down
            0x51 => 0x4e, // Page Down
            0x52 => 0x49, // Insert
            0x53 => 0x4c, // Delete
            0x5b => 0xe3, // Left GUI
            0x5c => 0xe7, // Right GUI
            0x5d => 0x65, // Application
            _ => return None,
        }
    } else {
        match scan_code {
            0x01 => 0x29, // Escape
            0x02 => 0x1e, // 1
            0x03 => 0x1f, // 2
            0x04 => 0x20, // 3
            0x05 => 0x21, // 4
            0x06 => 0x22, // 5
            0x07 => 0x23, // 6
            0x08 => 0x24, // 7
            0x09 => 0x25, // 8
            0x0a => 0x26, // 9
            0x0b => 0x27, // 0
            0x0c => 0x2d, // -
            0x0d => 0x2e, // =
            0x0e => 0x2a, // Backspace
            0x0f => 0x2b, // Tab
            0x10 => 0x14, // Q
            0x11 => 0x1a, // W
            0x12 => 0x08, // E
            0x13 => 0x15, // R
            0x14 => 0x17, // T
            0x15 => 0x1c, // Y
            0x16 => 0x18, // U
            0x17 => 0x0c, // I
            0x18 => 0x12, // O
            0x19 => 0x13, // P
            0x1a => 0x2f, // [
            0x1b => 0x30, // ]
            0x1c => 0x28, // Enter
            0x1d => 0xe0, // Left Control
            0x1e => 0x04, // A
            0x1f => 0x16, // S
            0x20 => 0x07, // D
            0x21 => 0x09, // F
            0x22 => 0x0a, // G
            0x23 => 0x0b, // H
            0x24 => 0x0d, // J
            0x25 => 0x0e, // K
            0x26 => 0x0f, // L
            0x27 => 0x33, // ;
            0x28 => 0x34, // '
            0x29 => 0x35, // `
            0x2a => 0xe1, // Left Shift
            0x2b => 0x31, // Backslash
            0x2c => 0x1d, // Z
            0x2d => 0x1b, // X
            0x2e => 0x06, // C
            0x2f => 0x19, // V
            0x30 => 0x05, // B
            0x31 => 0x11, // N
            0x32 => 0x10, // M
            0x33 => 0x36, // ,
            0x34 => 0x37, // .
            0x35 => 0x38, // /
            0x36 => 0xe5, // Right Shift
            0x37 => 0x55, // Keypad *
            0x38 => 0xe2, // Left Alt
            0x39 => 0x2c, // Space
            0x3a => 0x39, // Caps Lock
            0x3b => 0x3a, // F1
            0x3c => 0x3b, // F2
            0x3d => 0x3c, // F3
            0x3e => 0x3d, // F4
            0x3f => 0x3e, // F5
            0x40 => 0x3f, // F6
            0x41 => 0x40, // F7
            0x42 => 0x41, // F8
            0x43 => 0x42, // F9
            0x44 => 0x43, // F10
            0x45 => 0x53, // Num Lock
            0x46 => 0x47, // Scroll Lock
            0x47 => 0x5f, // Keypad 7
            0x48 => 0x60, // Keypad 8
            0x49 => 0x61, // Keypad 9
            0x4a => 0x56, // Keypad -
            0x4b => 0x5c, // Keypad 4
            0x4c => 0x5d, // Keypad 5
            0x4d => 0x5e, // Keypad 6
            0x4e => 0x57, // Keypad +
            0x4f => 0x59, // Keypad 1
            0x50 => 0x5a, // Keypad 2
            0x51 => 0x5b, // Keypad 3
            0x52 => 0x62, // Keypad 0
            0x53 => 0x63, // Keypad .
            0x56 => 0x64, // Non-US backslash
            0x57 => 0x44, // F11
            0x58 => 0x45, // F12
            _ => return None,
        }
    };
    Some(usage)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(kind: KeyEventKind, scan_code: u32, vk_code: u32, flags: u32) -> KeyEvent {
        KeyEvent {
            kind,
            scan_code,
            vk_code,
            flags,
        }
    }

    fn down(scan_code: u32) -> KeyEvent {
        event(KeyEventKind::Down, scan_code, 0, 0)
    }

    #[test]
    fn letters_map_to_hid_usage_ids() {
        assert_eq!(hid_usage(0x1e, false, 0x41), Some(0x04), "A");
        assert_eq!(hid_usage(0x10, false, 0x51), Some(0x14), "Q");
        assert_eq!(hid_usage(0x2c, false, 0x5a), Some(0x1d), "Z");
        assert_eq!(hid_usage(0x02, false, 0x31), Some(0x1e), "1");
    }

    #[test]
    fn left_and_right_shift_are_distinct() {
        let mut state = PressedState::default();
        assert_eq!(state.process(down(0x2a)), Some(0xe1), "left Shift");
        assert_eq!(state.process(down(0x36)), Some(0xe5), "right Shift");
        assert_eq!(hid_usage(0x1d, true, 0xa3), Some(0xe4), "right Ctrl");
        assert_eq!(hid_usage(0x38, true, 0xa5), Some(0xe6), "right Alt");
    }

    #[test]
    fn held_key_auto_repeat_is_counted_once() {
        let mut state = PressedState::default();
        assert_eq!(state.process(down(0x1e)), Some(0x04));
        assert_eq!(state.process(down(0x1e)), None);
        assert_eq!(state.process(down(0x1e)), None);
    }

    #[test]
    fn key_up_allows_a_new_press_to_count() {
        let mut state = PressedState::default();
        assert_eq!(state.process(down(0x1e)), Some(0x04));
        assert_eq!(state.process(event(KeyEventKind::Up, 0x1e, 0, 0)), None);
        assert_eq!(state.process(down(0x1e)), Some(0x04));
    }

    #[test]
    fn injected_events_are_ignored() {
        let mut state = PressedState::default();
        assert_eq!(
            state.process(event(KeyEventKind::Down, 0x1e, 0, FLAG_INJECTED)),
            None
        );
        assert_eq!(state.process(down(0x1e)), Some(0x04));
        assert_eq!(
            state.process(event(KeyEventKind::Up, 0x1e, 0, FLAG_INJECTED)),
            None
        );
        assert_eq!(
            state.process(down(0x1e)),
            None,
            "injected key-up must not clear state"
        );
    }

    #[test]
    fn extended_keys_and_keypad_are_not_conflated() {
        assert_eq!(hid_usage(0x1c, false, 0x0d), Some(0x28));
        assert_eq!(hid_usage(0x1c, true, 0x0d), Some(0x58));
        assert_eq!(hid_usage(0x35, false, 0xbf), Some(0x38));
        assert_eq!(hid_usage(0x35, true, 0x6f), Some(0x54));
        assert_eq!(hid_usage(0x52, true, 0x2d), Some(0x49));
        assert_eq!(hid_usage(0x52, false, 0x60), Some(0x62));
    }

    #[test]
    fn pause_and_num_lock_are_disambiguated_by_vk_code() {
        assert_eq!(hid_usage(0x45, false, VK_PAUSE), Some(0x48));
        assert_eq!(hid_usage(0x45, false, 0x90), Some(0x53));
    }

    #[test]
    fn unsupported_keys_are_skipped() {
        assert_eq!(hid_usage(0, false, 0), None);
        assert_eq!(hid_usage(0x5e, true, 0), None);
        let mut state = PressedState::default();
        assert_eq!(state.process(down(0x7f)), None);
    }

    #[test]
    #[ignore]
    fn hook_can_start_and_stop_repeatedly() {
        for _ in 0..2 {
            let mut handle = spawn(Arc::new(|_| {})).expect("安装真实 Windows 键盘钩子");
            handle.stop();
        }
    }
}
