//! Observe NetworkManager's Wi-Fi radio, connected devices, and signal strength.
use std::collections::{HashMap, HashSet};

use crate::{StatusBar, StatusUpdate, WifiState};
use mecha_wayland::prelude::*;

const NM: &str = "org.freedesktop.NetworkManager";
const PATH: &str = "/org/freedesktop/NetworkManager";
const DEVICE: &str = "org.freedesktop.NetworkManager.Device";
const WIRELESS: &str = "org.freedesktop.NetworkManager.Device.Wireless";
const AP: &str = "org.freedesktop.NetworkManager.AccessPoint";
const WIFI_DEVICE: u32 = 2;
const ACTIVATED: u32 = 100;

#[derive(Default)]
struct Snapshot {
    radio: Option<bool>,
    connected: bool,
    strength: Option<u8>,
    active_aps: HashSet<ObjectPath>,
    valid: bool,
}

impl Snapshot {
    fn state(&self) -> Option<WifiState> {
        if !self.valid {
            return None;
        }
        match self.radio? {
            false => Some(WifiState::Disabled),
            true if self.connected => Some(match self.strength {
                Some(0..=24) | None => WifiState::None,
                Some(25..=49) => WifiState::Low,
                Some(50..=74) => WifiState::Medium,
                Some(_) => WifiState::High,
            }),
            true => Some(WifiState::Disconnected),
        }
    }

    fn record_strength(&mut self, strength: u8) {
        self.strength = Some(
            self.strength
                .map_or(strength, |current| current.max(strength)),
        );
    }

    fn manager(&mut self, props: Option<&PropMap>) -> bool {
        self.radio = props.and_then(|props| {
            Some(
                prop::<bool>(props, "WirelessEnabled")?
                    && prop::<bool>(props, "WirelessHardwareEnabled")?,
            )
        });
        self.valid &= self.radio.is_some();
        self.radio == Some(true)
    }

    fn device(&mut self, props: Option<&PropMap>) -> bool {
        let Some(props) = props else {
            self.valid = false;
            return false;
        };
        match prop::<u32>(props, "DeviceType") {
            Some(WIFI_DEVICE) => match prop::<u32>(props, "State") {
                Some(ACTIVATED) => {
                    self.connected = true;
                    true
                }
                Some(_) => false,
                None => {
                    self.valid = false;
                    false
                }
            },
            Some(_) => false,
            None => {
                self.valid = false;
                false
            }
        }
    }

    fn wireless(&mut self, props: Option<&PropMap>) -> Option<ObjectPath> {
        let ap = props.and_then(|props| prop::<ObjectPath>(props, "ActiveAccessPoint"));
        match ap {
            Some(path) if path.as_str() != "/" => {
                self.active_aps.insert(path.clone()).then_some(path)
            }
            Some(_) => None,
            None => {
                self.valid = false;
                None
            }
        }
    }

    fn access_point(&mut self, path: &ObjectPath, props: Option<&PropMap>) {
        if let Some(props) = props {
            if self.active_aps.contains(path) {
                if let Some(strength) = prop::<u8>(props, "Strength") {
                    self.record_strength(strength);
                }
            }
        } else {
            self.valid = false;
        }
    }
}

enum Request {
    Manager,
    Devices,
    Device(ObjectPath),
    Wireless,
    AccessPoint(ObjectPath),
}

/// Latest observed Wi-Fi state.
pub struct WifiStatus {
    pub state: WifiState,
    pending: HashMap<CallId, Request>,
    snapshot: Snapshot,
    active_aps: HashSet<ObjectPath>,
    dirty: bool,
    properties: MatchId,
    devices: MatchId,
    owner: MatchId,
}
impl Resource for WifiStatus {}

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
            pending: HashMap::new(),
            snapshot: Snapshot::default(),
            active_aps: HashSet::new(),
            dirty: false,
            properties,
            devices,
            owner,
        });
        app.system(on_reply)
            .system(on_signal)
            .system(on_disconnected);
        refresh(app);
    }
}

fn get_all(app: &mut App, path: ObjectPath, interface: &'static str, request: Request) {
    let call = app
        .resource_mut::<DBus>()
        .system()
        .object(NM, path)
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
        wifi.snapshot = Snapshot {
            valid: true,
            ..Snapshot::default()
        };
    }
    get_all(
        app,
        ObjectPath::new(PATH).expect("valid NetworkManager path"),
        NM,
        Request::Manager,
    );
}

fn prop<T: mecha_wayland::dbus::Decode + mecha_wayland::dbus::Type>(
    props: &PropMap,
    key: &str,
) -> Option<T> {
    props.get_as::<T>(key).and_then(Result::ok)
}

fn relevant_change(
    interface: &str,
    path: Option<&ObjectPath>,
    changed: &PropMap,
    invalidated: &[String],
    active_aps: &HashSet<ObjectPath>,
    scan_aps: &HashSet<ObjectPath>,
) -> bool {
    let keys: &[&str] = match interface {
        NM if path.is_some_and(|path| path.as_str() == PATH) => {
            &["WirelessEnabled", "WirelessHardwareEnabled"]
        }
        DEVICE => &["State", "DeviceType"],
        WIRELESS => &["ActiveAccessPoint"],
        AP if path.is_some_and(|path| active_aps.contains(path) || scan_aps.contains(path)) => {
            &["Strength"]
        }
        _ => &[],
    };
    keys.iter()
        .any(|key| changed.contains_key(*key) || invalidated.iter().any(|v| v == key))
}

fn publish(app: &mut App, state: WifiState) {
    let previous = app.resource::<WifiStatus>().state;
    if previous == state {
        return;
    }
    // eprintln!("[wifi] state: {previous:?} -> {state:?}");
    app.resource_mut::<WifiStatus>().state = state;
    let bars: Vec<NodeId> = app.widgets::<StatusBar>().map(|(id, _)| id).collect();
    app.emit(StatusUpdate::Wifi(state), bars);
}

fn reply_props(event: &MethodReply, interface: &str) -> Option<PropMap> {
    event
        .result
        .as_ref()
        .map_err(|error| eprintln!("[wifi] {interface} GetAll failed: {error:?}"))
        .ok()
        .and_then(|message| {
            message
                .body::<(PropMap,)>()
                .map_err(|error| eprintln!("[wifi] failed to parse {interface} GetAll: {error:?}"))
                .ok()
        })
        .map(|(props,)| props)
}

fn on_reply(app: &mut App, event: &MethodReply) {
    let Some(request) = app.resource_mut::<WifiStatus>().pending.remove(&event.call) else {
        return;
    };

    match request {
        Request::Manager => {
            let props = reply_props(event, NM);
            if app
                .resource_mut::<WifiStatus>()
                .snapshot
                .manager(props.as_ref())
            {
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
        Request::Devices => {
            if let Some(paths) = event
                .result
                .as_ref()
                .map_err(|error| eprintln!("[wifi] GetDevices failed: {error:?}"))
                .ok()
                .and_then(|message| {
                    message
                        .body::<(Vec<ObjectPath>,)>()
                        .map_err(|error| eprintln!("[wifi] failed to parse GetDevices: {error:?}"))
                        .ok()
                })
                .map(|(paths,)| paths)
            {
                for path in paths {
                    get_all(app, path.clone(), DEVICE, Request::Device(path));
                }
            } else {
                app.resource_mut::<WifiStatus>().snapshot.valid = false;
            }
        }
        Request::Device(path) => {
            let props = reply_props(event, DEVICE);
            if app
                .resource_mut::<WifiStatus>()
                .snapshot
                .device(props.as_ref())
            {
                get_all(app, path, WIRELESS, Request::Wireless);
            }
        }
        Request::Wireless => {
            let props = reply_props(event, WIRELESS);
            if let Some(path) = app
                .resource_mut::<WifiStatus>()
                .snapshot
                .wireless(props.as_ref())
            {
                get_all(app, path.clone(), AP, Request::AccessPoint(path));
            }
        }
        Request::AccessPoint(path) => {
            let props = reply_props(event, AP);
            app.resource_mut::<WifiStatus>()
                .snapshot
                .access_point(&path, props.as_ref());
        }
    }

    if app.resource::<WifiStatus>().pending.is_empty() {
        if app.resource::<WifiStatus>().dirty {
            refresh(app);
        } else if let Some(state) = app.resource::<WifiStatus>().snapshot.state() {
            let aps = app.resource::<WifiStatus>().snapshot.active_aps.clone();
            app.resource_mut::<WifiStatus>().active_aps = aps;
            publish(app, state);
        } else {
            eprintln!("[wifi] incomplete NetworkManager snapshot; keeping previous state");
        }
    }
}

fn on_signal(app: &mut App, event: &SignalReceived) {
    let wifi = app.resource::<WifiStatus>();
    let owner = event.matches.contains(&wifi.owner);
    let devices = event.matches.contains(&wifi.devices);
    let properties = event.matches.contains(&wifi.properties);

    if owner {
        if let Ok((_, _, new_owner)) = event.msg.body::<(String, String, String)>() {
            let mut wifi = app.resource_mut::<WifiStatus>();
            wifi.pending.clear();
            wifi.snapshot = Snapshot::default();
            wifi.active_aps.clear();
            wifi.dirty = false;
            publish(app, WifiState::Disabled);
            if !new_owner.is_empty() {
                refresh(app);
            }
        }
    } else if devices
        && matches!(
            event.msg.member().map(|member| member.as_str()),
            Some("DeviceAdded" | "DeviceRemoved")
        )
    {
        refresh(app);
    } else if properties {
        if let Ok((interface, changed, invalidated)) = PropertiesChanged::parse(&event.msg) {
            if relevant_change(
                interface.as_str(),
                event.msg.path(),
                &changed,
                &invalidated,
                &wifi.active_aps,
                &wifi.snapshot.active_aps,
            ) {
                refresh(app);
            }
        }
    }
}

fn on_disconnected(app: &mut App, event: &BusDisconnected) {
    if event.bus == BusId::System {
        eprintln!("[wifi] system D-Bus disconnected; resetting state");
        let mut wifi = app.resource_mut::<WifiStatus>();
        wifi.pending.clear();
        wifi.snapshot = Snapshot::default();
        wifi.active_aps.clear();
        wifi.dirty = false;
        publish(app, WifiState::Disabled);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mecha_wayland::dbus::Value;

    fn props(entries: &[(&str, Value)]) -> PropMap {
        entries
            .iter()
            .map(|(key, value)| (key.to_string(), value.clone()))
            .collect()
    }

    #[test]
    fn radio_and_connection_states() {
        let mut snapshot = Snapshot {
            valid: true,
            radio: Some(false),
            connected: true,
            ..Snapshot::default()
        };
        assert_eq!(snapshot.state(), Some(WifiState::Disabled));
        snapshot.radio = Some(true);
        snapshot.connected = false;
        assert_eq!(snapshot.state(), Some(WifiState::Disconnected));
        snapshot.connected = true;
        assert_eq!(snapshot.state(), Some(WifiState::None));
        for (strength, expected) in [
            (24, WifiState::None),
            (25, WifiState::Low),
            (49, WifiState::Low),
            (50, WifiState::Medium),
            (74, WifiState::Medium),
            (75, WifiState::High),
        ] {
            snapshot.strength = Some(strength);
            assert_eq!(snapshot.state(), Some(expected));
        }
        snapshot.strength = None;
        snapshot.record_strength(20);
        snapshot.record_strength(75);
        snapshot.record_strength(30);
        assert_eq!(snapshot.strength, Some(75));
    }

    #[test]
    fn incomplete_scan_does_not_publish_disconnected() {
        let mut snapshot = Snapshot {
            valid: true,
            ..Snapshot::default()
        };
        assert_eq!(snapshot.state(), None);
        snapshot.radio = Some(true);
        snapshot.valid = false;
        assert_eq!(snapshot.state(), None);
    }

    #[test]
    fn manager_requires_a_complete_radio_read() {
        for (enabled, hardware, expected) in [
            (true, true, Some(WifiState::Disconnected)),
            (false, true, Some(WifiState::Disabled)),
            (true, false, Some(WifiState::Disabled)),
            (false, false, Some(WifiState::Disabled)),
        ] {
            let mut snapshot = Snapshot {
                valid: true,
                ..Snapshot::default()
            };
            let radio = props(&[
                ("WirelessEnabled", Value::Bool(enabled)),
                ("WirelessHardwareEnabled", Value::Bool(hardware)),
            ]);
            assert_eq!(snapshot.manager(Some(&radio)), enabled && hardware);
            assert_eq!(snapshot.state(), expected);
        }
        for radio in [
            None,
            Some(props(&[("WirelessEnabled", Value::Bool(true))])),
            Some(props(&[("WirelessEnabled", Value::U32(1))])),
        ] {
            let mut snapshot = Snapshot {
                valid: true,
                ..Snapshot::default()
            };
            assert!(!snapshot.manager(radio.as_ref()));
            assert_eq!(snapshot.state(), None);
        }
    }

    #[test]
    fn devices_only_follow_activated_wifi_and_preserve_unknown_scans() {
        let mut snapshot = Snapshot {
            radio: Some(true),
            valid: true,
            ..Snapshot::default()
        };
        assert!(!snapshot.device(Some(&props(&[
            ("DeviceType", Value::U32(1)),
            ("State", Value::U32(ACTIVATED)),
        ]))));
        assert!(!snapshot.device(Some(&props(&[
            ("DeviceType", Value::U32(WIFI_DEVICE)),
            ("State", Value::U32(30)),
        ]))));
        assert_eq!(snapshot.state(), Some(WifiState::Disconnected));
        assert!(snapshot.device(Some(&props(&[
            ("DeviceType", Value::U32(WIFI_DEVICE)),
            ("State", Value::U32(ACTIVATED)),
        ]))));
        assert_eq!(snapshot.state(), Some(WifiState::None));
        assert!(!snapshot.device(None));
        assert_eq!(snapshot.state(), None);

        for missing in [
            props(&[("State", Value::U32(ACTIVATED))]),
            props(&[("DeviceType", Value::U32(WIFI_DEVICE))]),
            props(&[("DeviceType", Value::Bool(true))]),
        ] {
            let mut snapshot = Snapshot {
                valid: true,
                radio: Some(true),
                ..Snapshot::default()
            };
            assert!(!snapshot.device(Some(&missing)));
            assert_eq!(snapshot.state(), None);
        }
    }

    #[test]
    fn wireless_ap_reads_deduplicate_and_choose_strongest() {
        let mut snapshot = Snapshot {
            valid: true,
            radio: Some(true),
            connected: true,
            ..Snapshot::default()
        };
        let first = ObjectPath::new("/org/freedesktop/NetworkManager/AccessPoint/1").unwrap();
        let second = ObjectPath::new("/org/freedesktop/NetworkManager/AccessPoint/2").unwrap();
        let no_ap = props(&[(
            "ActiveAccessPoint",
            Value::ObjectPath(ObjectPath::new("/").unwrap()),
        )]);
        assert_eq!(snapshot.wireless(Some(&no_ap)), None);
        assert_eq!(snapshot.state(), Some(WifiState::None));
        let first_props = props(&[("ActiveAccessPoint", Value::ObjectPath(first.clone()))]);
        assert_eq!(snapshot.wireless(Some(&first_props)), Some(first.clone()));
        assert_eq!(snapshot.wireless(Some(&first_props)), None);
        snapshot.access_point(&first, Some(&props(&[("Strength", Value::U8(35))])));
        assert_eq!(snapshot.state(), Some(WifiState::Low));
        let second_props = props(&[("ActiveAccessPoint", Value::ObjectPath(second.clone()))]);
        assert_eq!(snapshot.wireless(Some(&second_props)), Some(second.clone()));
        snapshot.access_point(&second, Some(&props(&[("Strength", Value::U8(80))])));
        assert_eq!(snapshot.state(), Some(WifiState::High));
        snapshot.access_point(&first, Some(&props(&[("Strength", Value::U8(15))])));
        assert_eq!(snapshot.state(), Some(WifiState::High));
        let unrelated = ObjectPath::new("/org/freedesktop/NetworkManager/AccessPoint/3").unwrap();
        snapshot.access_point(&unrelated, Some(&props(&[("Strength", Value::U8(100))])));
        assert_eq!(snapshot.strength, Some(80));
    }

    #[test]
    fn missing_ap_properties_do_not_forge_a_connection_or_strength() {
        for wireless in [None, Some(PropMap::new())] {
            let mut snapshot = Snapshot {
                valid: true,
                radio: Some(true),
                connected: true,
                ..Snapshot::default()
            };
            assert_eq!(snapshot.wireless(wireless.as_ref()), None);
            assert_eq!(snapshot.state(), None);
        }
        let ap = ObjectPath::new("/org/freedesktop/NetworkManager/AccessPoint/1").unwrap();
        let mut snapshot = Snapshot {
            valid: true,
            radio: Some(true),
            connected: true,
            ..Snapshot::default()
        };
        snapshot.active_aps.insert(ap.clone());
        snapshot.access_point(&ap, Some(&PropMap::new()));
        assert_eq!(snapshot.state(), Some(WifiState::None));
        snapshot.access_point(&ap, None);
        assert_eq!(snapshot.state(), None);
    }

    #[test]
    fn only_relevant_property_signals_trigger_refresh() {
        let manager = ObjectPath::new(PATH).unwrap();
        let ap = ObjectPath::new("/org/freedesktop/NetworkManager/AccessPoint/1").unwrap();
        let other = ObjectPath::new("/org/freedesktop/NetworkManager/AccessPoint/2").unwrap();
        let empty = HashSet::new();
        let active = HashSet::from([ap.clone()]);
        let changed = props(&[("Strength", Value::U8(50))]);
        assert!(relevant_change(
            AP,
            Some(&ap),
            &changed,
            &[],
            &active,
            &empty
        ));
        assert!(relevant_change(
            AP,
            Some(&ap),
            &changed,
            &[],
            &empty,
            &active
        ));
        assert!(!relevant_change(
            AP,
            Some(&other),
            &changed,
            &[],
            &active,
            &empty
        ));
        assert!(!relevant_change(AP, None, &changed, &[], &active, &empty));
        assert!(relevant_change(
            WIRELESS,
            Some(&other),
            &PropMap::new(),
            &["ActiveAccessPoint".into()],
            &empty,
            &empty,
        ));
        assert!(relevant_change(
            DEVICE,
            Some(&other),
            &props(&[("DeviceType", Value::U32(WIFI_DEVICE))]),
            &[],
            &empty,
            &empty,
        ));
        assert!(relevant_change(
            NM,
            Some(&manager),
            &props(&[("WirelessEnabled", Value::Bool(false))]),
            &[],
            &empty,
            &empty,
        ));
        assert!(!relevant_change(
            NM,
            Some(&other),
            &props(&[("WirelessEnabled", Value::Bool(false))]),
            &[],
            &empty,
            &empty,
        ));
        assert!(!relevant_change(
            DEVICE,
            Some(&other),
            &props(&[("Ip4Config", Value::U32(1))]),
            &[],
            &empty,
            &empty,
        ));

        let signal = Message::signal(
            ap.clone(),
            "org.freedesktop.DBus.Properties",
            "PropertiesChanged",
        )
        .with_body(&(AP, PropMap::new(), vec!["Strength".to_string()]));
        let (interface, changed, invalidated) = PropertiesChanged::parse(&signal).unwrap();
        assert!(relevant_change(
            interface.as_str(),
            signal.path(),
            &changed,
            &invalidated,
            &active,
            &empty,
        ));
    }
}
