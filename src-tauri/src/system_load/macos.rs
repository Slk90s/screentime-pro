//! system_load/macos.rs
//! macOS CPU 使用率监测。
//!
//! 实现：Mach `host_statistics(HOST_CPU_LOAD_INFO)` 读取**全机累积 CPU ticks**
//! （user / system / idle / nice 四个计数器），取两次采样的差值算使用率。
//!
//! ## ⚠️ v0.7.8 根因修复（2026-09-11）：macOS 状态栏 CPU 恒 0%
//!
//! 旧实现读 `sysctlbyname("kern.cp_time")`，但 **`kern.cp_time` 这个 OID 在 macOS
//! 上根本不存在**——它是 FreeBSD / OpenBSD / NetBSD / DragonFly 的内核接口。
//! Darwin 的 CPU 计数器只通过 Mach 的 `host_statistics(HOST_CPU_LOAD_INFO)` 暴露
//! （与 `libstatgrab` 的 macOS 实现、`gopsutil#31` 的结论一致）。
//! 因此 `sysctlbyname` 必然返回非 0（ENOENT），`read_cpu_ticks()` 恒 `None`，
//! `cpu_usage()` 恒回落 0.0 → **菜单栏 / 浮窗的 CPU 永远是 0%**。
//!
//! 注：v0.7.7 曾把该问题误判为「缓冲区宽度写死 16 字节」，改成探测大小后仍然失败——
//! 因为失败原因不是宽度，而是 **OID 本身不存在**。教训：**platform-only 代码的
//! 失败必须先确认「接口在该平台是否存在」，再怀疑参数细节**。
//!
//! ## 旧约定失效
//! - ❌ `kern.cp_time` 可用于 macOS → ✅ macOS 用 Mach `host_statistics`；
//!   `kern.cp_time` 仅 BSD 系（FreeBSD/NetBSD）有效。
//! - ❌ 「sysctl 探测缓冲区大小后按宽度自适应」是本模块的既有逻辑 → 已随 OID 更换删除。
//!
//! ## 为什么用 host_statistics 而不是 host_processor_info
//! - `host_processor_info(PROCESSOR_CPU_LOAD_INFO)` 是**按核**返回，且**会分配
//!   Mach VM 内存，调用方必须 `vm_deallocate`**，漏掉就泄漏（本项目早期代码隐患）。
//! - `host_statistics(HOST_CPU_LOAD_INFO)` 一次性给出**全机汇总**，写进调用方栈上
//!   结构体，零分配、零泄漏，正好满足「差分求总使用率」的需求。

#![cfg(target_os = "macos")]

/// Mach `host_cpu_load_info_data_t`：`natural_t cpu_ticks[CPU_STATE_MAX]`
/// （CPU_STATE_MAX = 4，natural_t = u32 → 16 字节）
///
/// 数组下标由 `CPU_STATE_*` 常量决定，顺序是 **user / system / idle / nice**
/// （注意：不是 user/nice/system/idle，与 `kern.cp_time` 的顺序不同，极易写错）。
#[repr(C)]
#[derive(Copy, Clone, Default)]
struct HostCpuLoadInfo {
    ticks: [u32; 4],
}

// 布局护栏：Mach 结构体定义漂移时 CI 直接编译失败，而不是静默读错值
const _: () = assert!(
    std::mem::size_of::<HostCpuLoadInfo>() == 16,
    "macOS host_cpu_load_info_data_t 应为 4 × natural_t(u32) = 16 字节"
);

extern "C" {
    fn mach_host_self() -> u32;
    /// `host_info_t` = `integer_t*`（i32*）；`host_info_outCnt` 单位是 **4 字节整数个数**
    fn host_statistics(host: u32, flavor: u32, info: *mut i32, count: *mut u32) -> i32;
}

/// `HOST_CPU_LOAD_INFO` = 3（见 `<mach/host_info.h>`）
const HOST_CPU_LOAD_INFO: u32 = 3;
/// `HOST_CPU_LOAD_INFO_COUNT` = 4（4 个 integer_t）
const HOST_CPU_LOAD_INFO_COUNT: u32 = 4;

/// 计数器下标（见 `<mach/machine.h>` 的 `CPU_STATE_*`）
const CPU_STATE_USER: usize = 0;
const CPU_STATE_SYSTEM: usize = 1;
const CPU_STATE_IDLE: usize = 2;
const CPU_STATE_NICE: usize = 3;

#[derive(Default)]
pub struct Inner {
    /// 上一次采样的原始 ticks (user, system, idle, nice)
    last: Option<[u32; 4]>,
}

impl Inner {
    pub fn new() -> Self {
        Self::default()
    }

    /// 返回 CPU 使用率**分数**（0.0~1.0，前端负责 ×100）。
    /// 首次调用只建立基准，返回 `None`（调用方按 0 处理）。
    pub fn cpu_usage(&mut self) -> Option<f32> {
        let cur = read_cpu_ticks()?;
        let prev = self.last.replace(cur)?;

        // ticks 是 u32 计数器，长时间运行会回绕 → 必须用 wrapping_sub，
        // 不能用 saturating_sub（回绕时会得到 0，表现为「偶发 0%」）。
        let d_user = cur[CPU_STATE_USER].wrapping_sub(prev[CPU_STATE_USER]) as u64;
        let d_system = cur[CPU_STATE_SYSTEM].wrapping_sub(prev[CPU_STATE_SYSTEM]) as u64;
        let d_idle = cur[CPU_STATE_IDLE].wrapping_sub(prev[CPU_STATE_IDLE]) as u64;
        let d_nice = cur[CPU_STATE_NICE].wrapping_sub(prev[CPU_STATE_NICE]) as u64;

        let total = d_user + d_system + d_idle + d_nice;
        if total == 0 {
            // 两次采样之间没有任何 tick 推进（间隔过短）→ 视为 0，不产生除零
            return Some(0.0);
        }
        let used = d_user + d_system + d_nice;
        Some((used as f32 / total as f32).clamp(0.0, 1.0))
    }
}

/// 读一次全机 CPU 累积 ticks（user / system / idle / nice）。
///
/// 失败（Mach 调用被拒）返回 `None`，调用方会按 0 处理并记日志。
fn read_cpu_ticks() -> Option<[u32; 4]> {
    let mut info = HostCpuLoadInfo::default();
    let mut count: u32 = HOST_CPU_LOAD_INFO_COUNT;
    let rc = unsafe {
        host_statistics(
            mach_host_self(),
            HOST_CPU_LOAD_INFO,
            info.ticks.as_mut_ptr() as *mut i32,
            &mut count,
        )
    };
    if rc != 0 {
        tracing::warn!(rc, "host_statistics(HOST_CPU_LOAD_INFO) 失败，本次 CPU 记为 0");
        return None;
    }
    if count < HOST_CPU_LOAD_INFO_COUNT {
        tracing::warn!(count, "host_statistics 返回字段数不足，本次 CPU 记为 0");
        return None;
    }
    if info.ticks.iter().all(|&t| t == 0) {
        // 全 0 说明拿到的不是有效数据，宁可返回 None 也不要假装 0%
        return None;
    }
    Some(info.ticks)
}
