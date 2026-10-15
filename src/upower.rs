use mecha_wayland::prelude::*;

pub struct UpowerModule;

struct UpowerBattery {
    property_changed: MatchId,
    initial_read: CallId, // TODO: do something with this so it can be freed / discarded later ?
}
impl Resource for UpowerBattery {}

const UPOWER: &str = "org.freedesktop.UPower";
const DISPLAY_DEVICE: &str = "/org/freedesktop/UPower/devices/DisplayDevice";
const DEVICE_INTERFACE: &str = "org.freedesktop.UPower.Device";

impl Module for UpowerModule {
    fn install(self, app: &mut App) {
        // watch and initial_state are our MatchID and CallID
        let (watch, initial_read) = {
            let mut dbus = app.resource_mut::<DBus>();
            let mut bus = dbus.system();

            let watch = bus.watch_properties(UPOWER, DISPLAY_DEVICE, DEVICE_INTERFACE);

            let initial_read = bus.object(UPOWER, DISPLAY_DEVICE).get_all(DEVICE_INTERFACE);

            (watch, initial_read)
        };

        app.insert_resource(UpowerBattery {
            initial_read,
            property_changed: watch,
        });
    }
}
