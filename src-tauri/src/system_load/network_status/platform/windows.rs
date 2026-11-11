//! Windows 原生链路状态：默认路由、接口介质、链路状态与当前 Wi-Fi BSS 频段。
//!
//! Cargo.toml 目前只启用了 IpHelper/Ndis，未启用 WinSock/WiFi。这里复用已启用的
//! `GetIfEntry2`/`GetIfTable2`/`FreeMibTable`，其余少量 API 使用与系统头文件一致的窄 FFI 声明，
//! 因而无需新增 Cargo feature。WLAN 查询只读当前连接和驱动缓存的 BSS 列表，绝不扫描，
//! 也不通过 SSID 猜测频段。

use super::{Band, LinkState, Medium, NativeBackend, NativeStatus, TunnelKind};
use std::ffi::c_void;
use windows::core::GUID;
use windows::Win32::NetworkManagement::IpHelper::{
    FreeMibTable, GetIfEntry2, GetIfTable2, MIB_IF_ROW2,
};
use windows::Win32::NetworkManagement::Ndis::NET_LUID_LH;

const ERROR_SUCCESS: u32 = 0;
const AF_UNSPEC: u16 = 0;
const AF_INET: u16 = 2;
const AF_INET6: u16 = 23;

const IF_TYPE_ETHERNET_CSMACD: u32 = 6;
const IF_TYPE_PPP: u32 = 23;
const IF_TYPE_FASTETHER: u32 = 62;
const IF_TYPE_FASTETHER_FX: u32 = 69;
const IF_TYPE_IEEE80211: u32 = 71;
const IF_TYPE_GIGABITETHERNET: u32 = 117;
const IF_TYPE_TUNNEL: u32 = 131;
const IF_ROW_FLAG_HARDWARE_INTERFACE: u8 = 0x01;

const WLAN_API_VERSION_2_0: u32 = 2;
const WLAN_INTF_OPCODE_CURRENT_CONNECTION: i32 = 7;
const WLAN_INTERFACE_STATE_CONNECTED: i32 = 1;
const DOT11_BSS_TYPE_ANY: i32 = 3;

pub(crate) struct Backend;

impl NativeBackend for Backend {
    fn snapshot() -> NativeStatus {
        let Some(route) = active_default_route() else {
            return NativeStatus::default();
        };
        let Some(row) = interface_row(route.interface_index) else {
            return NativeStatus::default();
        };

        let link = link_state(row.OperStatus.0);
        let medium = classify_medium(row.Type, &row.Alias, &row.Description);
        let band = match medium {
            Medium::Wifi if link == LinkState::Up => current_wifi_band(row.InterfaceGuid),
            Medium::Wifi => Band::Unknown,
            Medium::Unknown => Band::Unknown,
            _ => Band::NotApplicable,
        };

        let vpn = medium == Medium::Vpn;
        NativeStatus {
            link,
            medium,
            band,
            vpn,
            physical_medium: vpn.then(active_physical_medium).flatten(),
            tunnel: if vpn {
                classify_tunnel(row.Type, &row.Alias, &row.Description)
            } else {
                TunnelKind::NotApplicable
            },
            local_ip: None,
        }
    }
}

/// `GetBestRoute2` 让 Windows 执行完整的路由决策；表查询只作为无路由时的降级路径。
/// 固定的公网地址仅作为路由查询输入，不会发送数据包。
fn active_default_route() -> Option<RouteCandidate> {
    best_route_to(&SockaddrInet::ipv4([1, 1, 1, 1]))
        .or_else(|| best_route_to(&SockaddrInet::ipv6(IPV6_ROUTE_DESTINATION)))
        .or_else(default_route_from_table)
}

const IPV6_ROUTE_DESTINATION: [u8; 16] = [
    0x26, 0x06, 0x47, 0x00, 0x47, 0x00, 0, 0, 0, 0, 0, 0, 0, 0, 0x11, 0x11,
];

fn best_route_to(destination: &SockaddrInet) -> Option<RouteCandidate> {
    let mut route = RouteRow::default();
    let mut source = SockaddrInet::default();
    // SAFETY: 所有输出指针都指向有效且布局匹配的对象；空的可选输入指针由 API 明确允许。
    let result = unsafe {
        GetBestRoute2(
            std::ptr::null(),
            0,
            std::ptr::null(),
            destination,
            0,
            &mut route,
            &mut source,
        )
    };
    if result != ERROR_SUCCESS {
        return None;
    }
    route.candidate()
}

fn default_route_from_table() -> Option<RouteCandidate> {
    let mut raw_table = std::ptr::null_mut();
    // SAFETY: `raw_table` 是有效的输出指针；成功时返回的内存由下面的 guard 释放。
    let result = unsafe { GetIpForwardTable2(AF_UNSPEC, &mut raw_table) };
    let table = MibTable::new(raw_table);
    if result != ERROR_SUCCESS {
        return None;
    }
    let table = table?;

    // SAFETY: 成功返回的表在 guard 生命周期内有效；Table 是可变长数组的首元素。
    let (count, entries) = unsafe {
        (
            (*table.0).num_entries as usize,
            std::ptr::addr_of!((*table.0).table).cast::<RouteRow>(),
        )
    };
    let routes = (0..count).filter_map(|index| {
        // SAFETY: index 严格小于系统报告的 NumEntries，返回后立即复制所需字段。
        unsafe { entries.add(index).as_ref() }.and_then(RouteRow::candidate)
    });
    select_default_route(routes)
}

fn select_default_route(
    routes: impl IntoIterator<Item = RouteCandidate>,
) -> Option<RouteCandidate> {
    routes
        .into_iter()
        .filter(|route| {
            route.interface_index != 0
                && route.prefix_length == 0
                && !route.loopback
                && matches!(route.family, AF_INET | AF_INET6)
        })
        .min_by_key(|route| {
            // GetBestRoute2 失败后的确定性降级：路由优先级、IPv4 优先、接口索引。
            (
                route.metric,
                if route.family == AF_INET { 0 } else { 1 },
                route.interface_index,
            )
        })
}

fn interface_row(interface_index: u32) -> Option<MIB_IF_ROW2> {
    let mut row = MIB_IF_ROW2 {
        InterfaceIndex: interface_index,
        ..Default::default()
    };
    // SAFETY: row 提供正确的输入索引和输出缓冲区，GetIfEntry2 会原地填充它。
    let result = unsafe { GetIfEntry2(&mut row) };
    if result.0 == ERROR_SUCCESS {
        Some(row)
    } else {
        None
    }
}

fn link_state(oper_status: i32) -> LinkState {
    match oper_status {
        1 => LinkState::Up,
        // Down / Testing / Dormant / NotPresent / LowerLayerDown
        2 | 3 | 5 | 6 | 7 => LinkState::Down,
        // IfOperStatusUnknown 以及未来新增值均不猜测。
        _ => LinkState::Unknown,
    }
}

fn classify_medium(if_type: u32, alias: &[u16], description: &[u16]) -> Medium {
    if if_type == IF_TYPE_IEEE80211 {
        return Medium::Wifi;
    }
    if classify_tunnel(if_type, alias, description) != TunnelKind::NotApplicable {
        return Medium::Vpn;
    }
    if is_ethernet_type(if_type) {
        Medium::Ethernet
    } else {
        Medium::Other
    }
}

fn classify_tunnel(if_type: u32, alias: &[u16], description: &[u16]) -> TunnelKind {
    if if_type == IF_TYPE_PPP {
        return TunnelKind::Ppp;
    }

    let alias = wide_string(alias);
    let description = wide_string(description);
    let names = [alias.as_str(), description.as_str()];
    if names.iter().any(|name| {
        name.contains("wireguard") || name.contains("wire guard") || name.contains("tailscale")
    }) {
        return TunnelKind::Wireguard;
    }

    let is_tap = names
        .iter()
        .any(|name| contains_interface_word(name, "tap"));
    let is_tun = names
        .iter()
        .any(|name| name.contains("wintun") || contains_interface_word(name, "tun"));
    match (is_tap, is_tun) {
        (true, false) => return TunnelKind::Tap,
        (false, true) => return TunnelKind::Tun,
        // TUN/TAP 只说明驱动支持两种模式，无法据此猜测当前模式。
        (true, true) => return TunnelKind::Other,
        (false, false) => {}
    }

    if if_type == IF_TYPE_TUNNEL || names.iter().any(|name| contains_vpn_name(name)) {
        TunnelKind::Other
    } else {
        TunnelKind::NotApplicable
    }
}

fn wide_string(value: &[u16]) -> String {
    let length = value
        .iter()
        .position(|character| *character == 0)
        .unwrap_or(value.len());
    String::from_utf16_lossy(&value[..length]).to_lowercase()
}

/// 按完整接口词匹配，避免把 generic “Tunnel” 中的 “tun” 误判为 TUN。
fn contains_interface_word(value: &str, expected: &str) -> bool {
    value
        .split(|character: char| !character.is_ascii_alphanumeric())
        .any(|word| {
            word == expected
                || word.strip_prefix(expected).is_some_and(|suffix| {
                    !suffix.is_empty() && suffix.bytes().all(|b| b.is_ascii_digit())
                })
        })
}

fn contains_vpn_name(value: &str) -> bool {
    const VPN_MARKERS: &[&str] = &[
        "vpn",
        "wireguard",
        "openvpn",
        "tap-windows",
        "tap windows",
        "tap adapter",
        "tun/tap",
        "wintun",
        "zerotier",
        "tailscale",
        "anyconnect",
        "globalprotect",
        "fortissl",
        "tunnel",
    ];
    VPN_MARKERS.iter().any(|marker| value.contains(marker))
}

fn is_ethernet_type(if_type: u32) -> bool {
    matches!(
        if_type,
        IF_TYPE_ETHERNET_CSMACD
            | IF_TYPE_FASTETHER
            | IF_TYPE_FASTETHER_FX
            | IF_TYPE_GIGABITETHERNET
    )
}

fn active_physical_medium() -> Option<Medium> {
    let mut raw_table = std::ptr::null_mut();
    // SAFETY: `raw_table` 是有效的输出指针；成功时返回的内存由下面的 guard 释放。
    let result = unsafe { GetIfTable2(&mut raw_table) };
    let table = MibTable::new(raw_table);
    if result.0 != ERROR_SUCCESS {
        return None;
    }
    let table = table?;

    // SAFETY: 成功返回的表在 guard 生命周期内有效；Table 是可变长数组的首元素。
    let (count, entries) = unsafe {
        (
            (*table.0).NumEntries as usize,
            std::ptr::addr_of!((*table.0).Table).cast::<MIB_IF_ROW2>(),
        )
    };
    // SAFETY: entries 指向系统返回的连续 NumEntries 行，slice 不越过 guard 生命周期。
    let rows = unsafe { std::slice::from_raw_parts(entries, count) };
    physical_medium_from_rows(rows)
}

fn physical_medium_from_rows(rows: &[MIB_IF_ROW2]) -> Option<Medium> {
    let mut detected = None;
    for row in rows {
        if row.OperStatus.0 != 1
            || row.InterfaceAndOperStatusFlags._bitfield & IF_ROW_FLAG_HARDWARE_INTERFACE == 0
            || classify_tunnel(row.Type, &row.Alias, &row.Description) != TunnelKind::NotApplicable
        {
            continue;
        }
        let medium = if row.Type == IF_TYPE_IEEE80211 {
            Medium::Wifi
        } else if is_ethernet_type(row.Type) {
            Medium::Ethernet
        } else {
            continue;
        };

        match detected {
            None => detected = Some(medium),
            Some(current) if current == medium => {}
            // Wi-Fi 与以太网同时活动时无法仅凭接口表确定 VPN 的底层链路。
            Some(_) => return None,
        }
    }
    detected
}

fn current_wifi_band(interface_guid: GUID) -> Band {
    current_bss_frequency_khz(interface_guid).map_or(Band::Unknown, band_from_frequency_khz)
}

/// WLAN_BSS_ENTRY::ulChCenterFrequency 的单位是 kHz；5925 MHz 是 6 GHz 的起点。
fn band_from_frequency_khz(frequency_khz: u32) -> Band {
    match frequency_khz {
        0 => Band::Unknown,
        1..=2_999_999 => Band::Ghz24,
        3_000_000..=5_924_999 => Band::Ghz5,
        5_925_000..=7_125_000 => Band::Ghz6,
        _ => Band::Other,
    }
}

fn current_bss_frequency_khz(interface_guid: GUID) -> Option<u32> {
    let client = WlanClient::open()?;
    let bssid = current_bssid(client.0, &interface_guid)?;
    if bssid == [0; 6] {
        return None;
    }

    let mut raw_list = std::ptr::null_mut();
    // SAFETY: 使用有效的客户端句柄和接口 GUID；SSID 为空表示读取驱动缓存的全部 BSS，
    // `security_enabled=0` 在 SSID 为空时被忽略。本调用不是 WlanScan，不请求主动扫描。
    let result = unsafe {
        WlanGetNetworkBssList(
            client.0,
            &interface_guid,
            std::ptr::null(),
            DOT11_BSS_TYPE_ANY,
            0,
            std::ptr::null(),
            &mut raw_list,
        )
    };
    let list = WlanMemory::new(raw_list);
    if result != ERROR_SUCCESS {
        return None;
    }
    let list = list?;

    // SAFETY: 成功返回的列表由 guard 保活；先根据 dwTotalSize 限制可读 entry 数。
    let list_ref = unsafe { &*list.0 };
    let entries_offset = std::mem::offset_of!(WlanBssList, entries);
    let available_entries = (list_ref.total_size as usize).saturating_sub(entries_offset)
        / std::mem::size_of::<WlanBssEntry>();
    let count = (list_ref.number_of_items as usize).min(available_entries);
    let entries = std::ptr::addr_of!(list_ref.entries).cast::<WlanBssEntry>();
    for index in 0..count {
        // SAFETY: index 同时小于 dwNumberOfItems 和 dwTotalSize 能容纳的 entry 数。
        let entry = unsafe { &*entries.add(index) };
        // 只接受当前连接的 BSSID；同 SSID 的其他 AP 可能位于另一频段。
        if entry.bssid == bssid {
            return Some(entry.center_frequency_khz);
        }
    }
    None
}

fn current_bssid(client: *mut c_void, interface_guid: &GUID) -> Option<[u8; 6]> {
    let mut data_size = 0_u32;
    let mut raw_data = std::ptr::null_mut();
    // SAFETY: 输出指针有效，reserved/opcode-type 的可选空指针由 WLAN API 允许。
    let result = unsafe {
        WlanQueryInterface(
            client,
            interface_guid,
            WLAN_INTF_OPCODE_CURRENT_CONNECTION,
            std::ptr::null(),
            &mut data_size,
            &mut raw_data,
            std::ptr::null_mut(),
        )
    };
    let data = WlanMemory::new(raw_data);
    if result != ERROR_SUCCESS {
        return None;
    }
    let data = data?;
    if (data_size as usize) < std::mem::size_of::<WlanConnectionAttributes>() {
        return None;
    }

    // SAFETY: 已检查返回缓冲区大小，且 guard 保证读取期间内存有效。
    let attributes = unsafe { &*data.cast::<WlanConnectionAttributes>() };
    if attributes.state != WLAN_INTERFACE_STATE_CONNECTED {
        return None;
    }
    Some(attributes.association.bssid)
}

struct MibTable<T>(*mut T);

impl<T> MibTable<T> {
    fn new(pointer: *mut T) -> Option<Self> {
        (!pointer.is_null()).then_some(Self(pointer))
    }
}

impl<T> Drop for MibTable<T> {
    fn drop(&mut self) {
        // SAFETY: 此指针只由成功的 MIB 表查询返回，并且 guard 只释放一次。
        unsafe { FreeMibTable(self.0.cast()) }
    }
}

struct WlanClient(*mut c_void);

impl WlanClient {
    fn open() -> Option<Self> {
        let mut handle = std::ptr::null_mut();
        let mut negotiated_version = 0_u32;
        // SAFETY: 两个输出指针有效，reserved 必须为 null。
        let result = unsafe {
            WlanOpenHandle(
                WLAN_API_VERSION_2_0,
                std::ptr::null(),
                &mut negotiated_version,
                &mut handle,
            )
        };
        if result == ERROR_SUCCESS && !handle.is_null() {
            Some(Self(handle))
        } else {
            None
        }
    }
}

impl Drop for WlanClient {
    fn drop(&mut self) {
        // SAFETY: 此句柄来自成功的 WlanOpenHandle，且仅在此关闭一次。
        unsafe {
            WlanCloseHandle(self.0, std::ptr::null());
        }
    }
}

struct WlanMemory<T>(*mut T);

impl<T> WlanMemory<T> {
    fn new(pointer: *mut T) -> Option<Self> {
        (!pointer.is_null()).then_some(Self(pointer))
    }

    fn cast<U>(&self) -> *const U {
        self.0.cast::<U>()
    }
}

impl<T> Drop for WlanMemory<T> {
    fn drop(&mut self) {
        // SAFETY: 此指针只由 WLAN API 分配，并且 guard 只释放一次。
        unsafe { WlanFreeMemory(self.0.cast()) }
    }
}

#[repr(C, align(4))]
#[derive(Clone, Copy)]
struct SockaddrInet {
    family: u16,
    port: u16,
    // IPv4: address + sin_zero；IPv6: flowinfo + address + scope。
    payload: [u8; 24],
}

impl SockaddrInet {
    fn ipv4(address: [u8; 4]) -> Self {
        let mut result = Self {
            family: AF_INET,
            port: 0,
            payload: [0; 24],
        };
        result.payload[..4].copy_from_slice(&address);
        result
    }

    fn ipv6(address: [u8; 16]) -> Self {
        let mut result = Self {
            family: AF_INET6,
            port: 0,
            payload: [0; 24],
        };
        result.payload[4..20].copy_from_slice(&address);
        result
    }
}

impl Default for SockaddrInet {
    fn default() -> Self {
        Self {
            family: AF_UNSPEC,
            port: 0,
            payload: [0; 24],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct IpAddressPrefix {
    prefix: SockaddrInet,
    prefix_length: u8,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct RouteRow {
    interface_luid: NET_LUID_LH,
    interface_index: u32,
    destination_prefix: IpAddressPrefix,
    next_hop: SockaddrInet,
    site_prefix_length: u8,
    valid_lifetime: u32,
    preferred_lifetime: u32,
    metric: u32,
    protocol: i32,
    loopback: u8,
    autoconfigure_address: u8,
    publish: u8,
    immortal: u8,
    age: u32,
    origin: i32,
}

impl RouteRow {
    fn candidate(&self) -> Option<RouteCandidate> {
        (self.interface_index != 0).then_some(RouteCandidate {
            interface_index: self.interface_index,
            family: self.destination_prefix.prefix.family,
            prefix_length: self.destination_prefix.prefix_length,
            metric: self.metric,
            loopback: self.loopback != 0,
        })
    }
}

#[repr(C)]
struct MibIpForwardTable2 {
    num_entries: u32,
    table: [RouteRow; 1],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RouteCandidate {
    interface_index: u32,
    family: u16,
    prefix_length: u8,
    metric: u32,
    loopback: bool,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Dot11Ssid {
    length: u32,
    bytes: [u8; 32],
}

#[repr(C)]
struct WlanAssociationAttributes {
    ssid: Dot11Ssid,
    bss_type: i32,
    bssid: [u8; 6],
    phy_type: i32,
    phy_index: u32,
    signal_quality: u32,
    rx_rate: u32,
    tx_rate: u32,
}

#[repr(C)]
struct WlanSecurityAttributes {
    security_enabled: i32,
    one_x_enabled: i32,
    auth_algorithm: i32,
    cipher_algorithm: i32,
}

#[repr(C)]
struct WlanConnectionAttributes {
    state: i32,
    connection_mode: i32,
    profile_name: [u16; 256],
    association: WlanAssociationAttributes,
    security: WlanSecurityAttributes,
}

#[repr(C)]
struct WlanRateSet {
    length: u32,
    rates: [u16; 126],
}

#[repr(C)]
struct WlanBssEntry {
    ssid: Dot11Ssid,
    phy_id: u32,
    bssid: [u8; 6],
    bss_type: i32,
    bss_phy_type: i32,
    rssi: i32,
    link_quality: u32,
    in_reg_domain: u8,
    beacon_period: u16,
    timestamp: u64,
    host_timestamp: u64,
    capability_information: u16,
    center_frequency_khz: u32,
    rate_set: WlanRateSet,
    ie_offset: u32,
    ie_size: u32,
}

#[repr(C)]
struct WlanBssList {
    total_size: u32,
    number_of_items: u32,
    entries: [WlanBssEntry; 1],
}

#[link(name = "iphlpapi")]
extern "system" {
    fn GetBestRoute2(
        interface_luid: *const NET_LUID_LH,
        interface_index: u32,
        source_address: *const SockaddrInet,
        destination_address: *const SockaddrInet,
        address_sort_options: u32,
        best_route: *mut RouteRow,
        best_source_address: *mut SockaddrInet,
    ) -> u32;

    fn GetIpForwardTable2(family: u16, table: *mut *mut MibIpForwardTable2) -> u32;
}

#[link(name = "wlanapi")]
extern "system" {
    fn WlanOpenHandle(
        client_version: u32,
        reserved: *const c_void,
        negotiated_version: *mut u32,
        client_handle: *mut *mut c_void,
    ) -> u32;
    fn WlanCloseHandle(client_handle: *mut c_void, reserved: *const c_void) -> u32;
    fn WlanQueryInterface(
        client_handle: *mut c_void,
        interface_guid: *const GUID,
        opcode: i32,
        reserved: *const c_void,
        data_size: *mut u32,
        data: *mut *mut c_void,
        opcode_value_type: *mut i32,
    ) -> u32;
    fn WlanGetNetworkBssList(
        client_handle: *mut c_void,
        interface_guid: *const GUID,
        ssid: *const Dot11Ssid,
        bss_type: i32,
        security_enabled: i32,
        reserved: *const c_void,
        bss_list: *mut *mut WlanBssList,
    ) -> u32;
    fn WlanFreeMemory(memory: *const c_void);
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::NetworkManagement::IpHelper::MIB_IF_ROW2_0;
    use windows::Win32::NetworkManagement::Ndis::IF_OPER_STATUS;

    fn route(
        interface_index: u32,
        family: u16,
        prefix_length: u8,
        metric: u32,
        loopback: bool,
    ) -> RouteCandidate {
        RouteCandidate {
            interface_index,
            family,
            prefix_length,
            metric,
            loopback,
        }
    }

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(Some(0)).collect()
    }

    fn test_interface_row(if_type: u32, oper_status: i32, hardware: bool) -> MIB_IF_ROW2 {
        MIB_IF_ROW2 {
            Type: if_type,
            OperStatus: IF_OPER_STATUS(oper_status),
            InterfaceAndOperStatusFlags: MIB_IF_ROW2_0 {
                _bitfield: if hardware {
                    IF_ROW_FLAG_HARDWARE_INTERFACE
                } else {
                    0
                },
            },
            ..Default::default()
        }
    }

    #[test]
    fn frequency_mapping_uses_khz_and_exact_band_boundaries() {
        assert_eq!(band_from_frequency_khz(0), Band::Unknown);
        assert_eq!(band_from_frequency_khz(2_412_000), Band::Ghz24);
        assert_eq!(band_from_frequency_khz(2_999_999), Band::Ghz24);
        assert_eq!(band_from_frequency_khz(3_000_000), Band::Ghz5);
        assert_eq!(band_from_frequency_khz(5_180_000), Band::Ghz5);
        assert_eq!(band_from_frequency_khz(5_924_999), Band::Ghz5);
        assert_eq!(band_from_frequency_khz(5_925_000), Band::Ghz6);
        assert_eq!(band_from_frequency_khz(7_115_000), Band::Ghz6);
        assert_eq!(band_from_frequency_khz(7_125_000), Band::Ghz6);
        assert_eq!(band_from_frequency_khz(7_125_001), Band::Other);
        assert_eq!(band_from_frequency_khz(60_000_000), Band::Other);
        assert_eq!(band_from_frequency_khz(u32::MAX), Band::Other);
    }

    #[test]
    fn route_selection_ignores_non_default_and_invalid_routes() {
        let selected = select_default_route([
            route(1, AF_INET, 24, 1, false),
            route(2, AF_INET, 0, 1, true),
            route(3, AF_UNSPEC, 0, 1, false),
            route(0, AF_INET, 0, 1, false),
            route(8, AF_INET6, 0, 30, false),
            route(9, AF_INET, 0, 20, false),
        ]);
        assert_eq!(selected.map(|route| route.interface_index), Some(9));
    }

    #[test]
    fn route_selection_has_deterministic_family_and_index_tie_breaks() {
        let selected = select_default_route([
            route(20, AF_INET6, 0, 10, false),
            route(20, AF_INET, 0, 10, false),
            route(10, AF_INET, 0, 10, false),
        ]);
        assert_eq!(selected.map(|route| route.interface_index), Some(10));
        assert_eq!(selected.map(|route| route.family), Some(AF_INET));
    }

    #[test]
    fn link_and_media_classification_do_not_guess_unknown_states() {
        assert_eq!(link_state(1), LinkState::Up);
        for status in [2, 3, 5, 6, 7] {
            assert_eq!(link_state(status), LinkState::Down);
        }
        for status in [0, 4, 99] {
            assert_eq!(link_state(status), LinkState::Unknown);
        }

        let empty = [0_u16; 1];
        let vpn: Vec<u16> = "WireGuard Tunnel\0".encode_utf16().collect();
        assert_eq!(
            classify_medium(IF_TYPE_IEEE80211, &empty, &empty),
            Medium::Wifi
        );
        assert_eq!(
            classify_medium(IF_TYPE_ETHERNET_CSMACD, &vpn, &empty),
            Medium::Vpn
        );
        let meta: Vec<u16> = "Meta Tunnel\0".encode_utf16().collect();
        assert_eq!(
            classify_medium(IF_TYPE_ETHERNET_CSMACD, &meta, &empty),
            Medium::Vpn
        );
        assert_eq!(
            classify_medium(IF_TYPE_ETHERNET_CSMACD, &empty, &empty),
            Medium::Ethernet
        );
        assert_eq!(classify_medium(1, &empty, &empty), Medium::Other);
    }

    #[test]
    fn tunnel_classification_distinguishes_generic_tun_tap_and_wireguard() {
        let empty = [0_u16; 1];
        for (alias, description, expected) in [
            ("Meta Tunnel", "", TunnelKind::Other),
            ("", "TUN", TunnelKind::Tun),
            ("TAP-Windows Adapter V9", "", TunnelKind::Tap),
            ("", "WireGuard Tunnel", TunnelKind::Wireguard),
        ] {
            let alias = wide(alias);
            let description = wide(description);
            assert_eq!(
                classify_medium(IF_TYPE_ETHERNET_CSMACD, &alias, &description),
                Medium::Vpn
            );
            assert_eq!(
                classify_tunnel(IF_TYPE_ETHERNET_CSMACD, &alias, &description),
                expected
            );
        }

        assert_eq!(
            classify_tunnel(IF_TYPE_PPP, &empty, &empty),
            TunnelKind::Ppp
        );
        assert_eq!(
            classify_tunnel(IF_TYPE_ETHERNET_CSMACD, &empty, &empty),
            TunnelKind::NotApplicable
        );
        let tun_tap = wide("TUN/TAP");
        assert_eq!(
            classify_tunnel(IF_TYPE_ETHERNET_CSMACD, &tun_tap, &empty),
            TunnelKind::Other
        );
    }

    #[test]
    fn physical_medium_uses_only_unambiguous_active_hardware_rows() {
        let tunnel = test_interface_row(IF_TYPE_TUNNEL, 1, false);
        let wifi = test_interface_row(IF_TYPE_IEEE80211, 1, true);
        let ethernet = test_interface_row(IF_TYPE_ETHERNET_CSMACD, 1, true);
        let inactive_ethernet = test_interface_row(IF_TYPE_ETHERNET_CSMACD, 2, true);
        let virtual_wifi = test_interface_row(IF_TYPE_IEEE80211, 1, false);
        let mut vpn_with_hardware_flag = test_interface_row(IF_TYPE_ETHERNET_CSMACD, 1, true);
        let vpn_alias = wide("Meta Tunnel");
        vpn_with_hardware_flag.Alias[..vpn_alias.len()].copy_from_slice(&vpn_alias);

        assert_eq!(
            physical_medium_from_rows(&[tunnel, inactive_ethernet, vpn_with_hardware_flag, wifi]),
            Some(Medium::Wifi)
        );
        assert_eq!(
            physical_medium_from_rows(&[ethernet]),
            Some(Medium::Ethernet)
        );
        assert_eq!(physical_medium_from_rows(&[virtual_wifi]), None);
        assert_eq!(physical_medium_from_rows(&[wifi, ethernet]), None);
    }

    #[test]
    fn ffi_layouts_match_netioapi_and_wlanapi() {
        assert_eq!(std::mem::size_of::<SockaddrInet>(), 28);
        assert_eq!(std::mem::align_of::<SockaddrInet>(), 4);
        assert_eq!(std::mem::size_of::<IpAddressPrefix>(), 32);
        assert_eq!(std::mem::offset_of!(RouteRow, destination_prefix), 12);
        assert_eq!(std::mem::offset_of!(RouteRow, next_hop), 44);
        assert_eq!(std::mem::offset_of!(RouteRow, metric), 84);
        assert_eq!(std::mem::size_of::<RouteRow>(), 104);
        assert_eq!(std::mem::offset_of!(MibIpForwardTable2, table), 8);

        assert_eq!(std::mem::size_of::<WlanAssociationAttributes>(), 68);
        assert_eq!(
            std::mem::offset_of!(WlanConnectionAttributes, association),
            520
        );
        assert_eq!(std::mem::size_of::<WlanConnectionAttributes>(), 604);
        assert_eq!(std::mem::offset_of!(WlanBssEntry, center_frequency_khz), 92);
        assert_eq!(std::mem::size_of::<WlanBssEntry>(), 360);
        assert_eq!(std::mem::offset_of!(WlanBssList, entries), 8);
    }

    /// 人工真机探针：cargo test --lib native_status_probe -- --ignored --nocapture
    #[test]
    #[ignore]
    fn native_status_probe() {
        println!("{:#?}", Backend::snapshot());
    }
}
