use std::collections::{HashMap, HashSet};

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

/// Latest observed Wi-Fi state, derived from NetworkManager's radio and devices.
pub struct WifiStatus {
    pub state: WifiState,
    pub(super) pending: HashMap<CallId, Request>,
    pub(super) snapshot: Snapshot,
    pub(super) active_aps: HashSet<ObjectPath>,
    pub(super) dirty: bool,
    pub(super) matches: Matches,
}
impl Resource for WifiStatus {}

pub(super) enum Request {
    Manager,
    Devices,
    Device(ObjectPath),
    Wireless,
    AccessPoint(ObjectPath),
}

#[derive(Default)]
pub(super) struct Snapshot {
    pub(super) radio: Option<bool>,
    pub(super) connected: bool,
    pub(super) strength: Option<u8>,
    pub(super) active_aps: HashSet<ObjectPath>,
    pub(super) valid: bool,
}
