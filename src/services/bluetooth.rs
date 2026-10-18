//! Read-only BlueZ status service.
mod backend;
mod interface;
mod types;

#[cfg(test)]
mod tests;

pub use types::{BluetoothState, BluetoothStatus};

use mecha_wayland::prelude::*;
use types::{BluetoothObservation, Matches};

pub struct BluetoothModule;
impl Module for BluetoothModule {
    fn install(self, app: &mut App) {
        let matches = {
            let mut dbus = app.resource_mut::<DBus>();
            let mut bus = dbus.system();
            Matches {
                properties: bus.add_match(interface::properties_changed()),
                interfaces_added: bus.add_match(interface::interfaces_added()),
                interfaces_removed: bus.add_match(interface::interfaces_removed()),
                owner: bus.add_match(interface::owner_changed()),
            }
        };
        app.insert_resource(BluetoothStatus {
            state: BluetoothState::Off,
            observed: BluetoothObservation::default(),
            pending: None,
            dirty: false,
            matches,
        });
        app.system(backend::on_snapshot_reply)
            .system(backend::on_signal)
            .system(backend::on_disconnect);
        backend::request_snapshot(app);
    }
}
