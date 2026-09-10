//! macOS CPU 使用率监测（v0.6.2-beta.15）
//!
//! 实现：用 `sysctlbyname("kern.cp_time")` 读取全 CPU 累积 ticks（CP_USER/CP_NICE/CP_SYS/CP_IDLE）
//! 该接口稳定、不分配堆内存；差分即得使用率。
//!
//! ⚠️ v0.7.7（2026-09-10）修复字段宽度假设：
//!   旧实现把返回缓冲写死成 4×u32 = 16 字节。`kern.cp_time` 是 `long cp_time[4]`，
//!   在 64 位内核上是 **4×u64 = 32 字节**——旧代码会因 oldlen 太小被 sysctl 判 ENOMEM
//!   （安全返回 None，不会越界写），但结果是 macOS 上 CPU 恒为 0%。
//!   现在改为「先问所需大小 → 按大小自适应解析 u32/u64」，两种内核行为都正确。

#![cfg(target_os = "macos")]

#[repr(C)]
#[derive(Copy, Clone, Default)]
struct CpTime {
    user: u64,
    nice: u64,
    system: u64,
    idle: u64,
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
        let total = t.user + t.system + t.nice + t.idle;
        let idle = t.idle;
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

/// 读 `kern.cp_time`：4 个 tick 计数（user / nice / system / idle）
///
/// 两次调用 sysctl：第一次 `oldp = NULL` 只回填所需字节数，第二次按真实大小读；
/// 元素宽度按 `size` 自适应（16 字节 = u32×4，32 字节 = u64×4）。
fn read_cp_time() -> Option<CpTime> {
    let name = b"kern.cp_time\0";
    let name_ptr = name.as_ptr() as *const i8;

    // ① 探测所需缓冲区大小
    let mut size: usize = 0;
    let rc = unsafe {
        sysctlbyname(
            name_ptr,
            std::ptr::null_mut(),
            &mut size,
            std::ptr::null(),
            0,
        )
    };
    if rc != 0 || size < 16 || size > 1024 {
        return None;
    }

    // ② 按真实大小读取
    let mut buf = vec![0u8; size];
    let mut got = size;
    let rc = unsafe {
        sysctlbyname(
            name_ptr,
            buf.as_mut_ptr() as *mut std::ffi::c_void,
            &mut got,
            std::ptr::null(),
            0,
        )
    };
    if rc != 0 {
        return None;
    }
    let n = got.min(buf.len());
    let wide = n >= 32 && n % 8 == 0;
    let step = if wide { 8 } else { 4 };
    if n < step * 4 {
        return None;
    }

    let read_at = |idx: usize| -> u64 {
        let off = idx * step;
        if off + step > n {
            return 0;
        }
        if wide {
            let mut b = [0u8; 8];
            b.copy_from_slice(&buf[off..off + 8]);
            u64::from_ne_bytes(b)
        } else {
            let mut b = [0u8; 4];
            b.copy_from_slice(&buf[off..off + 4]);
            u32::from_ne_bytes(b) as u64
        }
    };
    let t = CpTime {
        user: read_at(0),
        nice: read_at(1),
        system: read_at(2),
        idle: read_at(3),
    };
    if t.user + t.nice + t.system + t.idle == 0 {
        return None;
    }
    Some(t)
}
