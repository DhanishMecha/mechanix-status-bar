//! Read-only NetworkManager client on mecha-wayland's system D-Bus connection.
use std::collections::HashMap;

use crate::{StatusBar, StatusUpdate, WifiState};
use mecha_wayland::prelude::*;

const NM: &str = "org.freedesktop.NetworkManager";
const PATH: &str = "/org/freedesktop/NetworkManager";
const DEVICE: &str = "org.freedesktop.NetworkManager.Device";
const WIRELESS: &str = "org.freedesktop.NetworkManager.Device.Wireless";
const AP: &str = "org.freedesktop.NetworkManager.AccessPoint";

/// Latest observed Wi-Fi state. No active connection is assumed at startup.
pub struct WifiStatus {
    pub state: WifiState,
    enabled: bool,
    hardware: bool,
    pending: HashMap<CallId, Request>,
    connected: Vec<u8>,
    dirty: bool,
    properties: MatchId,
    devices: MatchId,
    owner: MatchId,
}
impl Resource for WifiStatus {}

enum Request {
    Manager,
    Devices,
    Device(ObjectPath),
    Wireless,
    AccessPoint,
}

/// Install after `RingModule` and `DBusModule::new().system()`.
pub struct WifiModule;
impl Module for WifiModule {
    fn install(self, app: &mut App) {
        let (properties, devices, owner) = {
            let mut dbus = app.resource_mut::<DBus>();
            let mut bus = dbus.system();
            let properties = bus.add_match(
                MatchRule::signal()
                    .sender(NM)
                    .path_namespace(PATH)
                    .interface("org.freedesktop.DBus.Properties")
                    .member("PropertiesChanged"),
            );
            let devices = bus.add_match(MatchRule::signal().sender(NM).path(PATH).interface(NM));
            let owner = bus.add_match(
                MatchRule::signal()
                    .sender("org.freedesktop.DBus")
                    .interface("org.freedesktop.DBus")
                    .member("NameOwnerChanged")
                    .arg(0, NM),
            );
            (properties, devices, owner)
        };
        app.insert_resource(WifiStatus {
            state: WifiState::Disabled,
            enabled: false,
            hardware: false,
            pending: HashMap::new(),
            connected: Vec::new(),
            dirty: false,
            properties,
            devices,
            owner,
        });
        app.system(on_reply)
            .system(on_signal)
            .system(on_disconnected)
            .system(publish);
        refresh(app);
    }
}

fn get_all(app: &mut App, path: &str, interface: &'static str, request: Request) {
    let call = app
        .resource_mut::<DBus>()
        .system()
        .object(
            NM,
            ObjectPath::new(path).expect("valid NetworkManager object path"),
        )
        .get_all(interface);
    app.resource_mut::<WifiStatus>()
        .pending
        .insert(call, request);
}

fn refresh(app: &mut App) {
    if !app.resource::<WifiStatus>().pending.is_empty() {
        app.resource_mut::<WifiStatus>().dirty = true;
        return;
    }
    {
        let mut wifi = app.resource_mut::<WifiStatus>();
        wifi.dirty = false;
        wifi.connected.clear();
    }
    get_all(app, PATH, NM, Request::Manager);
}

fn property<T: mecha_wayland::dbus::Decode + mecha_wayland::dbus::Type>(
    props: &PropMap,
    key: &str,
) -> Option<T> {
    props.get_as::<T>(key).and_then(Result::ok)
}

fn on_reply(app: &mut App, event: &MethodReply) {
    let Some(request) = app.resource_mut::<WifiStatus>().pending.remove(&event.call) else {
        return;
    };
    match request {
        Request::Devices => {
            if let Ok(message) = &event.result {
                if let Ok((paths,)) = message.body::<(Vec<ObjectPath>,)>() {
                    for path in paths {
                        get_all(app, path.as_str(), DEVICE, Request::Device(path.clone()));
                    }
                }
            }
        }
        request => {
            let props = event
                .result
                .as_ref()
                .ok()
                .and_then(|m| m.body::<(PropMap,)>().ok())
                .map(|(p,)| p);
            match (request, props) {
                (Request::Manager, Some(props)) => {
                    let enabled = property::<bool>(&props, "WirelessEnabled").unwrap_or(false);
                    let hardware =
                        property::<bool>(&props, "WirelessHardwareEnabled").unwrap_or(false);
                    {
                        let mut wifi = app.resource_mut::<WifiStatus>();
                        wifi.enabled = enabled;
                        wifi.hardware = hardware;
                    }
                    if enabled && hardware {
                        let call = app.resource_mut::<DBus>().system().object(NM, PATH).call(
                            NM,
                            "GetDevices",
                            &(),
                        );
                        app.resource_mut::<WifiStatus>()
                            .pending
                            .insert(call, Request::Devices);
                    }
                }
                (Request::Manager, None) => {
                    let mut wifi = app.resource_mut::<WifiStatus>();
                    wifi.enabled = false;
                    wifi.hardware = false;
                }
                (Request::Device(path), Some(props)) => {
                    if property::<u32>(&props, "DeviceType") == Some(2)
                        && property::<u32>(&props, "State") == Some(100)
                    {
                        get_all(app, path.as_str(), WIRELESS, Request::Wireless);
                    }
                }
                (Request::Wireless, Some(props)) => {
                    if let Some(path) = property::<ObjectPath>(&props, "ActiveAccessPoint") {
                        if path.as_str() != "/" {
                            get_all(app, path.as_str(), AP, Request::AccessPoint);
                        }
                    }
                }
                (Request::AccessPoint, Some(props)) => {
                    app.resource_mut::<WifiStatus>()
                        .connected
                        .push(property::<u8>(&props, "Strength").unwrap_or(0));
                }
                _ => {}
            }
        }
    }
    if app.resource::<WifiStatus>().pending.is_empty() {
        if app.resource::<WifiStatus>().dirty {
            refresh(app);
        } else {
            let state = {
                let wifi = app.resource::<WifiStatus>();
                classify(
                    wifi.enabled,
                    wifi.hardware,
                    wifi.connected.iter().copied().max(),
                )
            };
            if app.resource::<WifiStatus>().state != state {
                app.resource_mut::<WifiStatus>().state = state;
            }
        }
    }
}

fn classify(enabled: bool, hardware: bool, strength: Option<u8>) -> WifiState {
    if !enabled || !hardware {
        return WifiState::Disabled;
    }
    match strength {
        None => WifiState::Disconnected,
        Some(0..=29) => WifiState::None,
        Some(30..=69) => WifiState::Low,
        Some(_) => WifiState::High,
    }
}

fn on_signal(app: &mut App, event: &SignalReceived) {
    let wifi = app.resource::<WifiStatus>();
    let owner = event.matches.contains(&wifi.owner);
    let properties = event.matches.contains(&wifi.properties);
    let devices = event.matches.contains(&wifi.devices);
    if owner {
        if let Ok((_, _, new_owner)) = event.msg.body::<(String, String, String)>() {
            let mut wifi = app.resource_mut::<WifiStatus>();
            wifi.pending.clear();
            wifi.connected.clear();
            wifi.dirty = false;
            wifi.state = WifiState::Disabled;
            drop(wifi);
            if !new_owner.is_empty() {
                refresh(app);
            }
        }
    } else if properties {
        if let Ok((interface, changed, invalidated)) = PropertiesChanged::parse(&event.msg) {
            let keys: &[&str] = match interface.as_str() {
                NM => &["WirelessEnabled", "WirelessHardwareEnabled"],
                DEVICE => &["State", "DeviceType"],
                WIRELESS => &["ActiveAccessPoint"],
                AP => &["Strength"],
                _ => &[],
            };
            if keys
                .iter()
                .any(|key| changed.contains_key(*key) || invalidated.iter().any(|v| v == key))
            {
                refresh(app);
            }
        }
    } else if devices
        && matches!(
            event.msg.member().map(|m| m.as_str()),
            Some("DeviceAdded" | "DeviceRemoved")
        )
    {
        refresh(app);
    }
}

fn on_disconnected(app: &mut App, event: &BusDisconnected) {
    if event.bus == BusId::System {
        let mut wifi = app.resource_mut::<WifiStatus>();
        wifi.pending.clear();
        wifi.dirty = false;
        wifi.state = WifiState::Disabled;
    }
}

fn publish(app: &mut App, _: &OnChanged<WifiStatus>) {
    let state = app.resource::<WifiStatus>().state;
    let bars: Vec<NodeId> = app.widgets::<StatusBar>().map(|(id, _)| id).collect();
    app.emit(StatusUpdate::Wifi(state), bars);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn radio_connection_and_strength_states() {
        assert_eq!(classify(false, true, Some(100)), WifiState::Disabled);
        assert_eq!(classify(true, false, Some(100)), WifiState::Disabled);
        assert_eq!(classify(true, true, None), WifiState::Disconnected);
        for (strength, state) in [
            (0, WifiState::None),
            (29, WifiState::None),
            (30, WifiState::Low),
            (69, WifiState::Low),
            (70, WifiState::High),
            (100, WifiState::High),
        ] {
            assert_eq!(classify(true, true, Some(strength)), state);
        }
    }
}
