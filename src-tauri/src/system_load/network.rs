//! system_load/network.rs
//! 跨平台网络速率采样（v0.7.6 新增）。
//!
//! 设计：
//! - 复用已有「手写 FFI / 系统调用」风格（参考 macos.rs / windows.rs / linux.rs）
//! - 跨平台：macOS 用 getifaddrs + 解析 if_data，Windows 用 GetIfTable2，Linux 读 /proc/net/dev
//! - 计算口径：排除 lo（回环）+ down；有硬件接口只累计硬件，无硬件时才用虚拟接口兜底
//! - 各接口独立差分，新增/移除接口不会把历史累计值伪装成瞬时流量
//! - 每秒调用一次 sample_rate 算 bytes/sec
//! - macOS 的 32 位累计计数先转 u64；计数器回绕/重置帧按 0 处理，避免负速率
//!
//! 防 Agent 幻视：此模块于 v0.7.6 引入，取代了「用 netstat 命令行解析」的临时方案（已弃）。
//! 旧 netstat 方案因 macOS 不同版本列格式差异（`-I` vs `-b`）导致偶发解析失败被剔除。
//!
//! 修改历史：
//!   - 2026-09-10 @v0.7.6: 新增 - 以三平台累计字节差分采样网络速率，替代 netstat 解析。
//!   - 2026-10-30 @v1.2.13: 修复 - 三端硬件优先/虚拟兜底并按接口差分，避免 VPN 重复与拓扑跳变。

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

/// 一次网络速率采样结果。
#[derive(Debug, Clone, Default)]
pub struct NetworkSnapshot {
    /// 接收速率（bytes per second），非负
    pub rx_bps: f64,
    /// 发送速率（bytes per second），非负
    pub tx_bps: f64,
}

/// 保存各接口上一次累计值并计算相邻两次采样的差分速率。
pub struct NetworkSampler {
    inner: Mutex<NetworkInner>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct IfaceTotal {
    id: String,
    rx: u64,
    tx: u64,
    hardware: bool,
}

#[derive(Default)]
struct NetworkInner {
    last_totals: HashMap<String, (u64, u64)>,
    last_sample_at: Option<Instant>,
}

fn selected_ifaces(interfaces: Vec<IfaceTotal>) -> Vec<IfaceTotal> {
    let has_hardware = interfaces.iter().any(|iface| iface.hardware);
    interfaces
        .into_iter()
        .filter(|iface| iface.hardware || !has_hardware)
        .collect()
}

fn rate_from_deltas(
    current: &[IfaceTotal],
    previous: &HashMap<String, (u64, u64)>,
    elapsed: f64,
) -> NetworkSnapshot {
    if elapsed <= 0.0 {
        return NetworkSnapshot::default();
    }
    let (mut rx, mut tx) = (0u64, 0u64);
    for iface in current {
        if let Some(&(last_rx, last_tx)) = previous.get(&iface.id) {
            rx = rx.saturating_add(iface.rx.saturating_sub(last_rx));
            tx = tx.saturating_add(iface.tx.saturating_sub(last_tx));
        }
    }
    NetworkSnapshot {
        rx_bps: rx as f64 / elapsed,
        tx_bps: tx as f64 / elapsed,
    }
}

impl NetworkSampler {
    /// 创建无历史基准的采样器；首次调用只建立基准并返回零速率。
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(NetworkInner::default()),
        }
    }

    /// 计算自上次 sample_rate 以来接收/发送的字节速率（bytes/sec）
    pub fn sample_rate(&self) -> NetworkSnapshot {
        self.sample_rate_at(read_ifaces(), Instant::now())
    }

    fn sample_rate_at(
        &self,
        read: std::io::Result<Vec<IfaceTotal>>,
        now: Instant,
    ) -> NetworkSnapshot {
        let current = match read {
            Ok(interfaces) => selected_ifaces(interfaces),
            // 读取失败不等于没有接口；保留基准，下一帧仍与最后一次成功采样比较。
            Err(_) => return NetworkSnapshot::default(),
        };
        let mut inner = match self.inner.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        let snap = match inner.last_sample_at {
            Some(at) => rate_from_deltas(
                &current,
                &inner.last_totals,
                now.duration_since(at).as_secs_f64(),
            ),
            None => NetworkSnapshot::default(),
        };
        inner.last_totals = current
            .into_iter()
            .map(|iface| (iface.id, (iface.rx, iface.tx)))
            .collect();
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
fn read_ifaces() -> std::io::Result<Vec<IfaceTotal>> {
    use std::ffi::CStr;
    // SAFETY: getifaddrs 返回的链表在 freeifaddrs 前有效；只读取 AF_LINK 节点的 if_data。
    unsafe {
        let mut head: *mut libc::ifaddrs = std::ptr::null_mut();
        if libc::getifaddrs(&mut head) != 0 {
            return Err(std::io::Error::last_os_error());
        }
        if head.is_null() {
            return Ok(Vec::new());
        }
        let mut interfaces = Vec::new();
        let mut cur = head;
        while !cur.is_null() {
            let ifa = &*cur;
            if ifa.ifa_addr.is_null() {
                cur = ifa.ifa_next;
                continue;
            }
            let flags = ifa.ifa_flags as i32;
            if (flags & libc::IFF_UP) == 0 || (flags & libc::IFF_LOOPBACK) != 0 {
                cur = ifa.ifa_next;
                continue;
            }
            if (*ifa.ifa_addr).sa_family != libc::AF_LINK as u8 {
                cur = ifa.ifa_next;
                continue;
            }
            let name = CStr::from_ptr(ifa.ifa_name).to_string_lossy();
            if name == "lo0" {
                cur = ifa.ifa_next;
                continue;
            }
            if !ifa.ifa_data.is_null() {
                let ifd = ifa.ifa_data as *const IfData;
                let id = name.into_owned();
                interfaces.push(IfaceTotal {
                    hardware: macos_is_hardware(&id, (*ifd).ifi_type),
                    id,
                    rx: (*ifd).ifi_ibytes as u64,
                    tx: (*ifd).ifi_obytes as u64,
                });
            }
            cur = ifa.ifa_next;
        }
        libc::freeifaddrs(head);
        Ok(interfaces)
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

#[cfg(any(target_os = "macos", test))]
fn macos_is_hardware(name: &str, ifi_type: u8) -> bool {
    const IFT_ETHERNET: u8 = 6;
    const IFT_IEEE80211: u8 = 71;
    name.starts_with("en") && matches!(ifi_type, IFT_ETHERNET | IFT_IEEE80211)
}

#[cfg(target_os = "windows")]
use windows::Win32::NetworkManagement::IpHelper::{
    FreeMibTable, GetIfTable2, IF_TYPE_SOFTWARE_LOOPBACK, MIB_IF_ROW2, MIB_IF_TABLE2,
};
#[cfg(target_os = "windows")]
use windows::Win32::NetworkManagement::Ndis::IF_OPER_STATUS;

#[cfg(target_os = "windows")]
const IF_OPER_STATUS_UP: IF_OPER_STATUS = IF_OPER_STATUS(1);

#[cfg(target_os = "windows")]
fn row_is_eligible(row: &MIB_IF_ROW2) -> bool {
    row.OperStatus == IF_OPER_STATUS_UP && row.Type != IF_TYPE_SOFTWARE_LOOPBACK
}

#[cfg(target_os = "windows")]
fn row_is_hardware(row: &MIB_IF_ROW2) -> bool {
    // HardwareInterface 是 MIB_IF_ROW2 状态位 bit 0。
    row.InterfaceAndOperStatusFlags._bitfield & 1 != 0
}

#[cfg(target_os = "windows")]
fn rate_history_id(row: &MIB_IF_ROW2) -> String {
    // InterfaceIndex 会被系统复用；GUID 可区分先后占用同一索引的不同接口。
    format!("{:?}", row.InterfaceGuid)
}

#[cfg(target_os = "windows")]
fn read_ifaces() -> std::io::Result<Vec<IfaceTotal>> {
    // SAFETY: GetIfTable2 成功后表内存有效，NumEntries 限定读取范围，用后立即释放。
    unsafe {
        let mut table: *mut MIB_IF_TABLE2 = std::ptr::null_mut();
        let rc = GetIfTable2(&mut table);
        if rc.0 != 0 {
            return Err(std::io::Error::from_raw_os_error(rc.0 as i32));
        }
        if table.is_null() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "GetIfTable2 returned a null table",
            ));
        }
        let t = &*table;
        let mut interfaces = Vec::with_capacity(t.NumEntries as usize);
        for i in 0..t.NumEntries as usize {
            // Table 是 [MIB_IF_ROW2; 1] 柔性数组起点；用指针偏移避免越界
            let row = &*t.Table.as_ptr().add(i);
            if row_is_eligible(row) {
                interfaces.push(IfaceTotal {
                    id: rate_history_id(row),
                    rx: row.InOctets,
                    tx: row.OutOctets,
                    hardware: row_is_hardware(row),
                });
            }
        }
        FreeMibTable(table as *const _);
        Ok(interfaces)
    }
}

#[cfg(all(test, target_os = "windows"))]
mod windows_tests {
    use super::*;
    use windows::Win32::NetworkManagement::IpHelper::MIB_IF_ROW2_0;

    fn row(up: bool, loopback: bool, hardware: bool) -> MIB_IF_ROW2 {
        MIB_IF_ROW2 {
            OperStatus: if up {
                IF_OPER_STATUS_UP
            } else {
                IF_OPER_STATUS(0)
            },
            Type: if loopback {
                IF_TYPE_SOFTWARE_LOOPBACK
            } else {
                6
            },
            InterfaceAndOperStatusFlags: MIB_IF_ROW2_0 {
                _bitfield: u8::from(hardware),
            },
            ..Default::default()
        }
    }

    #[test]
    fn windows_rows_distinguish_eligibility_from_hardware() {
        let physical = row(true, false, true);
        let tunnel = row(true, false, false);
        assert!(row_is_eligible(&physical));
        assert!(row_is_hardware(&physical));
        assert!(row_is_eligible(&tunnel));
        assert!(!row_is_hardware(&tunnel));
        assert!(!row_is_eligible(&row(false, false, true)));
        assert!(!row_is_eligible(&row(true, true, true)));
    }

    #[test]
    fn rate_history_uses_guid_when_interface_index_is_reused() {
        let first = MIB_IF_ROW2 {
            InterfaceIndex: 12,
            InterfaceGuid: windows::core::GUID::from_u128(1),
            ..Default::default()
        };
        let replacement = MIB_IF_ROW2 {
            InterfaceIndex: 12,
            InterfaceGuid: windows::core::GUID::from_u128(2),
            ..Default::default()
        };
        let same_interface_new_index = MIB_IF_ROW2 {
            InterfaceIndex: 99,
            InterfaceGuid: windows::core::GUID::from_u128(1),
            ..Default::default()
        };

        assert_ne!(rate_history_id(&first), rate_history_id(&replacement));
        assert_eq!(
            rate_history_id(&first),
            rate_history_id(&same_interface_new_index)
        );
    }
}

#[cfg(any(target_os = "linux", test))]
fn linux_flags_up(raw: &str) -> Option<bool> {
    let raw = raw.trim();
    let hex = raw.strip_prefix("0x").unwrap_or(raw);
    u32::from_str_radix(hex, 16)
        .ok()
        .map(|flags| flags & 1 != 0)
}

#[cfg(target_os = "linux")]
fn linux_interface_is_up(name: &str) -> bool {
    std::fs::read_to_string(format!("/sys/class/net/{name}/flags"))
        .ok()
        .and_then(|raw| linux_flags_up(&raw))
        .unwrap_or(true)
}

#[cfg(target_os = "linux")]
fn linux_is_hardware(name: &str) -> bool {
    std::path::Path::new("/sys/class/net")
        .join(name)
        .join("device")
        .exists()
}

#[cfg(target_os = "linux")]
fn read_ifaces() -> std::io::Result<Vec<IfaceTotal>> {
    // /proc/net/dev 格式：
    //   Inter-|   Receive                                                |  Transmit
    //    face |bytes    packets errs drop fifo frame compressed multicast|bytes    ...
    //   lo: 1234 ...
    //   eth0: ...
    let s = std::fs::read_to_string("/proc/net/dev")?;
    let mut interfaces = Vec::new();
    for line in s.lines().skip(2) {
        let Some(colon) = line.find(':') else {
            continue;
        };
        let name = line[..colon].trim();
        if name == "lo" || !linux_interface_is_up(name) {
            continue;
        }
        let rest = &line[colon + 1..];
        let mut it = rest.split_whitespace();
        let rx = it.next().and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
        let tx = it.nth(7).and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
        interfaces.push(IfaceTotal {
            id: name.to_string(),
            rx,
            tx,
            hardware: linux_is_hardware(name),
        });
    }
    Ok(interfaces)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn iface(id: &str, rx: u64, tx: u64, hardware: bool) -> IfaceTotal {
        IfaceTotal {
            id: id.to_string(),
            rx,
            tx,
            hardware,
        }
    }

    #[test]
    fn failed_native_read_preserves_baseline_for_next_success() {
        let sampler = NetworkSampler::new();
        let started_at = Instant::now();
        sampler.sample_rate_at(Ok(vec![iface("wifi", 100, 50, true)]), started_at);

        let failed = sampler.sample_rate_at(
            Err(std::io::Error::other("injected native read failure")),
            started_at + std::time::Duration::from_secs(1),
        );
        assert_eq!(failed.rx_bps, 0.0);
        assert_eq!(failed.tx_bps, 0.0);
        {
            let inner = sampler.inner.lock().unwrap_or_else(|e| e.into_inner());
            assert_eq!(
                inner.last_totals,
                HashMap::from([("wifi".to_string(), (100, 50))])
            );
            assert_eq!(inner.last_sample_at, Some(started_at));
        }

        let recovered = sampler.sample_rate_at(
            Ok(vec![iface("wifi", 160, 110, true)]),
            started_at + std::time::Duration::from_secs(3),
        );
        assert_eq!(recovered.rx_bps, 20.0);
        assert_eq!(recovered.tx_bps, 20.0);
    }

    #[test]
    fn successful_empty_read_replaces_baseline_and_timestamp() {
        let sampler = NetworkSampler::new();
        let started_at = Instant::now();
        sampler.sample_rate_at(Ok(vec![iface("wifi", 100, 50, true)]), started_at);
        let emptied_at = started_at + std::time::Duration::from_secs(2);

        let empty = sampler.sample_rate_at(Ok(Vec::new()), emptied_at);
        assert_eq!(empty.rx_bps, 0.0);
        assert_eq!(empty.tx_bps, 0.0);
        {
            let inner = sampler.inner.lock().unwrap_or_else(|e| e.into_inner());
            assert!(inner.last_totals.is_empty());
            assert_eq!(inner.last_sample_at, Some(emptied_at));
        }

        let next = sampler.sample_rate_at(
            Ok(vec![iface("wifi", u64::MAX, u64::MAX, true)]),
            emptied_at + std::time::Duration::from_secs(2),
        );
        assert_eq!(next.rx_bps, 0.0);
        assert_eq!(next.tx_bps, 0.0);
    }

    #[test]
    fn hardware_interfaces_suppress_virtual_duplicates() {
        let wifi = iface("wifi", 100, 50, true);
        let ethernet = iface("ethernet", 200, 80, true);
        let selected = selected_ifaces(vec![
            wifi.clone(),
            ethernet.clone(),
            iface("tunnel", 900, 700, false),
        ]);
        assert_eq!(selected, vec![wifi, ethernet]);
    }

    #[test]
    fn virtual_interfaces_are_fallback_when_no_hardware_exists() {
        let interfaces = vec![
            iface("vpn", 100, 50, false),
            iface("virtual-nic", 200, 80, false),
        ];
        assert_eq!(selected_ifaces(interfaces.clone()), interfaces);
    }

    #[test]
    fn new_interfaces_do_not_create_fake_rate_spikes() {
        let previous = HashMap::from([("wifi".to_string(), (100, 50))]);
        let current = vec![
            iface("wifi", 160, 90, true),
            iface("new-vpn", u64::MAX, u64::MAX, false),
        ];
        let rate = rate_from_deltas(&current, &previous, 2.0);
        assert_eq!(rate.rx_bps, 30.0);
        assert_eq!(rate.tx_bps, 20.0);
    }

    #[test]
    fn counter_reset_produces_zero_instead_of_negative_rate() {
        let previous = HashMap::from([("wifi".to_string(), (u64::MAX, 100))]);
        let current = vec![iface("wifi", 3, 80, true)];
        let rate = rate_from_deltas(&current, &previous, 1.0);
        assert_eq!(rate.rx_bps, 0.0);
        assert_eq!(rate.tx_bps, 0.0);
    }

    #[test]
    fn macos_classification_rejects_tunnels_and_virtual_bridges() {
        assert!(macos_is_hardware("en0", 6));
        assert!(macos_is_hardware("en3", 71));
        assert!(!macos_is_hardware("utun0", 131));
        assert!(!macos_is_hardware("bridge0", 6));
        assert!(!macos_is_hardware("awdl0", 6));
    }

    #[test]
    fn linux_flags_require_the_interface_to_be_up() {
        assert_eq!(linux_flags_up("0x1003\n"), Some(true));
        assert_eq!(linux_flags_up("0x1002\n"), Some(false));
        assert_eq!(linux_flags_up("1"), Some(true));
        assert_eq!(linux_flags_up("invalid"), None);
    }
}
