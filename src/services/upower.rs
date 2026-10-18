//! Read-only UPower battery status service.
mod backend;

#[cfg(test)]
mod test;

use backend::{DEVICE_INTERFACE, DISPLAY_DEVICE, UPOWER, UpowerBattery, upower_types};
use mecha_wayland::prelude::*;

/// Install after `RingModule` and `DBusModule::new().system()`.
pub struct UpowerModule;
impl Module for UpowerModule {
    fn install(self, app: &mut App) {
        let (watch, property_read) = {
            let mut dbus = app.resource_mut::<DBus>();
            let mut bus = dbus.system();
            let watch = bus.watch_properties(UPOWER, DISPLAY_DEVICE, DEVICE_INTERFACE);
            let property_read = bus.object(UPOWER, DISPLAY_DEVICE).get_all(DEVICE_INTERFACE);
            (watch, property_read)
        };

        app.insert_resource(UpowerBattery {
            property_read,
            property_changed: watch,
            is_present: false,
            device_type: upower_types::UNKNOWN,
        });

        app.system(backend::on_reply)
            .system(backend::on_property_changed);
    }
}
