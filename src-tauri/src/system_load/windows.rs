//! Windows CPU 使用率监测（GetSystemTimes）
//!
//! 取 idle / kernel / user 三个时长；diff 计算使用率。
//! 用 windows crate 0.58 内置签名（已在 Cargo.toml 中启用 Win32_System_SystemInformation）。

#![cfg(target_os = "windows")]

use windows::Win32::Foundation::FILETIME;
use windows::Win32::System::SystemInformation::GetSystemTimes;

#[derive(Default)]
pub struct Inner {
    last_idle: Option<u64>,
    last_total: Option<u64>,
}

impl Inner {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn cpu_usage(&mut self) -> Option<f32> {
        let (idle, kernel, user) = read_times()?;
        // idle 包含 kernel 的空闲部分，但我们用另外两个相加更清晰
        let total = kernel + user;
        // 差分
        let last_idle = self.last_idle;
        let last_total = self.last_total;
        self.last_idle = Some(idle);
        self.last_total = Some(total);
        let (li, lt) = (last_idle?, last_total?);
        let d_total = total.saturating_sub(lt);
        let d_idle = idle.saturating_sub(li);
        if d_total == 0 {
            return Some(0.0);
        }
        let used = d_total.saturating_sub(d_idle);
        Some((used as f32 / d_total as f32).clamp(0.0, 1.0))
    }
}

fn filetime_u64(ft: FILETIME) -> u64 {
    ((ft.dwHighDateTime as u64) << 32) | (ft.dwLowDateTime as u64)
}

fn read_times() -> Option<(u64, u64, u64)> {
    let mut idle = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    unsafe {
        GetSystemTimes(Some(&mut idle), Some(&mut kernel), Some(&mut user))
            .ok()?;
    }
    Some((filetime_u64(idle), filetime_u64(kernel), filetime_u64(user)))
}
