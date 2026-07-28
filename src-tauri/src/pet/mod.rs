//!
//! pet/mod.rs
//! 桌宠子系统入口（v0.6.0-beta 引入）。
//!
//! 设计思路：
//! - 在主窗口之外独立维护一个透明置顶的 pet 窗口，所有桌宠 UI/动画/状态都在此窗口运行。
//! - pet 窗口的生命周期（创建/销毁/显示/隐藏/移动）通过命令由前端控制，避免与主窗口的事件循环耦合。
//! - 前台应用变化监听（macOS/Windows/Linux）单独放 foreground_watcher，模块解耦。
//!
//! 修改历史：
//!   - 2026-07-17 @v0.6.0-beta.1: 初始创建 - 桌宠窗口 Rust 端骨架
//!

pub mod window;
pub mod menu_window;

// 重新导出 window 模块的全部 pub 项（含 #[tauri::command] 宏生成的 __cmd__* 同伴函数）。
// 不能用 `pub use window::{...}` 具名导出——那只导出命令函数本体，
// 漏掉 generate_handler! 必需的 `pet::__cmd__create_pet_window` 等宏产物，会导致 E0433。
pub use window::*;
pub use menu_window::*;