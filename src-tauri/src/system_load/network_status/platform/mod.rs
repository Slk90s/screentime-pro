//! 网络详情的平台原生后端：默认路由链路、介质与 Wi-Fi 频段。

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkState {
    Up,
    Down,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Medium {
    Ethernet,
    Wifi,
    Vpn,
    Other,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Band {
    NotApplicable,
    Ghz24,
    Ghz5,
    Ghz6,
    Other,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TunnelKind {
    NotApplicable,
    Tun,
    Tap,
    Wireguard,
    Ppp,
    Other,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct NativeStatus {
    pub link: LinkState,
    pub medium: Medium,
    pub band: Band,
    pub vpn: bool,
    /// 能可靠关联时才给出底层物理介质；无法关联时为 null，绝不猜测。
    pub physical_medium: Option<Medium>,
    pub tunnel: TunnelKind,
    /// 默认路由出口的本机地址，不是公网 IP。
    pub local_ip: Option<String>,
}

impl Default for NativeStatus {
    fn default() -> Self {
        Self {
            link: LinkState::Unknown,
            medium: Medium::Unknown,
            band: Band::Unknown,
            vpn: false,
            physical_medium: None,
            tunnel: TunnelKind::Unknown,
            local_ip: None,
        }
    }
}

pub(crate) trait NativeBackend {
    fn snapshot() -> NativeStatus;
}

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

#[cfg(all(target_os = "linux", not(test)))]
pub(crate) use linux::Backend;
#[cfg(all(target_os = "macos", not(test)))]
pub(crate) use macos::Backend;
#[cfg(all(target_os = "windows", not(test)))]
pub(crate) use windows::Backend;

#[cfg(test)]
pub(crate) use test_support::Backend;

#[cfg(test)]
mod test_support {
    pub(crate) struct Backend;

    impl super::NativeBackend for Backend {
        fn snapshot() -> super::NativeStatus {
            super::NativeStatus::default()
        }
    }
}
