use std::collections::HashMap;

use mecha_wayland::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WifiState {
    #[default]
    High,
    Medium,
    Low,
    None,
    Disabled,
    Disconnected,
}

impl WifiState {
    #[inline]
    pub fn is_visible(&self) -> bool {
        !matches!(self, Self::Disabled)
    }
}

pub(super) struct Matches {
    pub(super) properties: MatchId,
    pub(super) devices: MatchId,
    pub(super) owner: MatchId,
}

/// Latest observations from NetworkManager, updated by signals after discovery.
pub struct WifiStatus {
    pub state: WifiState,
    pub(super) observed: Observation,
    pub(super) scan: Option<Scan>,
    pub(super) pending: HashMap<CallId, Request>,
    pub(super) matches: Matches,
}
impl Resource for WifiStatus {}

#[derive(Default)]
pub(super) struct Scan {
    pub(super) observed: Observation,
    pub(super) valid: bool,
    pub(super) dirty: bool,
    pub(super) retry: bool,
    pub(super) changes: Vec<Change>,
}

pub(super) enum Request {
    Devices,
    All(AllRequest),
    Property(PropertyRequest),
}

impl Request {
    pub(super) fn is_discovery(&self) -> bool {
        matches!(self, Self::Devices | Self::All(_))
    }
}

pub(super) enum AllRequest {
    Manager,
    Device(ObjectPath),
    Wireless(ObjectPath),
    AccessPoint(ObjectPath),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum PropertyRequest {
    RadioEnabled,
    HardwareEnabled,
    DeviceState(ObjectPath),
    ActiveAccessPoint(ObjectPath),
    Strength(ObjectPath),
}

impl PropertyRequest {
    pub(super) fn target(&self) -> (ObjectPath, &'static str, &'static str) {
        use super::interface::{AP, DEVICE, NM, PATH, WIRELESS};
        match self {
            Self::RadioEnabled => (ObjectPath::new(PATH).unwrap(), NM, "WirelessEnabled"),
            Self::HardwareEnabled => (
                ObjectPath::new(PATH).unwrap(),
                NM,
                "WirelessHardwareEnabled",
            ),
            Self::DeviceState(path) => (path.clone(), DEVICE, "State"),
            Self::ActiveAccessPoint(path) => (path.clone(), WIRELESS, "ActiveAccessPoint"),
            Self::Strength(path) => (path.clone(), AP, "Strength"),
        }
    }
}

#[derive(Clone, Debug)]
pub(super) enum Change {
    RadioEnabled(bool),
    HardwareEnabled(bool),
    DeviceState(ObjectPath, u32),
    ActiveAccessPoint(ObjectPath, Option<ObjectPath>),
    Strength(ObjectPath, u8),
}

#[derive(Default)]
pub(super) struct Observation {
    pub(super) enabled: Option<bool>,
    pub(super) hardware_enabled: Option<bool>,
    pub(super) devices: HashMap<ObjectPath, WifiDevice>,
    pub(super) strengths: HashMap<ObjectPath, u8>,
}

pub(super) struct WifiDevice {
    pub(super) state: u32,
    pub(super) ap: Option<ObjectPath>,
}
