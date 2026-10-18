use std::collections::{HashMap, HashSet};

use mecha_wayland::prelude::*;

use super::interface::{ADAPTER, DEVICE};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BluetoothState {
    Connected,
    #[default]
    On,
    Off,
    Disconnected,
}

impl BluetoothState {
    #[inline]
    pub fn is_visible(&self) -> bool {
        !matches!(self, Self::Off)
    }
}

pub(super) struct Matches {
    pub(super) properties: MatchId,
    pub(super) interfaces_added: MatchId,
    pub(super) interfaces_removed: MatchId,
    pub(super) owner: MatchId,
}

/// Latest observed Bluetooth state, derived from BlueZ adapters and devices.
pub struct BluetoothStatus {
    pub state: BluetoothState,
    pub(super) observed: BluetoothObservation,
    pub(super) pending: Option<CallId>,
    pub(super) dirty: bool,
    pub(super) matches: Matches,
}
impl Resource for BluetoothStatus {}

pub(super) type ManagedObjects = HashMap<ObjectPath, HashMap<String, PropMap>>;

#[derive(Default)]
pub(super) struct BluetoothObservation {
    pub(super) adapters: HashMap<ObjectPath, bool>,
    pub(super) connected_devices: HashSet<ObjectPath>,
}

impl BluetoothObservation {
    pub(super) fn from_managed_objects(objects: ManagedObjects) -> Self {
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

    pub(super) fn apply_change(&mut self, change: BluetoothChange) -> bool {
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

    pub(super) fn state(&self) -> BluetoothState {
        if !self.adapters.values().any(|&powered| powered) {
            BluetoothState::Off
        } else if !self.connected_devices.is_empty() {
            BluetoothState::Connected
        } else {
            BluetoothState::On
        }
    }
}

pub(super) enum BluetoothChange {
    Added(ObjectPath, HashMap<String, PropMap>),
    Removed(ObjectPath, Vec<String>),
    Property(ObjectPath, &'static str, Option<bool>),
}
