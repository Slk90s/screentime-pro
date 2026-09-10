//! system_load/network.rs
//! 跨平台网络速率采样（v0.7.6 新增）。
//!
//! 设计：
//! - 复用已有「手写 FFI / 系统调用」风格（参考 macos.rs / windows.rs / linux.rs）
//! - 跨平台：macOS 用 getifaddrs + 解析 if_data，Windows 用 GetIfTable2，Linux 读 /proc/net/dev
//! - 计算口径：排除 lo（回环）+ down 状态的接口；每秒调用一次 sample_rate 算 bytes/sec
//! - 数字 32 位溢出风险：单接口 ≤4GB/s 时 u32 安全；保守起见 macOS 用 u64（freeifaddrs 后转 u64）
//!
//! 防 Agent 幻视：此模块于 v0.7.6 引入，取代了「用 netstat 命令行解析」的临时方案（已弃）。
//! 旧 netstat 方案因 macOS 不同版本列格式差异（`-I` vs `-b`）导致偶发解析失败被剔除。

use std::sync::Mutex;
use std::time::Instant;

#[derive(Debug, Clone, Default)]
pub struct NetworkSnapshot {
    /// 接收速率（bytes per second），非负
    pub rx_bps: f64,
    /// 发送速率（bytes per second），非负
    pub tx_bps: f64,
}

pub struct NetworkSampler {
    inner: Mutex<NetworkInner>,
}

#[derive(Default)]
struct NetworkInner {
    last_total_rx: Option<u64>,
    last_total_tx: Option<u64>,
    last_sample_at: Option<Instant>,
}

impl NetworkSampler {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(NetworkInner::default()),
        }
    }

    /// 取当前所有 UP 非 lo 接口的累计字节数；返回 (rx_total, tx_total)
    pub fn sample_totals(&self) -> (u64, u64) {
        read_iface_totals()
    }

    /// 计算自上次 sample_rate 以来接收/发送的字节速率（bytes/sec）
    pub fn sample_rate(&self) -> NetworkSnapshot {
        let (rx_now, tx_now) = self.sample_totals();
        let mut inner = match self.inner.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        let now = Instant::now();
        let snap = match (inner.last_total_rx, inner.last_total_tx, inner.last_sample_at) {
            (Some(lr), Some(lt), Some(at)) => {
                let elapsed = now.duration_since(at).as_secs_f64();
                if elapsed > 0.0 {
                    let dr = rx_now.saturating_sub(lr);
                    let dt = tx_now.saturating_sub(lt);
                    NetworkSnapshot {
                        rx_bps: (dr as f64 / elapsed).max(0.0),
                        tx_bps: (dt as f64 / elapsed).max(0.0),
                    }
                } else {
                    NetworkSnapshot::default()
                }
            }
            _ => NetworkSnapshot::default(),
        };
        inner.last_total_rx = Some(rx_now);
        inner.last_total_tx = Some(tx_now);
        inner.last_sample_at = Some(now);
        snap
    }
}

impl Default for NetworkSampler {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================
// 平台实现
// ============================================================

#[cfg(target_os = "macos")]
fn read_iface_totals() -> (u64, u64) {
    use std::ffi::CStr;
    unsafe {
        let mut head: *mut libc::ifaddrs = std::ptr::null_mut();
        if libc::getifaddrs(&mut head) != 0 || head.is_null() {
            return (0, 0);
        }
        let mut total_rx: u64 = 0;
        let mut total_tx: u64 = 0;
        let mut cur = head;
        while !cur.is_null() {
            let ifa = &*cur;
            // 跳过 NULL 接口
            if ifa.ifa_addr.is_null() {
                cur = ifa.ifa_next;
                continue;
            }
            // 只统计 UP 且非 lo 的接口
            let flags = ifa.ifa_flags as i32;
            if (flags & libc::IFF_UP) == 0 || (flags & libc::IFF_LOOPBACK) != 0 {
                cur = ifa.ifa_next;
                continue;
            }
            // 跳过非 AF_LINK 接口（macOS 上 ifa_data 只对 AF_LINK 有效）
            if (*ifa.ifa_addr).sa_family != libc::AF_LINK as u8 {
                cur = ifa.ifa_next;
                continue;
            }
            let name_c = CStr::from_ptr(ifa.ifa_name);
            let name = name_c.to_string_lossy();
            if name == "lo0" {
                cur = ifa.ifa_next;
                continue;
            }
            if !ifa.ifa_data.is_null() {
                let ifd = ifa.ifa_data as *const IfData;
                let rx = (*ifd).ifi_ibytes as u64;
                let tx = (*ifd).ifi_obytes as u64;
                total_rx = total_rx.saturating_add(rx);
                total_tx = total_tx.saturating_add(tx);
            }
            cur = ifa.ifa_next;
        }
        libc::freeifaddrs(head);
        (total_rx, total_tx)
    }
}

/// macOS `if_data` 结构（精简版，ifi_ibytes 在 offset 40，ifi_obytes 在 offset 44）
/// 完整字段参见 `<net/if.h>` / `if_data`。
#[cfg(target_os = "macos")]
#[repr(C)]
struct IfData {
    ifi_type: u8,
    ifi_typelen: u8,
    ifi_physical: u8,
    ifi_addrlen: u8,
    ifi_hdrlen: u8,
    ifi_recvquota: u8,
    ifi_xmitquota: u8,
    ifi_unused1: u8,
    ifi_mtu: u32,
    ifi_metric: u32,
    ifi_baudrate: u32,
    ifi_ipackets: u32,
    ifi_ierrors: u32,
    ifi_opackets: u32,
    ifi_oerrors: u32,
    ifi_collisions: u32,
    ifi_ibytes: u32, // 接收字节数
    ifi_obytes: u32, // 发送字节数
    ifi_imcasts: u32,
    ifi_omcasts: u32,
    ifi_iqdrops: u32,
    ifi_noproto: u32,
    ifi_recvtiming: u32,
    ifi_xmittiming: u32,
    ifi_lastchange: libc::timeval,
}

#[cfg(target_os = "windows")]
fn read_iface_totals() -> (u64, u64) {
    use windows::Win32::NetworkManagement::IpHelper::{
        GetIfTable2, FreeMibTable, MIB_IF_TABLE2, IF_TYPE_SOFTWARE_LOOPBACK,
    };
    use windows::Win32::NetworkManagement::Ndis::IF_OPER_STATUS;
    // IfOperStatusUp = 1（参考 Microsoft Docs NET_IF_OPER_STATUS 枚举）
    const IF_OPER_STATUS_UP: IF_OPER_STATUS = IF_OPER_STATUS(1);
    unsafe {
        let mut table: *mut MIB_IF_TABLE2 = std::ptr::null_mut();
        let rc = GetIfTable2(&mut table);
        if rc.0 != 0 || table.is_null() {
            return (0, 0);
        }
        let t = &*table;
        let mut total_rx: u64 = 0;
        let mut total_tx: u64 = 0;
        for i in 0..t.NumEntries as usize {
            // Table 是 [MIB_IF_ROW2; 1] 柔性数组起点；用指针偏移避免越界
            let row = &*t.Table.as_ptr().add(i);
            // 只统计 OperStatus = Up 且非回环的接口
            if row.OperStatus != IF_OPER_STATUS_UP {
                continue;
            }
            if row.Type == IF_TYPE_SOFTWARE_LOOPBACK {
                continue;
            }
            total_rx = total_rx.saturating_add(row.InOctets as u64);
            total_tx = total_tx.saturating_add(row.OutOctets as u64);
        }
        FreeMibTable(table as *const _);
        (total_rx, total_tx)
    }
}

#[cfg(target_os = "linux")]
fn read_iface_totals() -> (u64, u64) {
    // /proc/net/dev 格式：
    //   Inter-|   Receive                                                |  Transmit
    //    face |bytes    packets errs drop fifo frame compressed multicast|bytes    ...
    //   lo: 1234 ...
    //   eth0: ...
    let s = match std::fs::read_to_string("/proc/net/dev") {
        Ok(s) => s,
        Err(_) => return (0, 0),
    };
    let mut total_rx: u64 = 0;
    let mut total_tx: u64 = 0;
    for line in s.lines().skip(2) {
        // 格式：<name>:<whitespace><rx_bytes> <rx_packets> ...
        let Some(colon) = line.find(':') else { continue; };
        let name = line[..colon].trim();
        if name == "lo" {
            continue;
        }
        let rest = &line[colon + 1..];
        let mut it = rest.split_whitespace();
        // Receive 栏 8 个字段，第 1 个(index 0)是 rx_bytes
        let rx = it.next().and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
        // 此时迭代器已在 index 0 之后；nth(7) 再跳过 index 1..7（rx_packets..rx_multicast），
        // 落到 index 8 = Transmit 栏的 tx_bytes。字段顺序见上方头注释。
        let tx = it
            .nth(7)
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0);
        total_rx = total_rx.saturating_add(rx);
        total_tx = total_tx.saturating_add(tx);
    }
    (total_rx, total_tx)
}
