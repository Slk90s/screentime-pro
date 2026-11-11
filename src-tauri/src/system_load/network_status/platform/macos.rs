//! macOS native network details.
//!
//! SystemConfiguration supplies the current primary (default-route) interface and
//! its link state. CoreWLAN is used only for the current channel band of that
//! interface; this module never requests an SSID/BSSID and never starts a scan.

use super::{Band, LinkState, Medium, NativeBackend, NativeStatus, TunnelKind};
use core_foundation_sys::array::{CFArrayGetCount, CFArrayGetValueAtIndex, CFArrayRef};
use core_foundation_sys::base::{CFGetTypeID, CFIndex, CFRelease, CFTypeID, CFTypeRef};
use core_foundation_sys::dictionary::{
    CFDictionaryGetTypeID, CFDictionaryGetValue, CFDictionaryRef,
};
use core_foundation_sys::number::{CFBooleanGetTypeID, CFBooleanGetValue};
use core_foundation_sys::propertylist::CFPropertyListRef;
use core_foundation_sys::string::{
    kCFStringEncodingUTF8, CFStringCreateWithCString, CFStringGetCString, CFStringGetTypeID,
    CFStringRef,
};
use objc2::rc::{autoreleasepool, Retained};
use objc2::runtime::{AnyClass, AnyObject};
use objc2::{msg_send, sel};
use objc2_foundation::NSString;
use std::ffi::{c_void, CString};
use std::ptr;
use std::sync::OnceLock;

/// macOS backend backed by SystemConfiguration and CoreWLAN.
pub(crate) struct Backend;

impl NativeBackend for Backend {
    fn snapshot() -> NativeStatus {
        let Some(store) = dynamic_store() else {
            return NativeStatus::default();
        };
        let primary = primary_interfaces(&store);
        let interfaces = network_interfaces().unwrap_or_default();
        let Some(interface) = select_primary_interface(&primary, &interfaces) else {
            return NativeStatus::default();
        };

        let native_type = native_type_for(&interfaces, interface);
        let medium = medium_from_interface(interface, native_type);
        let tunnel = tunnel_from_interface(interface, native_type);
        let link = link_state(&store, interface).unwrap_or(LinkState::Unknown);
        let physical_medium = if medium == Medium::Vpn {
            active_physical_medium(&store, &primary, interface, &interfaces)
        } else if link == LinkState::Up {
            physical_medium_from_type(native_type)
        } else {
            None
        };
        let band = match medium {
            Medium::Wifi => wifi_band(interface),
            Medium::Ethernet | Medium::Vpn | Medium::Other => Band::NotApplicable,
            Medium::Unknown => Band::Unknown,
        };

        NativeStatus {
            link,
            medium,
            band,
            vpn: medium == Medium::Vpn,
            physical_medium,
            tunnel,
            // The shared sampler resolves the route-local address independently.
            local_ip: None,
        }
    }
}

type SCDynamicStoreRef = *const c_void;
type SCNetworkInterfaceRef = *const c_void;
type SCDynamicStoreCallBack =
    Option<unsafe extern "C" fn(SCDynamicStoreRef, CFArrayRef, *mut c_void)>;

#[link(name = "SystemConfiguration", kind = "framework")]
extern "C" {
    fn SCDynamicStoreCreate(
        allocator: *const c_void,
        name: CFStringRef,
        callout: SCDynamicStoreCallBack,
        context: *mut c_void,
    ) -> SCDynamicStoreRef;
    fn SCDynamicStoreCopyValue(store: SCDynamicStoreRef, key: CFStringRef) -> CFPropertyListRef;
    fn SCNetworkInterfaceCopyAll() -> CFArrayRef;
    fn SCNetworkInterfaceGetBSDName(interface: SCNetworkInterfaceRef) -> CFStringRef;
    fn SCNetworkInterfaceGetInterfaceType(interface: SCNetworkInterfaceRef) -> CFStringRef;
}

/// Owns one non-null Core Foundation object returned by a Create/Copy function.
struct OwnedCf(CFTypeRef);

impl OwnedCf {
    fn from_created(value: CFTypeRef) -> Option<Self> {
        if value.is_null() {
            None
        } else {
            Some(Self(value))
        }
    }
}

impl Drop for OwnedCf {
    fn drop(&mut self) {
        // SAFETY: `OwnedCf` is constructed only from a non-null Create/Copy result,
        // owns exactly one retain, and releases it once here.
        unsafe { CFRelease(self.0) };
    }
}

fn cf_string(value: &str) -> Option<OwnedCf> {
    let value = CString::new(value).ok()?;
    // SAFETY: the C string is NUL-terminated and lives for the entire call; a
    // null allocator selects the default allocator, and UTF-8 is a valid encoding.
    let string =
        unsafe { CFStringCreateWithCString(ptr::null(), value.as_ptr(), kCFStringEncodingUTF8) };
    OwnedCf::from_created(string.cast())
}

fn string_type_id() -> CFTypeID {
    // SAFETY: `CFStringGetTypeID` has no arguments or memory-safety preconditions.
    unsafe { CFStringGetTypeID() }
}

fn dictionary_type_id() -> CFTypeID {
    // SAFETY: `CFDictionaryGetTypeID` has no arguments or memory-safety preconditions.
    unsafe { CFDictionaryGetTypeID() }
}

fn boolean_type_id() -> CFTypeID {
    // SAFETY: `CFBooleanGetTypeID` has no arguments or memory-safety preconditions.
    unsafe { CFBooleanGetTypeID() }
}

fn cf_type_ids_equal(value: CFTypeRef, expected_type_id: CFTypeID) -> bool {
    if value.is_null() {
        return false;
    }
    // SAFETY: `value` is a live Core Foundation object; CFGetTypeID only reads its
    // runtime type. `expected_type_id` is compared numerically, not dereferenced.
    unsafe { CFGetTypeID(value) == expected_type_id }
}

fn cf_string_to_string(value: CFTypeRef) -> Option<String> {
    if !cf_type_ids_equal(value, string_type_id()) {
        return None;
    }

    // BSD interface names and SC interface types are short ASCII strings. A
    // bounded buffer avoids trusting a framework-provided length.
    let mut buffer = [0_u8; 256];
    // SAFETY: `value` was verified as a CFString, and `buffer` is writable for
    // exactly the byte count passed to Core Foundation.
    let success = unsafe {
        CFStringGetCString(
            value.cast(),
            buffer.as_mut_ptr().cast(),
            buffer.len() as CFIndex,
            kCFStringEncodingUTF8,
        )
    };
    if success == 0 {
        return None;
    }
    let length = buffer.iter().position(|byte| *byte == 0)?;
    std::str::from_utf8(&buffer[..length])
        .ok()
        .map(str::to_owned)
}

fn dynamic_store() -> Option<OwnedCf> {
    let name = cf_string("screentime-pro.network-status")?;
    // SAFETY: `name` is a live CFString, callback/context are intentionally null,
    // and the returned Create-rule object is immediately placed under `OwnedCf`.
    let store = unsafe { SCDynamicStoreCreate(ptr::null(), name.0.cast(), None, ptr::null_mut()) };
    OwnedCf::from_created(store)
}

fn copy_store_value(store: &OwnedCf, key: &str) -> Option<OwnedCf> {
    let key = cf_string(key)?;
    // SAFETY: both arguments are live objects of their required CF types. The
    // Copy-rule result is null-checked and owned by `OwnedCf`.
    let value = unsafe { SCDynamicStoreCopyValue(store.0, key.0.cast()) };
    OwnedCf::from_created(value)
}

fn dictionary_value(dictionary: &OwnedCf, key: &str) -> Option<CFTypeRef> {
    if !cf_type_ids_equal(dictionary.0, dictionary_type_id()) {
        return None;
    }
    let key = cf_string(key)?;
    // SAFETY: the dictionary's runtime type was checked and it remains retained;
    // the CFString key is also live. The borrowed value remains valid while the
    // dictionary is alive and is consumed before this function's caller drops it.
    let value = unsafe { CFDictionaryGetValue(dictionary.0 as CFDictionaryRef, key.0.cast()) };
    if value.is_null() {
        None
    } else {
        Some(value)
    }
}

fn dictionary_string(dictionary: &OwnedCf, key: &str) -> Option<String> {
    cf_string_to_string(dictionary_value(dictionary, key)?)
}

fn dictionary_bool(dictionary: &OwnedCf, key: &str) -> Option<bool> {
    let value = dictionary_value(dictionary, key)?;
    if !cf_type_ids_equal(value, boolean_type_id()) {
        return None;
    }
    // SAFETY: `value` was checked to be a CFBoolean and is kept alive by its
    // parent dictionary for the duration of this read.
    Some(unsafe { CFBooleanGetValue(value.cast()) })
}

fn primary_interfaces(store: &OwnedCf) -> Vec<String> {
    // Keep both address-family choices: a VPN may be primary for only one of them.
    let mut primary = Vec::new();
    for key in ["State:/Network/Global/IPv4", "State:/Network/Global/IPv6"] {
        let Some(global) = copy_store_value(store, key) else {
            continue;
        };
        if let Some(interface) = dictionary_string(&global, "PrimaryInterface") {
            if !interface.is_empty() && !primary.contains(&interface) {
                primary.push(interface);
            }
        }
    }
    primary
}

fn link_state(store: &OwnedCf, interface: &str) -> Option<LinkState> {
    let key = format!("State:/Network/Interface/{interface}/Link");
    let link = copy_store_value(store, &key)?;
    match dictionary_bool(&link, "Active")? {
        true => Some(LinkState::Up),
        false => Some(LinkState::Down),
    }
}

struct InterfaceInfo {
    name: String,
    native_type: Option<String>,
}

fn network_interfaces() -> Option<Vec<InterfaceInfo>> {
    // SAFETY: `SCNetworkInterfaceCopyAll` has no arguments. Its Copy-rule array is
    // immediately null-checked and placed under Core Foundation ownership.
    let interfaces = OwnedCf::from_created(unsafe { SCNetworkInterfaceCopyAll() }.cast())?;
    // SAFETY: SystemConfiguration guarantees a CFArray from CopyAll; the OwnedCf
    // keeps that array alive for this count and all subsequent element reads.
    let count = unsafe { CFArrayGetCount(interfaces.0.cast()) };
    let mut result = Vec::new();

    for index in 0..count {
        // SAFETY: `index` is within the count obtained from this exact live array.
        let interface =
            unsafe { CFArrayGetValueAtIndex(interfaces.0.cast(), index) as SCNetworkInterfaceRef };
        if interface.is_null() {
            continue;
        }
        // SAFETY: array elements are SCNetworkInterface objects kept alive by the
        // array. Both returned strings are borrowed and copied before the array drops.
        let bsd_name = unsafe { SCNetworkInterfaceGetBSDName(interface) };
        let Some(name) = cf_string_to_string(bsd_name.cast()) else {
            continue;
        };
        let native_type = unsafe { SCNetworkInterfaceGetInterfaceType(interface) };
        result.push(InterfaceInfo {
            name,
            native_type: cf_string_to_string(native_type.cast()),
        });
    }
    Some(result)
}

fn native_type_for<'a>(interfaces: &'a [InterfaceInfo], name: &str) -> Option<&'a str> {
    interfaces
        .iter()
        .find(|interface| interface.name == name)
        .and_then(|interface| interface.native_type.as_deref())
}

fn select_primary_interface<'a>(
    primary: &'a [String],
    interfaces: &[InterfaceInfo],
) -> Option<&'a str> {
    // Do not let an IPv4 physical route hide an IPv6 VPN route (or vice versa).
    primary
        .iter()
        .find(|name| medium_from_interface(name, native_type_for(interfaces, name)) == Medium::Vpn)
        .or_else(|| primary.first())
        .map(String::as_str)
}

fn physical_medium_from_type(native_type: Option<&str>) -> Option<Medium> {
    match native_type {
        Some("IEEE80211") => Some(Medium::Wifi),
        Some("Ethernet") => Some(Medium::Ethernet),
        // Bond/VLAN and name-only fallbacks are not confirmed physical hardware.
        _ => None,
    }
}

fn merge_physical_medium(current: &mut Option<Medium>, candidate: Medium) -> bool {
    match current {
        Some(current) if *current != candidate => false,
        Some(_) => true,
        None => {
            *current = Some(candidate);
            true
        }
    }
}

fn active_physical_medium(
    store: &OwnedCf,
    primary: &[String],
    selected: &str,
    interfaces: &[InterfaceInfo],
) -> Option<Medium> {
    let mut found = None;

    // A non-VPN global primary identifies the underlying route more reliably than
    // interface enumeration. It still must have an explicitly active link.
    for name in primary.iter().filter(|name| name.as_str() != selected) {
        let Some(candidate) = physical_medium_from_type(native_type_for(interfaces, name)) else {
            continue;
        };
        if link_state(store, name) != Some(LinkState::Up) {
            continue;
        }
        if !merge_physical_medium(&mut found, candidate) {
            return None;
        }
    }
    if found.is_some() {
        return found;
    }

    // With full-tunnel VPNs there may be no physical global primary left. Accept
    // enumeration only when every active physical interface agrees on the medium;
    // simultaneous Wi-Fi and Ethernet remains ambiguous and returns None.
    for interface in interfaces
        .iter()
        .filter(|interface| interface.name != selected)
    {
        let Some(candidate) = physical_medium_from_type(interface.native_type.as_deref()) else {
            continue;
        };
        if link_state(store, &interface.name) != Some(LinkState::Up) {
            continue;
        }
        if !merge_physical_medium(&mut found, candidate) {
            return None;
        }
    }
    found
}

fn tunnel_from_vpn_name(lower_name: &str) -> Option<TunnelKind> {
    if ["utun", "tun"]
        .iter()
        .any(|prefix| lower_name.starts_with(prefix))
    {
        Some(TunnelKind::Tun)
    } else if lower_name.starts_with("tap") {
        Some(TunnelKind::Tap)
    } else if lower_name.starts_with("ppp") {
        Some(TunnelKind::Ppp)
    } else if ["wg", "wireguard", "tailscale"]
        .iter()
        .any(|prefix| lower_name.starts_with(prefix))
    {
        Some(TunnelKind::Wireguard)
    } else if ["ipsec", "vpn", "zt"]
        .iter()
        .any(|prefix| lower_name.starts_with(prefix))
    {
        Some(TunnelKind::Other)
    } else {
        None
    }
}

fn vpn_tunnel_kind(lower_name: &str, native_type: Option<&str>) -> Option<TunnelKind> {
    // A known BSD name describes the actual tunnel device more specifically than
    // the SystemConfiguration service type (for example, IKEv2 over utun).
    if let Some(tunnel) = tunnel_from_vpn_name(lower_name) {
        return Some(tunnel);
    }
    let native_type = native_type?.to_ascii_lowercase();
    match native_type.as_str() {
        "l2tp" | "pptp" | "ppp" => Some(TunnelKind::Ppp),
        "ipsec" | "ikev2" => Some(TunnelKind::Other),
        _ => None,
    }
}

fn tunnel_from_interface(name: &str, native_type: Option<&str>) -> TunnelKind {
    vpn_tunnel_kind(&name.to_ascii_lowercase(), native_type).unwrap_or(TunnelKind::NotApplicable)
}

/// Maps SystemConfiguration's interface type, with conservative BSD-name fallback.
fn medium_from_interface(name: &str, native_type: Option<&str>) -> Medium {
    let lower_name = name.to_ascii_lowercase();
    if vpn_tunnel_kind(&lower_name, native_type).is_some() {
        return Medium::Vpn;
    }

    match native_type {
        Some("IEEE80211") => Medium::Wifi,
        Some("Ethernet" | "Bond" | "VLAN") => Medium::Ethernet,
        Some(_) => Medium::Other,
        None if lower_name.starts_with("bond") || lower_name.starts_with("vlan") => {
            Medium::Ethernet
        }
        None if [
            "lo", "awdl", "llw", "bridge", "gif", "stf", "anpi", "ap", "pdp_ip",
        ]
        .iter()
        .any(|prefix| lower_name.starts_with(prefix)) =>
        {
            Medium::Other
        }
        // In particular, an unadorned `en*` cannot be called Wi-Fi or wired
        // without SystemConfiguration's type information.
        None => Medium::Unknown,
    }
}

/// Maps CoreWLAN's `CWChannelBand` (`NSInteger`) without querying network identity.
fn band_from_channel_band(raw: isize) -> Band {
    match raw {
        0 => Band::Unknown,              // kCWChannelBandUnknown
        1 => Band::Ghz24,                // kCWChannelBand2GHz
        2 => Band::Ghz5,                 // kCWChannelBand5GHz
        3 => Band::Ghz6,                 // kCWChannelBand6GHz
        raw if raw < 0 => Band::Unknown, // Invalid/unsupported value.
        _ => Band::Other,                // Forward-compatible with future Apple bands.
    }
}

fn core_wlan_class() -> Option<&'static AnyClass> {
    // Objective-C classes remain registered with the runtime, so keep the
    // framework loaded for the process lifetime instead of repeatedly dlclose-ing
    // a framework whose class/singleton objects may still exist.
    static CORE_WLAN: OnceLock<Option<libloading::Library>> = OnceLock::new();
    let library = CORE_WLAN.get_or_init(|| {
        // SAFETY: this loads only Apple's immutable system framework from an
        // absolute path, with no user-controlled search path. The handle is kept
        // forever, so all runtime class references and objects remain valid.
        unsafe {
            libloading::Library::new("/System/Library/Frameworks/CoreWLAN.framework/CoreWLAN")
        }
        .ok()
    });
    library.as_ref()?;
    AnyClass::get(c"CWWiFiClient")
}

fn wifi_band(interface_name: &str) -> Band {
    // An unavailable framework/class is an ordinary unsupported result, not a
    // missing-class panic. Nil client/interface/channel values follow the same path.
    let Some(client_class) = core_wlan_class() else {
        return Band::Unknown;
    };

    autoreleasepool(|_| {
        let interface_name = NSString::from_str(interface_name);
        // SAFETY: CWWiFiClient responds to +sharedWiFiClient. The method returns a
        // nullable, non-consumed object; Option<Retained> applies ARC correctly.
        let client: Option<Retained<AnyObject>> =
            unsafe { msg_send![client_class, sharedWiFiClient] };
        let Some(client) = client else {
            return Band::Unknown;
        };

        // SAFETY: CWWiFiClient implements -interfaceWithName: and expects an
        // NSString. The name is live, and a nullable +0 result is retained safely.
        let interface: Option<Retained<AnyObject>> =
            unsafe { msg_send![&*client, interfaceWithName: &*interface_name] };
        let Some(interface) = interface else {
            return Band::Unknown;
        };

        // SAFETY: every CWInterface is an NSObject, so respondsToSelector: is valid
        // and sel!(wlanChannel) has the correct selector encoding.
        let has_channel: bool =
            unsafe { msg_send![&*interface, respondsToSelector: sel!(wlanChannel)] };
        if !has_channel {
            return Band::Unknown;
        }
        // SAFETY: the selector check above established support. `wlanChannel`
        // returns a nullable, non-consumed CWChannel, retained by Option<Retained>.
        let channel: Option<Retained<AnyObject>> = unsafe { msg_send![&*interface, wlanChannel] };
        let Some(channel) = channel else {
            return Band::Unknown;
        };

        // SAFETY: CWChannel is an NSObject; querying selector support is valid.
        let has_band: bool = unsafe { msg_send![&*channel, respondsToSelector: sel!(channelBand)] };
        if !has_band {
            return Band::Unknown;
        }
        // SAFETY: the selector check established support, and CoreWLAN declares
        // channelBand as CWChannelBand, an NSInteger (`isize` on 64-bit macOS).
        let raw_band: isize = unsafe { msg_send![&*channel, channelBand] };
        band_from_channel_band(raw_band)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_interface_types_map_to_expected_media() {
        let cases = [
            ("en0", Some("IEEE80211"), Medium::Wifi),
            ("en7", Some("Ethernet"), Medium::Ethernet),
            ("bond0", Some("Bond"), Medium::Ethernet),
            ("vlan3", Some("VLAN"), Medium::Ethernet),
            ("utun4", Some("IPSec"), Medium::Vpn),
            ("ppp0", Some("PPP"), Medium::Vpn),
            ("ipsec0", Some("L2TP"), Medium::Vpn),
            ("en0", Some("FireWire"), Medium::Other),
            ("pdp_ip0", Some("WWAN"), Medium::Other),
        ];
        for (name, native_type, expected) in cases {
            assert_eq!(
                medium_from_interface(name, native_type),
                expected,
                "interface {name}, native type {native_type:?}"
            );
        }
    }

    #[test]
    fn missing_interface_type_does_not_guess_wifi_or_ethernet() {
        assert_eq!(
            medium_from_interface("en0", None),
            Medium::Unknown,
            "an en* name alone does not identify its physical medium"
        );
        assert_eq!(
            medium_from_interface("utun2", None),
            Medium::Vpn,
            "well-known tunnel names still identify a VPN"
        );
        assert_eq!(
            medium_from_interface("awdl0", None),
            Medium::Other,
            "known Apple pseudo-interfaces are other, not Ethernet/Wi-Fi"
        );
    }

    #[test]
    fn vpn_names_and_native_types_preserve_tunnel_kind() {
        let cases = [
            ("utun0", None, TunnelKind::Tun),
            ("utun4", Some("IPSec"), TunnelKind::Tun),
            ("tun0", None, TunnelKind::Tun),
            ("tap0", None, TunnelKind::Tap),
            ("ppp0", Some("PPP"), TunnelKind::Ppp),
            ("ipsec0", Some("IPSec"), TunnelKind::Other),
            ("wg0", None, TunnelKind::Wireguard),
            ("wireguard0", None, TunnelKind::Wireguard),
            ("tailscale0", None, TunnelKind::Wireguard),
            ("zt7nnig26", None, TunnelKind::Other),
            ("en0", Some("L2TP"), TunnelKind::Ppp),
            ("en0", Some("PPTP"), TunnelKind::Ppp),
            ("en0", Some("PPP"), TunnelKind::Ppp),
            ("en0", Some("IKEv2"), TunnelKind::Other),
        ];
        for (name, native_type, expected) in cases {
            assert_eq!(
                medium_from_interface(name, native_type),
                Medium::Vpn,
                "interface {name}, native type {native_type:?}"
            );
            assert_eq!(
                tunnel_from_interface(name, native_type),
                expected,
                "interface {name}, native type {native_type:?}"
            );
        }
    }

    #[test]
    fn non_vpn_interfaces_do_not_report_a_tunnel() {
        for (name, native_type) in [
            ("en0", Some("IEEE80211")),
            ("en7", Some("Ethernet")),
            ("awdl0", None),
            ("mystery0", None),
        ] {
            assert_eq!(
                tunnel_from_interface(name, native_type),
                TunnelKind::NotApplicable
            );
        }
    }

    #[test]
    fn primary_selection_does_not_hide_vpn_behind_other_address_family() {
        let primary = vec![String::from("en0"), String::from("utun2")];
        let interfaces = vec![
            InterfaceInfo {
                name: String::from("en0"),
                native_type: Some(String::from("IEEE80211")),
            },
            InterfaceInfo {
                name: String::from("utun2"),
                native_type: Some(String::from("IPSec")),
            },
        ];
        assert_eq!(
            select_primary_interface(&primary, &interfaces),
            Some("utun2")
        );
    }

    #[test]
    fn physical_medium_requires_a_specific_hardware_type() {
        assert_eq!(
            physical_medium_from_type(Some("IEEE80211")),
            Some(Medium::Wifi)
        );
        assert_eq!(
            physical_medium_from_type(Some("Ethernet")),
            Some(Medium::Ethernet)
        );
        for native_type in [Some("Bond"), Some("VLAN"), Some("FireWire"), None] {
            assert_eq!(physical_medium_from_type(native_type), None);
        }
    }

    #[test]
    fn conflicting_active_physical_media_are_rejected() {
        let mut found = None;
        assert!(merge_physical_medium(&mut found, Medium::Wifi));
        assert!(merge_physical_medium(&mut found, Medium::Wifi));
        assert!(!merge_physical_medium(&mut found, Medium::Ethernet));
    }

    #[test]
    fn core_wlan_channel_bands_cover_known_unknown_and_future_values() {
        let cases = [
            (0, Band::Unknown),
            (1, Band::Ghz24),
            (2, Band::Ghz5),
            (3, Band::Ghz6),
            (-1, Band::Unknown),
            (4, Band::Other),
            (isize::MAX, Band::Other),
        ];
        for (raw, expected) in cases {
            assert_eq!(
                band_from_channel_band(raw),
                expected,
                "CoreWLAN channel-band value {raw}"
            );
        }
    }
}
