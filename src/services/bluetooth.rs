//! Read-only BlueZ client on mecha-wayland's system D-Bus connection.
use std::collections::{HashMap, HashSet};

use crate::{BluetoothState, StatusBar, StatusUpdate};
use mecha_wayland::prelude::*;

const BLUEZ: &str = "org.bluez";
const ADAPTER: &str = "org.bluez.Adapter1";
const DEVICE: &str = "org.bluez.Device1";
const OBJECT_MANAGER: &str = "org.freedesktop.DBus.ObjectManager";

type ManagedObjects = HashMap<ObjectPath, HashMap<String, PropMap>>;

#[derive(Default)]
struct BluetoothObservation {
    adapters: HashMap<ObjectPath, bool>,
    connected_devices: HashSet<ObjectPath>,
}

impl BluetoothObservation {
    fn from_managed_objects(objects: ManagedObjects) -> Self {
        let mut observed = Self::default();
        for (path, interfaces) in objects {
            observed.add_interfaces(path, interfaces);
        }
        observed
    }

    fn add_interfaces(&mut self, path: ObjectPath, interfaces: HashMap<String, PropMap>) {
        if let Some(props) = interfaces.get(ADAPTER) {
            self.adapters.insert(
                path.clone(),
                props
                    .get_as::<bool>("Powered")
                    .and_then(Result::ok)
                    .unwrap_or(false),
            );
        }
        if let Some(props) = interfaces.get(DEVICE) {
            if props.get_as::<bool>("Connected").and_then(Result::ok) == Some(true) {
                self.connected_devices.insert(path);
            } else {
                self.connected_devices.remove(&path);
            }
        }
    }

    fn apply_change(&mut self, change: BluetoothChange) -> bool {
        match change {
            BluetoothChange::Added(path, interfaces) => self.add_interfaces(path, interfaces),
            BluetoothChange::Removed(path, interfaces) => {
                for interface in interfaces {
                    match interface.as_str() {
                        ADAPTER => {
                            self.adapters.remove(&path);
                        }
                        DEVICE => {
                            self.connected_devices.remove(&path);
                        }
                        _ => {}
                    }
                }
            }
            BluetoothChange::Property(path, interface, Some(value)) => match interface {
                ADAPTER => {
                    self.adapters.insert(path, value);
                }
                DEVICE => {
                    if value {
                        self.connected_devices.insert(path);
                    } else {
                        self.connected_devices.remove(&path);
                    }
                }
                _ => {}
            },
            BluetoothChange::Property(_, _, None) => return false,
        }
        true
    }

    fn state(&self) -> BluetoothState {
        if !self.adapters.values().any(|&powered| powered) {
            BluetoothState::Off
        } else if !self.connected_devices.is_empty() {
            BluetoothState::Connected
        } else {
            BluetoothState::On
        }
    }
}

enum BluetoothChange {
    Added(ObjectPath, HashMap<String, PropMap>),
    Removed(ObjectPath, Vec<String>),
    Property(ObjectPath, &'static str, Option<bool>),
}

fn decode_bluez_change(msg: &Message) -> Option<BluetoothChange> {
    match msg.member()?.as_str() {
        "InterfacesAdded" => {
            let (path, interfaces) = msg.body::<(ObjectPath, HashMap<String, PropMap>)>().ok()?;
            (interfaces.contains_key(ADAPTER) || interfaces.contains_key(DEVICE))
                .then_some(BluetoothChange::Added(path, interfaces))
        }
        "InterfacesRemoved" => {
            let (path, interfaces) = msg.body::<(ObjectPath, Vec<String>)>().ok()?;
            interfaces
                .iter()
                .any(|name| name == ADAPTER || name == DEVICE)
                .then_some(BluetoothChange::Removed(path, interfaces))
        }
        "PropertiesChanged" => {
            let (interface, props, invalidated) = PropertiesChanged::parse(msg).ok()?;
            let (interface, key) = match interface.as_str() {
                ADAPTER => (ADAPTER, "Powered"),
                DEVICE => (DEVICE, "Connected"),
                _ => return None,
            };
            if !props.contains_key(key) && !invalidated.iter().any(|name| name == key) {
                return None;
            }
            let value = if invalidated.iter().any(|name| name == key) {
                None
            } else {
                props.get_as::<bool>(key).and_then(Result::ok)
            };
            Some(BluetoothChange::Property(
                msg.path()?.clone(),
                interface,
                value,
            ))
        }
        _ => None,
    }
}

/// Latest observed Bluetooth state, derived from BlueZ adapters and devices.
pub struct BluetoothStatus {
    pub state: BluetoothState,
    observed: BluetoothObservation,
    pending: Option<CallId>,
    dirty: bool,
    properties: MatchId,
    interfaces_added: MatchId,
    interfaces_removed: MatchId,
    owner: MatchId,
}
impl Resource for BluetoothStatus {}

pub struct BluetoothModule;
impl Module for BluetoothModule {
    fn install(self, app: &mut App) {
        let (properties, interfaces_added, interfaces_removed, owner) = {
            let mut dbus = app.resource_mut::<DBus>();
            let mut bus = dbus.system();
            let properties = bus.add_match(
                MatchRule::signal()
                    .sender(BLUEZ)
                    .path_namespace("/org/bluez")
                    .interface("org.freedesktop.DBus.Properties")
                    .member("PropertiesChanged"),
            );
            let interfaces_added = bus.add_match(
                MatchRule::signal()
                    .sender(BLUEZ)
                    .interface(OBJECT_MANAGER)
                    .member("InterfacesAdded"),
            );
            let interfaces_removed = bus.add_match(
                MatchRule::signal()
                    .sender(BLUEZ)
                    .interface(OBJECT_MANAGER)
                    .member("InterfacesRemoved"),
            );
            let owner = bus.add_match(
                MatchRule::signal()
                    .sender("org.freedesktop.DBus")
                    .interface("org.freedesktop.DBus")
                    .member("NameOwnerChanged")
                    .arg(0, BLUEZ),
            );
            (properties, interfaces_added, interfaces_removed, owner)
        };
        app.insert_resource(BluetoothStatus {
            state: BluetoothState::Off,
            observed: BluetoothObservation::default(),
            pending: None,
            dirty: false,
            properties,
            interfaces_added,
            interfaces_removed,
            owner,
        });
        app.system(on_snapshot_reply)
            .system(on_signal)
            .system(on_disconnected);
        request_snapshot(app);
    }
}

fn request_snapshot(app: &mut App) {
    if app.resource::<BluetoothStatus>().pending.is_some() {
        app.resource_mut::<BluetoothStatus>().dirty = true;
        return;
    }
    let call = app.resource_mut::<DBus>().system().object(BLUEZ, "/").call(
        OBJECT_MANAGER,
        "GetManagedObjects",
        &(),
    );
    let mut bluetooth = app.resource_mut::<BluetoothStatus>();
    bluetooth.pending = Some(call);
    bluetooth.dirty = false;
}

fn publish_state(app: &mut App, state: BluetoothState) {
    if app.resource::<BluetoothStatus>().state == state {
        return;
    }
    // eprintln!(
    //     "[bluetooth] state: {:?} -> {:?}",
    //     app.resource::<BluetoothStatus>().state,
    //     state
    // );
    app.resource_mut::<BluetoothStatus>().state = state;
    let bars: Vec<NodeId> = app.widgets::<StatusBar>().map(|(id, _)| id).collect();
    app.emit(StatusUpdate::Bluetooth(state), bars);
}

fn on_snapshot_reply(app: &mut App, event: &MethodReply) {
    if app.resource::<BluetoothStatus>().pending != Some(event.call) {
        return;
    }
    let objects = event
        .result
        .as_ref()
        .map_err(|e| eprintln!("[bluetooth] GetManagedObjects failed: {e:?}"))
        .ok()
        .and_then(|message| {
            message
                .body::<(ManagedObjects,)>()
                .map_err(|e| {
                    eprintln!("[bluetooth] failed to parse GetManagedObjects reply: {e:?}")
                })
                .ok()
        });
    let (dirty, state) = {
        let mut bluetooth = app.resource_mut::<BluetoothStatus>();
        bluetooth.pending = None;
        if !bluetooth.dirty {
            bluetooth.observed = objects
                .map(|(objects,)| BluetoothObservation::from_managed_objects(objects))
                .unwrap_or_default();
        }
        (bluetooth.dirty, bluetooth.observed.state())
    };
    if dirty {
        request_snapshot(app);
    } else {
        publish_state(app, state);
    }
}

fn on_signal(app: &mut App, event: &SignalReceived) {
    let bluetooth = app.resource::<BluetoothStatus>();
    let owner = event.matches.contains(&bluetooth.owner);
    if owner {
        if let Ok((_, _, new_owner)) = event.msg.body::<(String, String, String)>() {
            let mut bluetooth = app.resource_mut::<BluetoothStatus>();
            bluetooth.pending = None;
            bluetooth.dirty = false;
            bluetooth.observed = BluetoothObservation::default();
            publish_state(app, BluetoothState::Off);
            if !new_owner.is_empty() {
                request_snapshot(app);
            }
        }
    } else if event.matches.contains(&bluetooth.properties)
        || event.matches.contains(&bluetooth.interfaces_added)
        || event.matches.contains(&bluetooth.interfaces_removed)
    {
        if let Some(change) = decode_bluez_change(&event.msg) {
            if app.resource::<BluetoothStatus>().pending.is_some() {
                // The snapshot may predate this signal; request a fresh one once it completes.
                request_snapshot(app);
            } else {
                let (applied, state) = {
                    let mut bluetooth = app.resource_mut::<BluetoothStatus>();
                    let applied = bluetooth.observed.apply_change(change);
                    (applied, bluetooth.observed.state())
                };
                if applied {
                    publish_state(app, state);
                } else {
                    request_snapshot(app);
                }
            }
        }
    }
}

fn on_disconnected(app: &mut App, event: &BusDisconnected) {
    if event.bus == BusId::System {
        eprintln!("[bluetooth] system D-Bus disconnected; resetting state");
        let mut bluetooth = app.resource_mut::<BluetoothStatus>();
        bluetooth.pending = None;
        bluetooth.dirty = false;
        bluetooth.observed = BluetoothObservation::default();
        publish_state(app, BluetoothState::Off);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mecha_wayland::dbus::Value;

    #[test]
    fn snapshot_and_property_signals() {
        let adapter = ObjectPath::new("/org/bluez/hci0").unwrap();
        let device = ObjectPath::new("/org/bluez/hci0/dev_00_11_22_33_44_55").unwrap();
        let objects = ManagedObjects::from([
            (
                adapter.clone(),
                HashMap::from([(
                    ADAPTER.into(),
                    PropMap::from([("Powered".into(), Value::Bool(false))]),
                )]),
            ),
            (
                device.clone(),
                HashMap::from([(
                    DEVICE.into(),
                    PropMap::from([("Connected".into(), Value::Bool(false))]),
                )]),
            ),
        ]);
        let mut observed = BluetoothObservation::from_managed_objects(objects);
        assert_eq!(observed.state(), BluetoothState::Off);
        assert!(observed.connected_devices.is_empty());

        let property = |path: ObjectPath, interface: &str, name: &str, value: bool| {
            Message::signal(path, "org.freedesktop.DBus.Properties", "PropertiesChanged").with_body(
                &(
                    interface,
                    PropMap::from([(name.to_string(), Value::Bool(value))]),
                    Vec::<String>::new(),
                ),
            )
        };
        assert!(observed.apply_change(
            decode_bluez_change(&property(adapter.clone(), ADAPTER, "Powered", true)).unwrap()
        ));
        assert_eq!(observed.state(), BluetoothState::On);
        assert!(observed.apply_change(
            decode_bluez_change(&property(device.clone(), DEVICE, "Connected", true)).unwrap()
        ));
        assert_eq!(observed.state(), BluetoothState::Connected);
        assert_eq!(observed.connected_devices.len(), 1);
        assert!(observed.apply_change(
            decode_bluez_change(&property(device, DEVICE, "Connected", false)).unwrap()
        ));
        assert_eq!(observed.state(), BluetoothState::On);
        assert!(observed.connected_devices.is_empty());
        assert!(decode_bluez_change(&property(adapter, ADAPTER, "Alias", true)).is_none());
    }

    #[test]
    fn object_manager_removal_uses_body_path() {
        let adapter = ObjectPath::new("/org/bluez/hci0").unwrap();
        let device = ObjectPath::new("/org/bluez/hci0/dev_00_11_22_33_44_55").unwrap();
        let mut observed = BluetoothObservation::default();
        observed.adapters.insert(adapter.clone(), true);
        observed.connected_devices.insert(device.clone());

        let removed = |path: ObjectPath, interface: &str| {
            Message::signal("/", OBJECT_MANAGER, "InterfacesRemoved")
                .with_body(&(path, vec![interface.to_string()]))
        };
        let msg = removed(device, DEVICE);
        assert_eq!(msg.path().unwrap().as_str(), "/");
        assert!(observed.apply_change(decode_bluez_change(&msg).unwrap()));
        assert_eq!(observed.state(), BluetoothState::On);

        assert!(observed.apply_change(decode_bluez_change(&removed(adapter, ADAPTER)).unwrap()));
        assert_eq!(observed.state(), BluetoothState::Off);
    }

    #[test]
    fn added_devices_keep_connected_until_last_one_is_removed() {
        let adapter = ObjectPath::new("/org/bluez/hci0").unwrap();
        let first = ObjectPath::new("/org/bluez/hci0/dev_00_11_22_33_44_55").unwrap();
        let second = ObjectPath::new("/org/bluez/hci0/dev_00_11_22_33_44_66").unwrap();
        let mut observed = BluetoothObservation::default();
        observed.adapters.insert(adapter, true);

        for path in [&first, &second] {
            let added = Message::signal("/", OBJECT_MANAGER, "InterfacesAdded").with_body(&(
                path.clone(),
                HashMap::from([(
                    DEVICE.to_string(),
                    PropMap::from([("Connected".into(), Value::Bool(true))]),
                )]),
            ));
            assert!(observed.apply_change(decode_bluez_change(&added).unwrap()));
            assert_eq!(observed.state(), BluetoothState::Connected);
        }
        for (path, expected) in [
            (first, BluetoothState::Connected),
            (second, BluetoothState::On),
        ] {
            let removed = Message::signal("/", OBJECT_MANAGER, "InterfacesRemoved")
                .with_body(&(path, vec![DEVICE.to_string()]));
            assert!(observed.apply_change(decode_bluez_change(&removed).unwrap()));
            assert_eq!(observed.state(), expected);
        }
    }

    #[test]
    fn invalidation_requests_snapshot() {
        let adapter = ObjectPath::new("/org/bluez/hci0").unwrap();
        let mut observed = BluetoothObservation::default();
        observed.adapters.insert(adapter.clone(), true);
        let msg = Message::signal(
            adapter,
            "org.freedesktop.DBus.Properties",
            "PropertiesChanged",
        )
        .with_body(&(ADAPTER, PropMap::new(), vec!["Powered".to_string()]));
        assert!(!observed.apply_change(decode_bluez_change(&msg).unwrap()));
        assert_eq!(observed.state(), BluetoothState::On);
    }
}
