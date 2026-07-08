//! 桌面端二进制入口
//!
//! 仅负责拉起 `screentime_pro_lib::run()`。
//! `windows_subsystem = "windows"` 让 Windows 发布版不弹出控制台黑窗。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    screentime_pro_lib::run()
}
