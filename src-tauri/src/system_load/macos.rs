//! macOS CPU 使用率监测（v0.6.2-beta.15）
//!
//! 实现：用 `sysctlbyname("kern.cp_time")` 读取全 CPU 累积 ticks（CP_USER/CP_NICE/CP_SYS/CP_IDLE）
//! 该接口稳定、不分配堆内存；差分即得使用率。

#![cfg(target_os = "macos")]

use std::mem::MaybeUninit;

#[repr(C)]
#[derive(Copy, Clone, Default)]
struct CpTime {
    user: u32,
    nice: u32,
    system: u32,
    idle: u32,
}

extern "C" {
    fn sysctlbyname(
        name: *const i8,
        oldp: *mut std::ffi::c_void,
        oldlenp: *mut usize,
        newp: *const std::ffi::c_void,
        newlen: usize,
    ) -> i32;
}

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
        let t = read_cp_time()?;
        let total = t.user as u64 + t.system as u64 + t.nice as u64 + t.idle as u64;
        let idle = t.idle as u64;
        let last_total = self.last_total;
        let last_idle = self.last_idle;
        self.last_total = Some(total);
        self.last_idle = Some(idle);
        let (lt, li) = (last_total?, last_idle?);
        let d_total = total.saturating_sub(lt);
        let d_idle = idle.saturating_sub(li);
        if d_total == 0 {
            return Some(0.0);
        }
        let used = d_total.saturating_sub(d_idle);
        Some((used as f32 / d_total as f32).clamp(0.0, 1.0))
    }
}

fn read_cp_time() -> Option<CpTime> {
    let name = b"kern.cp_time\0";
    let mut val: CpTime = CpTime::default();
    let mut size = std::mem::size_of::<CpTime>();
    let rc = unsafe {
        sysctlbyname(
            name.as_ptr() as *const i8,
            &mut val as *mut CpTime as *mut std::ffi::c_void,
            &mut size,
            std::ptr::null(),
            0,
        )
    };
    if rc != 0 {
        return None;
    }
    Some(val)
}

// 抑制未使用导入 warn
const _UNUSED: MaybeUninit<u8> = MaybeUninit::uninit();
