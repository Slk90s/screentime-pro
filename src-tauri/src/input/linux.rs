//! Linux/X11 全局只读键盘采集。
//!
//! 使用 RECORD 扩展，只读取核心 KeyPress/KeyRelease 的 keycode，并转换为
//! USB HID keyboard usage ID；不读取字符、窗口或应用信息。Wayland 明确降级为不支持。

use std::collections::HashSet;
use std::env;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::mpsc::{self, SyncSender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use x11rb::connection::{Connection, RequestConnection};
use x11rb::protocol::record::{self, ConnectionExt as _};
use x11rb::protocol::xproto;
use x11rb::rust_connection::RustConnection;

const WAYLAND_ERROR: &str = "unsupported: Wayland 不支持全局只读键盘统计";
const RECORD_FROM_SERVER: u8 = 0;
const RECORD_START_OF_DATA: u8 = 4;
const CORE_EVENT_BYTES: usize = 32;

type KeyCallback = Arc<dyn Fn(i32) + Send + Sync>;

/// 正在运行的 XRecord 采集会话；字段仅由本模块管理。
pub(crate) struct InputHookHandle {
    control: Option<RustConnection>,
    context: Option<record::Context>,
    thread: Option<JoinHandle<()>>,
}

impl InputHookHandle {
    /// 停止 RECORD，唤醒并回收读取线程，然后释放 context 和控制连接。
    pub(crate) fn stop(&mut self) {
        let Some(context) = self.context.take() else {
            if let Some(thread) = self.thread.take() {
                join_hook_thread(thread);
            }
            self.control.take();
            return;
        };

        if let Some(control) = self.control.as_ref() {
            // EnableContext 在数据连接上阻塞等待回复；另一连接上的 DisableContext
            // 会使服务器发送 EndOfData，从而可靠唤醒读取线程。
            match control.record_disable_context(context) {
                Ok(cookie) => {
                    if let Err(error) = cookie.check() {
                        tracing::debug!(%error, "禁用 XRecord context 时连接已关闭");
                    }
                }
                Err(error) => tracing::debug!(%error, "发送 XRecord DisableContext 失败"),
            }
        }

        if let Some(thread) = self.thread.take() {
            join_hook_thread(thread);
        }

        if let Some(control) = self.control.take() {
            free_record_context(&control, context);
            // control 在此作用域末尾关闭；数据连接已由读取线程关闭。
        }
    }
}

impl Drop for InputHookHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

/// 启动 X11 RECORD 键盘采集；重复 KeyPress 和无法映射的键不会上报。
pub(crate) fn spawn(callback: KeyCallback) -> Result<InputHookHandle, String> {
    let display = env::var("DISPLAY").ok();
    let session_type = env::var("XDG_SESSION_TYPE").ok();
    check_x11_session(display.as_deref(), session_type.as_deref())?;

    // RECORD 协议要求控制与数据使用不同连接：读取线程会阻塞在 EnableContext。
    let (control, _) =
        x11rb::connect(None).map_err(|error| format!("连接 X11 控制通道失败: {error}"))?;
    let (data, _) =
        x11rb::connect(None).map_err(|error| format!("连接 X11 数据通道失败: {error}"))?;

    if control
        .extension_information(record::X11_EXTENSION_NAME)
        .map_err(|error| format!("查询 X11 RECORD 扩展失败: {error}"))?
        .is_none()
    {
        return Err("unsupported: X11 服务器不支持 RECORD 扩展".to_string());
    }
    control
        .record_query_version(
            record::X11_XML_VERSION.0 as u16,
            record::X11_XML_VERSION.1 as u16,
        )
        .map_err(|error| format!("查询 XRecord 版本失败: {error}"))?
        .reply()
        .map_err(|error| format!("读取 XRecord 版本失败: {error}"))?;

    let context = control
        .generate_id()
        .map_err(|error| format!("分配 XRecord context ID 失败: {error}"))?;
    let range = record::Range {
        device_events: record::Range8 {
            first: xproto::KEY_PRESS_EVENT,
            last: xproto::KEY_RELEASE_EVENT,
        },
        ..record::Range::default()
    };
    control
        .record_create_context(context, 0, &[record::CS::ALL_CLIENTS.into()], &[range])
        .map_err(|error| format!("创建 XRecord context 失败: {error}"))?
        .check()
        .map_err(|error| format!("服务器拒绝 XRecord context: {error}"))?;

    let (startup_sender, startup_receiver) = mpsc::sync_channel::<Result<(), String>>(1);
    let thread = match thread::Builder::new()
        .name("screentime-xrecord-keyboard".to_string())
        .spawn(move || record_thread_main(data, context, callback, startup_sender))
    {
        Ok(thread) => thread,
        Err(error) => {
            free_record_context(&control, context);
            return Err(format!("启动 XRecord 键盘采集线程失败: {error}"));
        }
    };

    match startup_receiver.recv() {
        Ok(Ok(())) => Ok(InputHookHandle {
            control: Some(control),
            context: Some(context),
            thread: Some(thread),
        }),
        Ok(Err(error)) => {
            join_hook_thread(thread);
            free_record_context(&control, context);
            Err(error)
        }
        Err(error) => {
            join_hook_thread(thread);
            free_record_context(&control, context);
            Err(format!("XRecord 键盘采集线程初始化异常: {error}"))
        }
    }
}

fn check_x11_session(display: Option<&str>, session_type: Option<&str>) -> Result<(), String> {
    if session_type
        .map(|value| value.trim().eq_ignore_ascii_case("wayland"))
        .unwrap_or(false)
    {
        return Err(WAYLAND_ERROR.to_string());
    }
    if display.map(str::trim).unwrap_or("").is_empty() {
        return Err("unsupported: 未检测到 X11 DISPLAY".to_string());
    }
    Ok(())
}

fn free_record_context(control: &RustConnection, context: record::Context) {
    match control.record_free_context(context) {
        Ok(cookie) => {
            if let Err(error) = cookie.check() {
                tracing::debug!(%error, "释放 XRecord context 时连接已关闭");
            }
        }
        Err(error) => tracing::debug!(%error, "发送 XRecord FreeContext 失败"),
    }
}

fn join_hook_thread(thread: JoinHandle<()>) {
    if thread.join().is_err() {
        tracing::warn!("XRecord 键盘采集线程异常退出");
    }
}

fn record_thread_main(
    data: RustConnection,
    context: record::Context,
    callback: KeyCallback,
    startup_sender: SyncSender<Result<(), String>>,
) {
    let replies = match data.record_enable_context(context) {
        Ok(replies) => replies,
        Err(error) => {
            drop(startup_sender.send(Err(format!("启用 XRecord context 失败: {error}"))));
            return;
        }
    };

    let mut pressed = PressedState::default();
    let mut started = false;
    for reply in replies {
        let reply = match reply {
            Ok(reply) => reply,
            Err(error) => {
                if started {
                    tracing::warn!(%error, "XRecord 键盘采集连接中断");
                } else {
                    drop(startup_sender.send(Err(format!("读取 XRecord 启动回复失败: {error}"))));
                }
                return;
            }
        };

        match reply.category {
            RECORD_START_OF_DATA if !started => {
                // 收到 StartOfData 才表示 EnableContext 已被服务器处理，消除 stop 竞态。
                if startup_sender.send(Ok(())).is_err() {
                    return;
                }
                started = true;
            }
            RECORD_FROM_SERVER if started && !reply.client_swapped => {
                dispatch_record_data(&reply.data, &mut pressed, &callback);
            }
            _ => {}
        }
    }

    if !started {
        drop(startup_sender.send(Err("XRecord 未返回 StartOfData".to_string())));
    }
}

fn dispatch_record_data(data: &[u8], pressed: &mut PressedState, callback: &KeyCallback) {
    let packets = data.chunks_exact(CORE_EVENT_BYTES);
    let trailing = packets.remainder();
    if !trailing.is_empty() {
        tracing::debug!(bytes = trailing.len(), "忽略不完整的 XRecord 核心事件数据");
    }

    for packet in packets {
        let kind = match packet[0] {
            xproto::KEY_PRESS_EVENT => KeyEventKind::Down,
            xproto::KEY_RELEASE_EVENT => KeyEventKind::Up,
            _ => continue,
        };
        let event = KeyEvent {
            kind,
            keycode: packet[1],
        };
        if let Some(usage) = pressed.process(event) {
            // 外部回调 panic 不应终止采集线程或造成资源泄漏。
            drop(catch_unwind(AssertUnwindSafe(|| callback(usage))));
        }
    }
}

#[derive(Clone, Copy)]
enum KeyEventKind {
    Down,
    Up,
}

#[derive(Clone, Copy)]
struct KeyEvent {
    kind: KeyEventKind,
    keycode: u8,
}

#[derive(Default)]
struct PressedState {
    pressed: HashSet<u8>,
}

impl PressedState {
    fn process(&mut self, event: KeyEvent) -> Option<i32> {
        match event.kind {
            KeyEventKind::Down => {
                if !self.pressed.insert(event.keycode) {
                    return None;
                }
                hid_usage(event.keycode)
            }
            KeyEventKind::Up => {
                self.pressed.remove(&event.keycode);
                None
            }
        }
    }
}

/// 将 X11 keycode（evdev input code + 8）映射到 USB HID usage page 0x07。
/// 只描述物理键位，不经过 keysym，因此不受当前键盘布局影响。
fn hid_usage(keycode: u8) -> Option<i32> {
    let usage = match keycode {
        9 => 0x29,   // Escape
        10 => 0x1e,  // 1
        11 => 0x1f,  // 2
        12 => 0x20,  // 3
        13 => 0x21,  // 4
        14 => 0x22,  // 5
        15 => 0x23,  // 6
        16 => 0x24,  // 7
        17 => 0x25,  // 8
        18 => 0x26,  // 9
        19 => 0x27,  // 0
        20 => 0x2d,  // -
        21 => 0x2e,  // =
        22 => 0x2a,  // Backspace
        23 => 0x2b,  // Tab
        24 => 0x14,  // Q
        25 => 0x1a,  // W
        26 => 0x08,  // E
        27 => 0x15,  // R
        28 => 0x17,  // T
        29 => 0x1c,  // Y
        30 => 0x18,  // U
        31 => 0x0c,  // I
        32 => 0x12,  // O
        33 => 0x13,  // P
        34 => 0x2f,  // [
        35 => 0x30,  // ]
        36 => 0x28,  // Enter
        37 => 0xe0,  // Left Control
        38 => 0x04,  // A
        39 => 0x16,  // S
        40 => 0x07,  // D
        41 => 0x09,  // F
        42 => 0x0a,  // G
        43 => 0x0b,  // H
        44 => 0x0d,  // J
        45 => 0x0e,  // K
        46 => 0x0f,  // L
        47 => 0x33,  // ;
        48 => 0x34,  // '
        49 => 0x35,  // `
        50 => 0xe1,  // Left Shift
        51 => 0x31,  // Backslash
        52 => 0x1d,  // Z
        53 => 0x1b,  // X
        54 => 0x06,  // C
        55 => 0x19,  // V
        56 => 0x05,  // B
        57 => 0x11,  // N
        58 => 0x10,  // M
        59 => 0x36,  // ,
        60 => 0x37,  // .
        61 => 0x38,  // /
        62 => 0xe5,  // Right Shift
        63 => 0x55,  // Keypad *
        64 => 0xe2,  // Left Alt
        65 => 0x2c,  // Space
        66 => 0x39,  // Caps Lock
        67 => 0x3a,  // F1
        68 => 0x3b,  // F2
        69 => 0x3c,  // F3
        70 => 0x3d,  // F4
        71 => 0x3e,  // F5
        72 => 0x3f,  // F6
        73 => 0x40,  // F7
        74 => 0x41,  // F8
        75 => 0x42,  // F9
        76 => 0x43,  // F10
        77 => 0x53,  // Num Lock
        78 => 0x47,  // Scroll Lock
        79 => 0x5f,  // Keypad 7
        80 => 0x60,  // Keypad 8
        81 => 0x61,  // Keypad 9
        82 => 0x56,  // Keypad -
        83 => 0x5c,  // Keypad 4
        84 => 0x5d,  // Keypad 5
        85 => 0x5e,  // Keypad 6
        86 => 0x57,  // Keypad +
        87 => 0x59,  // Keypad 1
        88 => 0x5a,  // Keypad 2
        89 => 0x5b,  // Keypad 3
        90 => 0x62,  // Keypad 0
        91 => 0x63,  // Keypad .
        94 => 0x64,  // Non-US backslash
        95 => 0x44,  // F11
        96 => 0x45,  // F12
        104 => 0x58, // Keypad Enter
        105 => 0xe4, // Right Control
        106 => 0x54, // Keypad /
        107 => 0x46, // Print Screen
        108 => 0xe6, // Right Alt
        110 => 0x4a, // Home
        111 => 0x52, // Up
        112 => 0x4b, // Page Up
        113 => 0x50, // Left
        114 => 0x4f, // Right
        115 => 0x4d, // End
        116 => 0x51, // Down
        117 => 0x4e, // Page Down
        118 => 0x49, // Insert
        119 => 0x4c, // Delete
        125 => 0x67, // Keypad =
        127 => 0x48, // Pause
        129 => 0x85, // Keypad comma
        133 => 0xe3, // Left GUI
        134 => 0xe7, // Right GUI
        135 => 0x65, // Application
        191 => 0x68, // F13
        192 => 0x69, // F14
        193 => 0x6a, // F15
        194 => 0x6b, // F16
        195 => 0x6c, // F17
        196 => 0x6d, // F18
        197 => 0x6e, // F19
        198 => 0x6f, // F20
        199 => 0x70, // F21
        200 => 0x71, // F22
        201 => 0x72, // F23
        202 => 0x73, // F24
        _ => return None,
    };
    Some(usage)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    fn event(kind: KeyEventKind, keycode: u8) -> KeyEvent {
        KeyEvent { kind, keycode }
    }

    fn down(keycode: u8) -> KeyEvent {
        event(KeyEventKind::Down, keycode)
    }

    fn up(keycode: u8) -> KeyEvent {
        event(KeyEventKind::Up, keycode)
    }

    #[test]
    fn wayland_is_rejected_even_when_xwayland_display_exists() {
        assert_eq!(
            check_x11_session(Some(":0"), Some("wayland")),
            Err(WAYLAND_ERROR.to_string())
        );
        assert_eq!(
            check_x11_session(None, Some(" Wayland ")),
            Err(WAYLAND_ERROR.to_string())
        );
    }

    #[test]
    fn x11_requires_a_display() {
        assert!(check_x11_session(Some(":0"), Some("x11")).is_ok());
        assert!(check_x11_session(Some(":1"), None).is_ok());
        assert!(check_x11_session(None, Some("x11")).is_err());
        assert!(check_x11_session(Some("  "), Some("x11")).is_err());
    }

    #[test]
    fn letters_numbers_and_punctuation_map_to_hid() {
        assert_eq!(hid_usage(38), Some(0x04), "A");
        assert_eq!(hid_usage(24), Some(0x14), "Q");
        assert_eq!(hid_usage(52), Some(0x1d), "Z");
        assert_eq!(hid_usage(10), Some(0x1e), "1");
        assert_eq!(hid_usage(18), Some(0x26), "9");
        assert_eq!(hid_usage(19), Some(0x27), "0");
        assert_eq!(hid_usage(47), Some(0x33), ";");
        assert_eq!(hid_usage(48), Some(0x34), "apostrophe");
        assert_eq!(hid_usage(59), Some(0x36), "comma");
        assert_eq!(hid_usage(61), Some(0x38), "slash");
        assert_eq!(hid_usage(94), Some(0x64), "non-US backslash");
    }

    #[test]
    fn function_keys_cover_f1_through_f24() {
        for keycode in 67..=76 {
            assert_eq!(hid_usage(keycode), Some(i32::from(keycode) - 9));
        }
        assert_eq!(hid_usage(95), Some(0x44), "F11");
        assert_eq!(hid_usage(96), Some(0x45), "F12");
        for keycode in 191..=202 {
            assert_eq!(hid_usage(keycode), Some(i32::from(keycode) - 87));
        }
    }

    #[test]
    fn left_and_right_modifiers_are_distinct() {
        assert_eq!(hid_usage(37), Some(0xe0), "left Control");
        assert_eq!(hid_usage(105), Some(0xe4), "right Control");
        assert_eq!(hid_usage(50), Some(0xe1), "left Shift");
        assert_eq!(hid_usage(62), Some(0xe5), "right Shift");
        assert_eq!(hid_usage(64), Some(0xe2), "left Alt");
        assert_eq!(hid_usage(108), Some(0xe6), "right Alt");
        assert_eq!(hid_usage(133), Some(0xe3), "left GUI");
        assert_eq!(hid_usage(134), Some(0xe7), "right GUI");
    }

    #[test]
    fn editing_navigation_and_keypad_are_mapped() {
        assert_eq!(hid_usage(110), Some(0x4a), "Home");
        assert_eq!(hid_usage(111), Some(0x52), "Up");
        assert_eq!(hid_usage(113), Some(0x50), "Left");
        assert_eq!(hid_usage(118), Some(0x49), "Insert");
        assert_eq!(hid_usage(119), Some(0x4c), "Delete");
        assert_eq!(hid_usage(107), Some(0x46), "Print Screen");
        assert_eq!(hid_usage(127), Some(0x48), "Pause");
        assert_eq!(hid_usage(104), Some(0x58), "keypad Enter");
        assert_eq!(hid_usage(106), Some(0x54), "keypad slash");
        assert_eq!(hid_usage(90), Some(0x62), "keypad 0");
        assert_eq!(hid_usage(91), Some(0x63), "keypad dot");
        assert_eq!(hid_usage(125), Some(0x67), "keypad equal");
    }

    #[test]
    fn held_key_auto_repeat_is_counted_once() {
        let mut state = PressedState::default();
        assert_eq!(state.process(down(38)), Some(0x04));
        assert_eq!(state.process(down(38)), None);
        assert_eq!(state.process(down(38)), None);
    }

    #[test]
    fn release_allows_a_new_press_to_count() {
        let mut state = PressedState::default();
        assert_eq!(state.process(down(38)), Some(0x04));
        assert_eq!(state.process(up(38)), None);
        assert_eq!(state.process(down(38)), Some(0x04));
    }

    #[test]
    fn different_keys_have_independent_pressed_state() {
        let mut state = PressedState::default();
        assert_eq!(state.process(down(38)), Some(0x04));
        assert_eq!(state.process(down(39)), Some(0x16));
        assert_eq!(state.process(up(38)), None);
        assert_eq!(state.process(down(39)), None);
        assert_eq!(state.process(down(38)), Some(0x04));
    }

    #[test]
    fn unsupported_keys_are_skipped() {
        assert_eq!(hid_usage(8), None);
        assert_eq!(hid_usage(103), None);
        assert_eq!(hid_usage(u8::MAX), None);
        let mut state = PressedState::default();
        assert_eq!(state.process(down(103)), None);
        assert_eq!(state.process(down(103)), None);
        assert_eq!(state.process(up(103)), None);
    }

    #[test]
    fn record_packets_feed_the_same_pressed_state_machine() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let output = Arc::clone(&seen);
        let callback: KeyCallback = Arc::new(move |usage| {
            output
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .push(usage);
        });
        let mut state = PressedState::default();
        let mut packets = Vec::new();

        for (kind, keycode) in [
            (xproto::KEY_PRESS_EVENT, 38),
            (xproto::KEY_PRESS_EVENT, 38),
            (xproto::KEY_RELEASE_EVENT, 38),
            (xproto::KEY_PRESS_EVENT, 38),
        ] {
            let start = packets.len();
            packets.resize(start + CORE_EVENT_BYTES, 0);
            packets[start] = kind;
            packets[start + 1] = keycode;
        }
        dispatch_record_data(&packets, &mut state, &callback);

        assert_eq!(
            *seen.lock().unwrap_or_else(|poisoned| poisoned.into_inner()),
            vec![0x04, 0x04]
        );
    }
}
