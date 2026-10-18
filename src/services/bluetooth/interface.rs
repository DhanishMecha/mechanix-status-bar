//! BlueZ D-Bus names and signal match rules used by this service.
use mecha_wayland::prelude::*;

pub(super) const BLUEZ: &str = "org.bluez";
pub(super) const ADAPTER: &str = "org.bluez.Adapter1";
pub(super) const DEVICE: &str = "org.bluez.Device1";
pub(super) const OBJECT_MANAGER: &str = "org.freedesktop.DBus.ObjectManager";

pub(super) fn properties_changed() -> MatchRule {
    MatchRule::signal()
        .sender(BLUEZ)
        .path_namespace("/org/bluez")
        .interface("org.freedesktop.DBus.Properties")
        .member("PropertiesChanged")
}

pub(super) fn interfaces_added() -> MatchRule {
    MatchRule::signal()
        .sender(BLUEZ)
        .interface(OBJECT_MANAGER)
        .member("InterfacesAdded")
}

pub(super) fn interfaces_removed() -> MatchRule {
    MatchRule::signal()
        .sender(BLUEZ)
        .interface(OBJECT_MANAGER)
        .member("InterfacesRemoved")
}

pub(super) fn owner_changed() -> MatchRule {
    MatchRule::signal()
        .sender("org.freedesktop.DBus")
        .interface("org.freedesktop.DBus")
        .member("NameOwnerChanged")
        .arg(0, BLUEZ)
}
