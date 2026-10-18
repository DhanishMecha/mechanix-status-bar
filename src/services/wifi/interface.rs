//! NetworkManager D-Bus names and signal match rules used by this service.
use mecha_wayland::prelude::*;

pub(super) const NM: &str = "org.freedesktop.NetworkManager";
pub(super) const PATH: &str = "/org/freedesktop/NetworkManager";
pub(super) const DEVICE: &str = "org.freedesktop.NetworkManager.Device";
pub(super) const WIRELESS: &str = "org.freedesktop.NetworkManager.Device.Wireless";
pub(super) const AP: &str = "org.freedesktop.NetworkManager.AccessPoint";
pub(super) const WIFI_DEVICE: u32 = 2;
pub(super) const ACTIVATED: u32 = 100;

pub(super) fn properties_changed() -> MatchRule {
    MatchRule::signal()
        .sender(NM)
        .path_namespace(PATH)
        .interface("org.freedesktop.DBus.Properties")
        .member("PropertiesChanged")
}

pub(super) fn devices_changed() -> MatchRule {
    MatchRule::signal().sender(NM).path(PATH).interface(NM)
}

pub(super) fn owner_changed() -> MatchRule {
    MatchRule::signal()
        .sender("org.freedesktop.DBus")
        .interface("org.freedesktop.DBus")
        .member("NameOwnerChanged")
        .arg(0, NM)
}
