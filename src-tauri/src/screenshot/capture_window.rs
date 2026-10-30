//!
//! capture_window.rs — 截图遮罩窗的平台层修正（v1.2.2，2026-10-21）。
//!
//! 为什么需要这个模块
//!   `ensure_capture_window()` 用 tauri 的 `.always_on_top(true)` 建窗，而 tao 在
//!   macOS 上把 always_on_top 映射为 `NSFloatingWindowLevel`（**3**）。菜单栏是
//!   `NSMainMenuWindowLevel`（**24**）、状态栏项是 `NSStatusWindowLevel`（25）。
//!   于是遮罩窗的层级**低于菜单栏**，AppKit 会把它的 frame 约束在
//!   `visibleFrame`（已扣掉菜单栏那条）里 —— 两个症状由此产生：
//!     ① 顶部菜单栏那一行永远盖不到，`全屏` 也选不进去；
//!     ② 窗口高度被砍掉约 25pt，而冻结帧是按整屏物理像素抓的、`.shot` 用
//!        `width/height:100%` 强行拉伸铺满窗口 → 画面纵向被压扁，视觉上像
//!        「全屏变画中画里的一张小图」。用户实机反馈的正是这两条。
//!
//! 修法
//!   建窗后把 NSWindow 的 level 提到 `NSScreenSaverWindowLevel`（1000）：
//!   高于菜单栏、Dock、通知中心，遮蔽整块屏幕。同时补
//!   `CanJoinAllSpaces | Stationary | FullScreenAuxiliary`：
//!     - CanJoinAllSpaces   ：用户在别的 Space（含全屏 App）里按快捷键也能盖住；
//!     - Stationary         ：切 Space 动画期间窗口不跟着漂移；
//!     - FullScreenAuxiliary：允许覆盖在全屏 App 之上（默认会被拒）。
//!   隐藏/关闭时**恢复原 level**，避免遮罩窗在非截图期间仍然压着菜单栏
//!   （虽然它 visible=false，但恢复原值是最小惊讶原则）。
//!
//! ⚠️ 落地注意
//!   1. 必须在**主线程**操作 NSWindow。本模块所有函数都由
//!      `run_on_main_thread` 派发（调用方 `mod.rs` 已保证），不在子线程直接调。
//!   2. `ns_window()` 拿到的指针原始所有权属于 tauri，**不可 retain/release**，
//!      这里只做 `&NSWindow` 借用（`ptr::NonNull` 转引用，不转移所有权）。
//!   3. 只在 macOS 编译；其他平台整体 cfg 掉，调用点用同名空实现。
//!
//! 修改历史：
//!   - 2026-10-21 @v1.2.2: 新增 - 遮罩窗 NSWindow level 提升到 screenSaver 级，
//!     修复「全屏截图盖不到菜单栏 / 全屏变画中画」两个同源症状。

#[cfg(target_os = "macos")]
mod imp {
    use objc2_app_kit::{
        NSFloatingWindowLevel, NSScreenSaverWindowLevel, NSWindow, NSWindowCollectionBehavior,
    };
    use std::ptr::NonNull;
    use tauri::{Manager, WebviewWindow};

    /// 取 tauri 窗口底层的 `*mut NSWindow`。
    ///
    /// tauri 的 `ns_window()` 返回 `*mut c_void`，所有权仍归 tauri ——
    /// 这里只是借用成 `&NSWindow`，绝不能 retain/release。
    ///
    /// Safety: 必须在主线程调用；指针非空且指向有效的 NSWindow。
    fn ns_window(window: &WebviewWindow) -> Option<&'static NSWindow> {
        let ptr = window.ns_window().ok()? as *mut NSWindow;
        let nn = NonNull::new(ptr)?;
        // Safety: tauri 保证该指针在窗口存活期间有效；我们只借用不改所有权。
        Some(unsafe { nn.as_ref() })
    }

    /// 把遮罩窗提到「屏幕保护程序」层级，盖住菜单栏 / Dock / 全屏 App。
    ///
    /// 幂等：重复调用结果一致（level 已是目标值也不会有副作用）。
    pub fn raise_above_menu_bar(window: &WebviewWindow) -> Result<(), String> {
        let w = ns_window(window).ok_or_else(|| "取 NSWindow 失败".to_string())?;
        // Safety: AppKit 窗口属性修改要求在主线程序列上执行；调用方已用
        // run_on_main_thread 派发（见模块头落地注意 #1）。
        unsafe {
            w.setLevel(NSScreenSaverWindowLevel);
            w.setCollectionBehavior(
                NSWindowCollectionBehavior::CanJoinAllSpaces
                    | NSWindowCollectionBehavior::Stationary
                    | NSWindowCollectionBehavior::FullScreenAuxiliary,
            );
        }
        tracing::info!(
            level = NSScreenSaverWindowLevel,
            "遮罩窗已提升到 screenSaver 层级（覆盖菜单栏 / Dock / 全屏 App）"
        );
        Ok(())
    }

    /// 恢复 tao 默认的浮动层级（隐藏遮罩窗时调用）。
    pub fn restore_default_level(window: &WebviewWindow) -> Result<(), String> {
        let w = ns_window(window).ok_or_else(|| "取 NSWindow 失败".to_string())?;
        // Safety: 同 raise_above_menu_bar。
        unsafe {
            w.setLevel(NSFloatingWindowLevel);
            w.setCollectionBehavior(NSWindowCollectionBehavior::Default);
        }
        Ok(())
    }

    /// 便捷封装：解析 label → 在主线程提升层级（阻塞等待结果）。
    ///
    /// 调用方不需要自己写 `run_on_main_thread`。用 channel 回传结果是为了让
    /// 失败原因（拿不到窗口 / NSWindow 指针空）能落到日志里，而不是静默吞掉。
    pub fn raise_capture_window(app: &tauri::AppHandle, label: &str) -> Result<(), String> {
        let win = app
            .get_webview_window(label)
            .ok_or_else(|| format!("窗口 `{label}` 不存在"))?;
        let (tx, rx) = std::sync::mpsc::channel();
        let app2 = app.clone();
        let label2 = label.to_string();
        app.run_on_main_thread(move || {
            let r = match app2.get_webview_window(&label2) {
                Some(w) => raise_above_menu_bar(&w),
                None => Err(format!("窗口 `{label2}` 已销毁")),
            };
            let _ = tx.send(r);
        })
        .map_err(|e| format!("主线程派发失败: {e}"))?;
        // 主线程同步执行（tauri-runtime-wry 的 send_user_message 在主线程上内联跑），
        // 所以这里几乎立即返回；给 2s 上限只为防御极端调度。
        rx.recv_timeout(std::time::Duration::from_secs(2))
            .map_err(|_| "等待主线程提升层级超时".to_string())?
    }
    /// 便捷封装：解析 label → 在主线程恢复默认层级（不等待结果）。
    ///
    /// 与 `raise_capture_window` 不同，这里**不等回执**：调用点是隐藏流程，
    /// 恢复层级失败完全无害（窗口随即 visible=false），不值得让调用方阻塞。
    pub fn restore_capture_window(app: &tauri::AppHandle, label: &str) {
        let app2 = app.clone();
        let label2 = label.to_string();
        let _ = app.run_on_main_thread(move || {
            if let Some(w) = app2.get_webview_window(&label2) {
                if let Err(e) = restore_default_level(&w) {
                    tracing::debug!(error = %e, "恢复遮罩窗默认层级失败（无害）");
                }
            }
        });
    }
}

#[cfg(target_os = "macos")]
pub use imp::{raise_capture_window, restore_capture_window};

// ===== 非 macOS：空实现 =====
// Windows / Linux 上 always_on_top 已足够（无菜单栏层级约束），无需额外处理。
// 保留同名 API 是为了让 mod.rs 的调用点不必到处写 #[cfg]。
//
// ⚠️ 只导出 `raise_capture_window` / `restore_capture_window` 这两个**被调用点直接使用**
// 的封装。底层原语 `raise_above_menu_bar` / `restore_default_level` 是 macOS imp 模块的
// 内部细节（只被 imp 里的两个封装调用），既不必导出、也不要在非 macOS 侧留同名空壳——
// 否则 Windows 构建会因为「声明了但没人调」多出两条 dead_code 警告。

#[cfg(not(target_os = "macos"))]
pub fn raise_capture_window(_app: &tauri::AppHandle, _label: &str) -> Result<(), String> {
    Ok(())
}

#[cfg(not(target_os = "macos"))]
pub fn restore_capture_window(_app: &tauri::AppHandle, _label: &str) {}
