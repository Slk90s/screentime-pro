use std::{fs, path::Path, process::Command};

use super::{Band, LinkState, Medium, NativeBackend, NativeStatus, TunnelKind};

pub(crate) struct Backend;

impl NativeBackend for Backend {
    fn snapshot() -> NativeStatus {
        let interface = match default_route_interface() {
            Ok(Some(interface)) => interface,
            Ok(None) => {
                return NativeStatus {
                    link: LinkState::Down,
                    ..NativeStatus::default()
                };
            }
            Err(()) => return NativeStatus::default(),
        };

        let sysfs = Path::new("/sys/class/net").join(&interface);
        let link = match fs::read_to_string(sysfs.join("operstate")) {
            Ok(state) => match state.trim() {
                "up" => LinkState::Up,
                "down" => LinkState::Down,
                _ => LinkState::Unknown,
            },
            Err(_) => LinkState::Unknown,
        };
        let wireless = sysfs.join("wireless").exists() || sysfs.join("phy80211").exists();
        let native_type = fs::read_to_string(sysfs.join("type"))
            .ok()
            .and_then(|value| value.trim().parse::<u32>().ok());
        let medium = classify_medium(&interface, native_type, wireless);
        let band = match medium {
            Medium::Wifi => wifi_band(&interface),
            Medium::Ethernet | Medium::Vpn => Band::NotApplicable,
            Medium::Other | Medium::Unknown => Band::Unknown,
        };
        let physical_medium = if medium == Medium::Vpn {
            active_physical_medium(Path::new("/sys/class/net"))
        } else {
            confirmed_physical_medium(link, medium, sysfs.join("device").exists())
        };

        NativeStatus {
            link,
            medium,
            band,
            vpn: medium == Medium::Vpn,
            physical_medium,
            tunnel: classify_tunnel(&interface),
            local_ip: None,
        }
    }
}

fn default_route_interface() -> Result<Option<String>, ()> {
    let ipv4 = fs::read_to_string("/proc/net/route");
    let ipv6 = fs::read_to_string("/proc/net/ipv6_route");
    let ipv4_route = ipv4
        .as_ref()
        .ok()
        .and_then(|routes| parse_default_route(routes))
        .map(|(interface, _)| interface);
    let ipv6_route = ipv6
        .as_ref()
        .ok()
        .and_then(|routes| parse_ipv6_default_route(routes));
    if let Some(interface) = ipv4_route.or(ipv6_route) {
        return Ok(Some(interface));
    }
    if ipv4.is_err() || ipv6.is_err() {
        return Err(());
    }
    Ok(None)
}

fn parse_ipv6_default_route(input: &str) -> Option<String> {
    let mut best: Option<(String, u32)> = None;
    for line in input.lines() {
        let fields: Vec<_> = line.split_whitespace().collect();
        if fields.len() < 10 || !fields[0].chars().all(|c| c == '0') {
            continue;
        }
        if fields[1] != "00" {
            continue;
        }
        let flags = match u32::from_str_radix(fields[8], 16) {
            Ok(flags) => flags,
            Err(_) => continue,
        };
        if flags & 0x1 == 0 {
            continue;
        }
        let metric = match u32::from_str_radix(fields[5], 16) {
            Ok(metric) => metric,
            Err(_) => continue,
        };
        let replace = match best.as_ref() {
            Some((_, current_metric)) => metric < *current_metric,
            None => true,
        };
        if replace {
            best = Some((fields[9].to_owned(), metric));
        }
    }
    best.map(|(interface, _)| interface)
}

fn classify_tunnel(interface: &str) -> TunnelKind {
    if ["tun", "utun"]
        .iter()
        .any(|prefix| interface.starts_with(prefix))
    {
        TunnelKind::Tun
    } else if interface.starts_with("tap") {
        TunnelKind::Tap
    } else if interface.starts_with("wg") {
        TunnelKind::Wireguard
    } else if interface.starts_with("ppp") {
        TunnelKind::Ppp
    } else if ["tailscale", "zt"]
        .iter()
        .any(|prefix| interface.starts_with(prefix))
    {
        TunnelKind::Other
    } else {
        TunnelKind::NotApplicable
    }
}

fn classify_medium(interface: &str, native_type: Option<u32>, wireless: bool) -> Medium {
    if classify_tunnel(interface) != TunnelKind::NotApplicable {
        return Medium::Vpn;
    }
    if wireless {
        return Medium::Wifi;
    }
    if [
        "wwan", "rmnet", "qmi", "bridge", "br", "bond", "virbr", "docker", "veth",
    ]
    .iter()
    .any(|prefix| interface.starts_with(prefix))
    {
        return Medium::Other;
    }
    match native_type {
        Some(1) => Medium::Ethernet,
        Some(_) => Medium::Other,
        None => Medium::Unknown,
    }
}

fn confirmed_physical_medium(link: LinkState, medium: Medium, has_device: bool) -> Option<Medium> {
    if link != LinkState::Up || !has_device {
        return None;
    }
    match medium {
        Medium::Ethernet | Medium::Wifi => Some(medium),
        Medium::Vpn | Medium::Other | Medium::Unknown => None,
    }
}

/// A tunnel does not reveal which lower interface carries it. Report a medium only
/// when every active, hardware-backed physical candidate agrees on that medium.
fn active_physical_medium(sysfs_root: &Path) -> Option<Medium> {
    let entries = fs::read_dir(sysfs_root).ok()?;
    let mut result = None;
    for entry in entries.filter_map(Result::ok) {
        let sysfs = entry.path();
        if !matches!(
            fs::read_to_string(sysfs.join("operstate")),
            Ok(state) if state.trim() == "up"
        ) {
            continue;
        }
        let interface = entry.file_name();
        let interface = interface.to_string_lossy();
        let native_type = fs::read_to_string(sysfs.join("type"))
            .ok()
            .and_then(|value| value.trim().parse::<u32>().ok());
        let wireless = sysfs.join("wireless").exists() || sysfs.join("phy80211").exists();
        let medium = classify_medium(&interface, native_type, wireless);
        let Some(candidate) =
            confirmed_physical_medium(LinkState::Up, medium, sysfs.join("device").exists())
        else {
            continue;
        };
        match result {
            None => result = Some(candidate),
            Some(current) if current == candidate => {}
            Some(_) => return None,
        }
    }
    result
}

fn parse_default_route(input: &str) -> Option<(String, u32)> {
    let mut best: Option<(String, u32)> = None;
    for line in input.lines() {
        let fields: Vec<_> = line.split_whitespace().collect();
        if fields.len() < 8 || fields[1] != "00000000" || fields[7] != "00000000" {
            continue;
        }
        let flags = match u32::from_str_radix(fields[3], 16) {
            Ok(flags) => flags,
            Err(_) => continue,
        };
        if flags & 0x1 == 0 {
            continue;
        }
        let metric = match fields[6].parse::<u32>() {
            Ok(metric) => metric,
            Err(_) => continue,
        };
        let replace = match best.as_ref() {
            Some((_, current_metric)) => metric < *current_metric,
            None => true,
        };
        if replace {
            best = Some((fields[0].to_owned(), metric));
        }
    }
    best
}

fn wifi_band(interface: &str) -> Band {
    let output = match Command::new("iw").args(["dev", interface, "info"]).output() {
        Ok(output) if output.status.success() => output,
        _ => return Band::Unknown,
    };
    let stdout = match String::from_utf8(output.stdout) {
        Ok(stdout) => stdout,
        Err(_) => return Band::Unknown,
    };
    parse_wifi_band(&stdout).unwrap_or(Band::Unknown)
}

fn parse_wifi_band(input: &str) -> Option<Band> {
    for line in input.lines() {
        let line = line.trim_start();
        if !line.starts_with("channel ") {
            continue;
        }
        let open = match line.find('(') {
            Some(open) => open,
            None => continue,
        };
        let mut fields = line[open + 1..].split_whitespace();
        let mhz = match fields.next().and_then(|value| value.parse::<u32>().ok()) {
            Some(mhz) => mhz,
            None => continue,
        };
        if !matches!(fields.next(), Some(unit) if unit.starts_with("MHz")) {
            continue;
        }
        return Some(match mhz {
            0 => Band::Unknown,
            1..=2999 => Band::Ghz24,
            3000..=5924 => Band::Ghz5,
            5925..=7125 => Band::Ghz6,
            _ => Band::Other,
        });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn route_parser_requires_up_and_selects_lowest_metric() {
        let routes = concat!(
            "Iface Destination Gateway Flags RefCnt Use Metric Mask MTU Window IRTT\n",
            "eth0 00000000 010011AC 0002 0 0 10 00000000 0 0 0\n",
            "wlan0 00000000 010011AC 0003 0 0 50 00000000 0 0 0\n",
            "eth1 000011AC 00000000 0001 0 0 10 FFFF0000 0 0 0\n",
        );
        assert_eq!(
            parse_default_route(routes),
            Some((String::from("wlan0"), 50))
        );
        assert_eq!(parse_default_route("Iface Destination\n"), None);
    }

    #[test]
    fn ipv6_default_route_is_supported() {
        let routes = concat!(
            "00000000000000000000000000000000 00 00000000000000000000000000000000 00 ",
            "00000000000000000000000000000000 00000064 00000000 00000001 00000001 eth0\n",
            "00000000000000000000000000000000 00 00000000000000000000000000000000 00 ",
            "00000000000000000000000000000000 00000032 00000000 00000001 00000001 wlan0\n",
        );
        assert_eq!(
            parse_ipv6_default_route(routes),
            Some(String::from("wlan0"))
        );
    }

    #[test]
    fn media_classification_does_not_default_everything_to_ethernet() {
        assert_eq!(classify_medium("tailscale0", Some(1), false), Medium::Vpn);
        assert_eq!(classify_medium("zt1234", Some(1), false), Medium::Vpn);
        assert_eq!(classify_medium("wwan0", Some(1), false), Medium::Other);
        assert_eq!(classify_medium("br0", Some(1), false), Medium::Other);
        assert_eq!(classify_medium("eth0", Some(1), false), Medium::Ethernet);
        assert_eq!(classify_medium("wlan0", Some(1), true), Medium::Wifi);
        assert_eq!(classify_medium("mystery0", None, false), Medium::Unknown);
    }

    #[test]
    fn tunnel_classification_covers_linux_vpn_conventions() {
        let cases = [
            ("tun0", TunnelKind::Tun),
            ("utun1", TunnelKind::Tun),
            ("tap0", TunnelKind::Tap),
            ("wg0", TunnelKind::Wireguard),
            ("ppp0", TunnelKind::Ppp),
            ("tailscale0", TunnelKind::Other),
            ("ztabc123", TunnelKind::Other),
            ("eth0", TunnelKind::NotApplicable),
            ("wlan0", TunnelKind::NotApplicable),
            ("br0", TunnelKind::NotApplicable),
        ];
        for (interface, expected) in cases {
            assert_eq!(
                classify_tunnel(interface),
                expected,
                "interface {interface}"
            );
        }
    }

    #[test]
    fn physical_medium_requires_active_identifiable_hardware() {
        assert_eq!(
            confirmed_physical_medium(LinkState::Up, Medium::Ethernet, true),
            Some(Medium::Ethernet)
        );
        assert_eq!(
            confirmed_physical_medium(LinkState::Up, Medium::Wifi, true),
            Some(Medium::Wifi)
        );
        for link in [LinkState::Down, LinkState::Unknown] {
            assert_eq!(
                confirmed_physical_medium(link, Medium::Ethernet, true),
                None
            );
        }
        assert_eq!(
            confirmed_physical_medium(LinkState::Up, Medium::Ethernet, false),
            None
        );
        for medium in [Medium::Vpn, Medium::Other, Medium::Unknown] {
            assert_eq!(confirmed_physical_medium(LinkState::Up, medium, true), None);
        }
    }

    #[test]
    fn frequency_parser_enforces_real_band_boundaries() {
        assert_eq!(
            parse_wifi_band("Interface wlan0\n\tchannel 1 (2412 MHz), width: 20 MHz"),
            Some(Band::Ghz24)
        );
        assert_eq!(parse_wifi_band("channel 36 (3000 MHz)"), Some(Band::Ghz5));
        assert_eq!(parse_wifi_band("channel 2 (5925 MHz)"), Some(Band::Ghz6));
        assert_eq!(parse_wifi_band("channel 4 (7126 MHz)"), Some(Band::Other));
        assert_eq!(parse_wifi_band("channel 1 (60000 MHz)"), Some(Band::Other));
        assert_eq!(parse_wifi_band("channel 0 (0 MHz)"), Some(Band::Unknown));
        assert_eq!(
            parse_wifi_band(&format!("channel 1 ({} MHz)", u32::MAX)),
            Some(Band::Other)
        );
    }
}
