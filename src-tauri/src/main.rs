//! 桌面端二进制入口
//!
//! 仅负责拉起 `screentime_pro_lib::run()`。
//! `windows_subsystem = "windows"` 让 Windows 发布版不弹出控制台黑窗。
//!
//! 修改历史：
//!   - 2026-07-08 @v0.2.0: 新增 - 桌面入口委托库运行，并隐藏 Windows 发布版控制台。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    screentime_pro_lib::run()
}
