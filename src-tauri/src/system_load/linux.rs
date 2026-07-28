//! Linux CPU 使用率监测（v0.6.2-beta.15）
//!
//! 从 /proc/stat 第一行（cpu aggregate）取 user/nice/system/idle/iowait/irq/softirq/steal ticks，
//! 差分即得使用率。

#![cfg(target_os = "linux")]

use std::fs;
use std::io;

use procfs::KernelStats;

#[derive(Default)]
pub struct Inner {
    last_total: Option<u64>,
    last_idle: Option<u64>,
}

impl Inner {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn cpu_usage(&mut self) -> Option<f32> {
        let k = match fs::read_to_string("/proc/stat") {
            Ok(s) => s,
            Err(_) => return None,
        };
        let first_line = k.lines().next()?;
        let mut it = first_line.split_whitespace();
        let name = it.next()?;
        if name != "cpu" {
            return None;
        }
        let mut idx = 0u32;
        let mut user = 0u64;
        let mut nice = 0u64;
        let mut system = 0u64;
        let mut idle = 0u64;
        let mut iowait = 0u64;
        let mut irq = 0u64;
        let mut softirq = 0u64;
        let mut steal = 0u64;
        for tok in it {
            let v: u64 = match tok.parse() {
                Ok(n) => n,
                Err(_) => break,
            };
            match idx {
                0 => user = v,
                1 => nice = v,
                2 => system = v,
                3 => idle = v,
                4 => iowait = v,
                5 => irq = v,
                6 => softirq = v,
                7 => steal = v,
                _ => {}
            }
            idx += 1;
        }
        let total = user + nice + system + idle + iowait + irq + softirq + steal;
        let idle_total = idle + iowait;
        let last_total = self.last_total;
        let last_idle = self.last_idle;
        self.last_total = Some(total);
        self.last_idle = Some(idle_total);
        let (lt, li) = (last_total?, last_idle?);
        let d_total = total.saturating_sub(lt);
        let d_idle = idle_total.saturating_sub(li);
        if d_total == 0 {
            return Some(0.0);
        }
        let used = d_total.saturating_sub(d_idle);
        Some((used as f32 / d_total as f32).clamp(0.0, 1.0))
    }
}

// 抑制 unused 警告
#[allow(dead_code)]
fn _unused_kernel(_: KernelStats) -> io::Result<()> {
    Ok(())
}
