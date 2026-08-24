//! system_load/metrics.rs
//! 系统托盘指标采样器（v0.7.5 新增）。
//!
//! 设计：
//! - 复用 CpuMonitor 的 CPU 采样（零重复代码）
//! - 新增 macOS 内存（host_statistics64 + hw.memsize）与磁盘（statfs）
//! - 单线程统一调度：1s 采 CPU/内存，磁盘采后缓存避免频繁 statfs
//! - 采样结果直接驱动托盘 title，绕开 IPC 往返，零前端开销
//!
//! macOS 内存口径说明（对齐活动监视器）：
//!   available = (free + inactive + speculative) * page_size
//!   used%     = (total - available) / total
//!   不直接用 free_count（macOS 会用空闲内存做文件缓存，free 经常接近 0）
//!
//! 磁盘口径（对齐 Finder「读取信息」）：
//!   total = f_blocks * f_frsize
//!   free  = f_bfree * f_frsize
//!   used  = total - free

#![cfg(target_os = "macos")]

use crate::system_load::CpuMonitor;
use std::sync::{Arc, Mutex};

extern "C" {
    fn sysctl(
        name: *const i8,
        namelen: u32,
        oldp: *mut std::ffi::c_void,
        oldlenp: *mut usize,
        newp: *const std::ffi::c_void,
        newlen: usize,
    ) -> i32;
    fn mach_host_self() -> u32;
    fn host_statistics64(
        host: u32,
        flavor: u32,
        info: *mut u8,
        count: *mut u32,
    ) -> i32;
    fn statfs(path: *const i8, buf: *mut u8) -> i32;
}

#[derive(Debug, Clone, Default)]
pub struct MetricsSnapshot {
    pub cpu_usage: f32,
    pub memory_usage: f32,
    pub memory_used_bytes: u64,
    pub memory_total_bytes: u64,
    pub disk_usage: f32,
    pub disk_used_bytes: u64,
    pub disk_total_bytes: u64,
}

pub struct MetricsSampler {
    cpu: Arc<CpuMonitor>,
    inner: Mutex<MetricsInner>,
}

struct MetricsInner {
    page_size: u64,
    memory_total: u64,
    last_disk: Option<(u64, u64)>,
    last_disk_secs: u64,
    last_snapshot: Option<MetricsSnapshot>,
}

impl MetricsSampler {
    pub fn new(cpu: Arc<CpuMonitor>) -> Self {
        let (page_size, memory_total) = Self::init_memory_constants();
        Self {
            cpu,
            inner: Mutex::new(MetricsInner {
                page_size,
                memory_total,
                last_disk: None,
                last_disk_secs: 0,
                last_snapshot: None,
            }),
        }
    }

    pub fn sample_all(&self) -> MetricsSnapshot {
        let cpu_usage = self.cpu.cpu_usage().unwrap_or(0.0);
        let (mem_used, mem_total) = self.sample_memory();
        let mem_pct = if mem_total > 0 {
            (mem_used as f64 / mem_total as f64).clamp(0.0, 1.0) as f32
        } else {
            0.0
        };
        let (disk_used, disk_total) = self.sample_disk_cached();
        let disk_pct = if disk_total > 0 {
            (disk_used as f64 / disk_total as f64).clamp(0.0, 1.0) as f32
        } else {
            0.0
        };
        MetricsSnapshot {
            cpu_usage: cpu_usage.clamp(0.0, 1.0),
            memory_usage: mem_pct,
            memory_used_bytes: mem_used,
            memory_total_bytes: mem_total,
            disk_usage: disk_pct,
            disk_used_bytes: disk_used,
            disk_total_bytes: disk_total,
        }
    }

    pub fn tray_title(&self, snap: &MetricsSnapshot) -> String {
        let cpu = (snap.cpu_usage * 100.0).round() as u32;
        let mem = (snap.memory_usage * 100.0).round() as u32;
        let dsk = (snap.disk_usage * 100.0).round() as u32;
        format!("C{} M{} D{}", cpu, mem, dsk)
    }

    pub fn should_update(&self, snap: &MetricsSnapshot) -> bool {
        let inner = match self.inner.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        if let Some(ref last) = inner.last_snapshot {
            let d_cpu = ((snap.cpu_usage - last.cpu_usage).abs() * 100.0).round() as u32;
            let d_mem = ((snap.memory_usage - last.memory_usage).abs() * 100.0).round() as u32;
            let d_dsk = ((snap.disk_usage - last.disk_usage).abs() * 100.0).round() as u32;
            if d_cpu < 1 && d_mem < 1 && d_dsk < 1 {
                return false;
            }
        }
        true
    }

    pub fn cache_snapshot(&self, snap: MetricsSnapshot) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.last_snapshot = Some(snap);
        }
    }

    fn init_memory_constants() -> (u64, u64) {
        let page_size = unsafe { Self::read_page_size() };
        let total = unsafe { Self::read_memory_total() };
        (page_size.max(4096), total)
    }

    fn sample_memory(&self) -> (u64, u64) {
        let (page_size, memory_total) = match self.inner.lock() {
            Ok(inner) => (inner.page_size, inner.memory_total),
            Err(poisoned) => {
                let inner = poisoned.into_inner();
                (inner.page_size, inner.memory_total)
            }
        };
        let stats = unsafe { Self::read_vm_stats() };
        if let Some(st) = stats {
            let avail_pages = st.free_count + st.inactive_count + st.speculative_count;
            let avail_bytes = avail_pages.saturating_mul(page_size);
            let used = memory_total.saturating_sub(avail_bytes);
            (used, memory_total)
        } else {
            (0, memory_total)
        }
    }

    fn sample_disk_cached(&self) -> (u64, u64) {
        let now_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        {
            let inner = match self.inner.lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            if let (Some((used, total)), last_secs) = (inner.last_disk, inner.last_disk_secs) {
                if now_secs.saturating_sub(last_secs) < 30 {
                    return (used, total);
                }
            }
        }
        let (used, total) = unsafe { Self::read_disk_usage() };
        if used > 0 && total > 0 {
            if let Ok(mut inner) = self.inner.lock() {
                inner.last_disk = Some((used, total));
                inner.last_disk_secs = now_secs;
            }
        }
        (used, total)
    }

    unsafe fn read_page_size() -> u64 {
        let name = b"hw.pagesize\0";
        let mut page_size: u32 = 0;
        let mut len = std::mem::size_of::<u32>();
        let rc = sysctl(
            name.as_ptr() as *const i8,
            name.len() as u32 - 1,
            &mut page_size as *mut u32 as *mut std::ffi::c_void,
            &mut len,
            std::ptr::null(),
            0,
        );
        if rc == 0 && page_size > 0 {
            page_size as u64
        } else {
            4096
        }
    }

    unsafe fn read_memory_total() -> u64 {
        let name = b"hw.memsize\0";
        let mut total: u64 = 0;
        let mut len = std::mem::size_of::<u64>();
        let rc = sysctl(
            name.as_ptr() as *const i8,
            name.len() as u32 - 1,
            &mut total as *mut u64 as *mut std::ffi::c_void,
            &mut len,
            std::ptr::null(),
            0,
        );
        if rc == 0 && total > 0 {
            total
        } else {
            0
        }
    }

    unsafe fn read_vm_stats() -> Option<VmStats64> {
        const HOST_VM_INFO64: u32 = 4;
        const BUF_U32_COUNT: u32 = 128;
        const BUF_SIZE: usize = (BUF_U32_COUNT as usize) * 4;
        let host = mach_host_self();
        let mut buf = vec![0u8; BUF_SIZE];
        let mut count: u32 = BUF_U32_COUNT;
        let rc = host_statistics64(host, HOST_VM_INFO64, buf.as_mut_ptr(), &mut count);
        if rc != 0 {
            tracing::warn!(rc, "host_statistics64 failed, returning no memory stats");
            return None;
        }
        if count < 36 {
            tracing::warn!(count, "host_statistics64 returned too few fields");
            return None;
        }
        let free = u64::from_le_bytes(buf[8..16].try_into().unwrap_or([0u8; 8]));
        let inactive = u64::from_le_bytes(buf[24..32].try_into().unwrap_or([0u8; 8]));
        let speculative = u64::from_le_bytes(buf[32..40].try_into().unwrap_or([0u8; 8]));
        Some(VmStats64 {
            free_count: free,
            inactive_count: inactive,
            speculative_count: speculative,
        })
    }

    unsafe fn read_disk_usage() -> (u64, u64) {
        let mut buf = [0u8; 256];
        let path = b"/\0";
        let rc = statfs(path.as_ptr() as *const i8, buf.as_mut_ptr());
        if rc != 0 {
            return (0, 0);
        }
        let f_blocks = u64::from_le_bytes(buf[8..16].try_into().unwrap_or([0u8; 8]));
        let f_bfree = u64::from_le_bytes(buf[16..24].try_into().unwrap_or([0u8; 8]));
        let f_frsize = u64::from_le_bytes(buf[72..80].try_into().unwrap_or([0u8; 8]));
        if f_frsize == 0 || f_blocks == 0 {
            return (0, 0);
        }
        let total = f_blocks.saturating_mul(f_frsize);
        let free = f_bfree.saturating_mul(f_frsize);
        let used = total.saturating_sub(free);
        (used, total)
    }
}

#[derive(Debug, Clone)]
struct VmStats64 {
    free_count: u64,
    inactive_count: u64,
    speculative_count: u64,
}
