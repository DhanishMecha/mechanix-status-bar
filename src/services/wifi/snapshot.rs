//! Derive the icon from the current devices and their active access points.
use mecha_wayland::prelude::*;

use super::interface::ACTIVATED;
use super::types::{Change, Observation, WifiState};

impl Observation {
    pub(super) fn state(&self) -> Option<WifiState> {
        let radio = self.enabled? && self.hardware_enabled?;
        if !radio {
            return Some(WifiState::Disabled);
        }
        let mut connected = false;
        let mut strength = None;
        for device in self
            .devices
            .values()
            .filter(|device| device.state == ACTIVATED)
        {
            connected = true;
            if let Some(value) = device.ap.as_ref().and_then(|ap| self.strengths.get(ap)) {
                strength = Some(strength.map_or(*value, |current: u8| current.max(*value)));
            }
        }
        Some(if !connected {
            WifiState::Disconnected
        } else {
            match strength {
                Some(0..=24) | None => WifiState::None,
                Some(25..=49) => WifiState::Low,
                Some(50..=74) => WifiState::Medium,
                Some(_) => WifiState::High,
            }
        })
    }

    /// Return the new active AP, if it needs a targeted Strength read.
    pub(super) fn apply(&mut self, change: &Change) -> Option<ObjectPath> {
        match change {
            Change::RadioEnabled(value) => self.enabled = Some(*value),
            Change::HardwareEnabled(value) => self.hardware_enabled = Some(*value),
            Change::DeviceState(path, state) => {
                if let Some(device) = self.devices.get_mut(path) {
                    device.state = *state;
                    if *state != ACTIVATED {
                        device.ap = None;
                    }
                }
            }
            Change::ActiveAccessPoint(path, ap) => {
                if let Some(device) = self.devices.get_mut(path) {
                    device.ap = ap.clone();
                }
            }
            Change::Strength(path, strength) => {
                if self.active(path) {
                    self.strengths.insert(path.clone(), *strength);
                }
            }
        }
        // Do not reuse a cached strength after leaving and returning to an AP.
        self.strengths.retain(|path, _| {
            self.devices
                .values()
                .any(|device| device.ap.as_ref() == Some(path))
        });
        if let Change::ActiveAccessPoint(_, Some(ap)) = change
            && self
                .devices
                .values()
                .any(|device| device.state == ACTIVATED && device.ap.as_ref() == Some(ap))
            && !self.strengths.contains_key(ap)
        {
            return Some(ap.clone());
        }
        None
    }

    pub(super) fn active(&self, ap: &ObjectPath) -> bool {
        self.devices
            .values()
            .any(|device| device.ap.as_ref() == Some(ap))
    }
}
