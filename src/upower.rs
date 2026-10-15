use mecha_wayland::prelude::*;
use mechanix_status_bar::{StatusBar, StatusUpdate};

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
        // watch and initial_read are our MatchId and CallId
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

        app.system(on_reply);
    }
}

/// Handle the initial property snapshot; ignore replies to other calls.
fn on_reply(app: &mut App, e: &MethodReply) {
    if e.call != app.resource::<UpowerBattery>().initial_read {
        return;
    }

    let msg = e.result.as_ref().expect("UPower: initial GetAll failed");
    let (properties,) = msg
        .body::<(PropMap,)>()
        .expect("UPower: invalid GetAll reply");

    // TODO: main.rs discards the NodeID of the main status bar, store it so we
    // can send events easily
    let bars: Vec<NodeId> = app.widgets::<StatusBar>().map(|(id, _)| id).collect();
    // check if Upower.Device Type == 2 and IsPresent == true
    let percentage = battery_valid(&properties).then(|| battery_percentage(&properties));
    app.emit(StatusUpdate::Battery(percentage), bars);
}

// A Rust man's C enum
mod upower_types {
    pub const BATTERY: u32 = 2;
}
fn battery_valid(properties: &PropMap) -> bool {
    let is_present = properties
        .get_as::<bool>("IsPresent")
        .expect("UPower: missing IsPresent")
        .expect("UPower: invalid IsPresent");

    let device_type = properties
        .get_as::<u32>("Type")
        .expect("UPower: missing Type")
        .expect("UPower: invalid Type");
    is_present && device_type == upower_types::BATTERY
}

/// Check battery percentage from a message. Assume battery is valid and
//percentage field exists
fn battery_percentage(properties: &PropMap) -> u8 {
    let percentage = properties
        .get_as::<f64>("Percentage")
        .expect("UPower: missing Percentage")
        .expect("UPower: invalid Percentage");
    assert!(percentage.is_finite(), "UPower: non-finite Percentage");
    percentage.clamp(0.0, 100.0).round() as u8
}
