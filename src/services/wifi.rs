//! Read-only NetworkManager Wi-Fi status service.
mod backend;
mod interface;
mod snapshot;
mod types;

#[cfg(test)]
mod tests;

pub use types::{WifiState, WifiStatus};

use std::collections::{HashMap, HashSet};

use mecha_wayland::prelude::*;
use types::{Matches, Snapshot};

/// Install after `RingModule` and `DBusModule::new().system()`.
pub struct WifiModule;
impl Module for WifiModule {
    fn install(self, app: &mut App) {
        let matches = {
            let mut dbus = app.resource_mut::<DBus>();
            let mut bus = dbus.system();
            Matches {
                properties: bus.add_match(interface::properties_changed()),
                devices: bus.add_match(interface::devices_changed()),
                owner: bus.add_match(interface::owner_changed()),
            }
        };
        app.insert_resource(WifiStatus {
            state: WifiState::Disabled,
            pending: HashMap::new(),
            snapshot: Snapshot::default(),
            active_aps: HashSet::new(),
            dirty: false,
            matches,
        });
        app.system(backend::on_snapshot_reply)
            .system(backend::on_signal)
            .system(backend::on_disconnect);
        backend::request_snapshot(app);
    }
}
