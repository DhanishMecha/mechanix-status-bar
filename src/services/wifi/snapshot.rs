//! Accumulate NetworkManager observations and derive the Wi-Fi state.
use mecha_wayland::prelude::*;

use super::interface::{ACTIVATED, WIFI_DEVICE};
use super::types::{Snapshot, WifiState};

impl Snapshot {
    pub(super) fn state(&self) -> Option<WifiState> {
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

    pub(super) fn record_strength(&mut self, strength: u8) {
        self.strength = Some(
            self.strength
                .map_or(strength, |current| current.max(strength)),
        );
    }

    pub(super) fn manager(&mut self, props: Option<&PropMap>) -> bool {
        self.radio = props.and_then(|props| {
            Some(
                prop::<bool>(props, "WirelessEnabled")?
                    && prop::<bool>(props, "WirelessHardwareEnabled")?,
            )
        });
        self.valid &= self.radio.is_some();
        self.radio == Some(true)
    }

    pub(super) fn device(&mut self, props: Option<&PropMap>) -> bool {
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

    pub(super) fn wireless(&mut self, props: Option<&PropMap>) -> Option<ObjectPath> {
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

    pub(super) fn access_point(&mut self, path: &ObjectPath, props: Option<&PropMap>) {
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

fn prop<T: mecha_wayland::dbus::Decode + mecha_wayland::dbus::Type>(
    props: &PropMap,
    key: &str,
) -> Option<T> {
    props.get_as::<T>(key).and_then(Result::ok)
}
