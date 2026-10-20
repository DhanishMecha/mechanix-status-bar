//! Read-only NetworkManager client on mecha-wayland's system D-Bus connection.
use super::interface::{ACTIVATED, AP, DEVICE, NM, PATH, WIFI_DEVICE, WIRELESS};
use super::types::{AllRequest, Change, Observation, PropertyRequest, Request, Scan, WifiDevice};
use super::{WifiState, WifiStatus};
use crate::{StatusBar, StatusUpdate};
use mecha_wayland::dbus::Value;
use mecha_wayland::prelude::*;

fn get_all(app: &mut App, path: ObjectPath, interface: &'static str, request: AllRequest) {
    let call = app
        .resource_mut::<DBus>()
        .system()
        .object(NM, path)
        .get_all(interface);
    app.resource_mut::<WifiStatus>()
        .pending
        .insert(call, Request::All(request));
}

pub(super) fn get_property(app: &mut App, request: PropertyRequest) {
    if app
        .resource::<WifiStatus>()
        .pending
        .values()
        .any(|pending| matches!(pending, Request::Property(existing) if existing == &request))
    {
        return;
    }
    let (path, interface, name) = request.target();
    let call = app
        .resource_mut::<DBus>()
        .system()
        .object(NM, path)
        .get_property(interface, name);
    app.resource_mut::<WifiStatus>()
        .pending
        .insert(call, Request::Property(request));
}

pub(super) fn request_snapshot(app: &mut App) {
    start_snapshot(app, true);
}

fn start_snapshot(app: &mut App, retry: bool) {
    if let Some(scan) = app.resource_mut::<WifiStatus>().scan.as_mut() {
        scan.dirty = true;
        return;
    }
    app.resource_mut::<WifiStatus>().scan = Some(Scan {
        valid: true,
        retry,
        ..Scan::default()
    });
    get_all(
        app,
        ObjectPath::new(PATH).expect("valid NetworkManager path"),
        NM,
        AllRequest::Manager,
    );
}

fn publish_state(app: &mut App) {
    let Some(state) = app.resource::<WifiStatus>().observed.state() else {
        return;
    };
    if app.resource::<WifiStatus>().state == state {
        return;
    }
    app.resource_mut::<WifiStatus>().state = state;
    let bars: Vec<NodeId> = app.widgets::<StatusBar>().map(|(id, _)| id).collect();
    app.emit(StatusUpdate::Wifi(state), bars);
}

fn reply_props(event: &MethodReply, name: &str) -> Option<PropMap> {
    event
        .result
        .as_ref()
        .map_err(|error| eprintln!("[wifi] {name} failed: {error:?}"))
        .ok()?
        .body::<(PropMap,)>()
        .map_err(|error| eprintln!("[wifi] invalid {name} reply: {error:?}"))
        .ok()
        .map(|(props,)| props)
}

fn property<T: mecha_wayland::dbus::Decode>(props: &PropMap, name: &str) -> Option<T> {
    props.get_as::<T>(name).and_then(Result::ok)
}

pub(super) fn ap_path(path: ObjectPath) -> Option<ObjectPath> {
    (path.as_str() != "/").then_some(path)
}

fn on_get_all(app: &mut App, request: AllRequest, event: &MethodReply) {
    let props = reply_props(event, "GetAll");
    match request {
        AllRequest::Manager => {
            let enabled = props
                .as_ref()
                .and_then(|p| property::<bool>(p, "WirelessEnabled"));
            let hardware = props
                .as_ref()
                .and_then(|p| property::<bool>(p, "WirelessHardwareEnabled"));
            let mut wifi = app.resource_mut::<WifiStatus>();
            let scan = wifi.scan.as_mut().unwrap();
            scan.observed.enabled = enabled;
            scan.observed.hardware_enabled = hardware;
            scan.valid &= enabled.is_some() && hardware.is_some();
            // Discover devices even while the radio is off, so later State signals work.
            let call =
                app.resource_mut::<DBus>()
                    .system()
                    .object(NM, PATH)
                    .call(NM, "GetDevices", &());
            app.resource_mut::<WifiStatus>()
                .pending
                .insert(call, Request::Devices);
        }
        AllRequest::Device(path) => {
            let kind = props
                .as_ref()
                .and_then(|p| property::<u32>(p, "DeviceType"));
            let state = props.as_ref().and_then(|p| property::<u32>(p, "State"));
            let mut wifi = app.resource_mut::<WifiStatus>();
            let scan = wifi.scan.as_mut().unwrap();
            if kind.is_none() {
                scan.valid = false;
            }
            if kind == Some(WIFI_DEVICE) {
                if let Some(state) = state {
                    scan.observed
                        .devices
                        .insert(path.clone(), WifiDevice { state, ap: None });
                    if state == ACTIVATED {
                        get_all(app, path.clone(), WIRELESS, AllRequest::Wireless(path));
                    }
                } else {
                    scan.valid = false;
                }
            }
        }
        AllRequest::Wireless(path) => {
            let ap = props
                .as_ref()
                .and_then(|p| property::<ObjectPath>(p, "ActiveAccessPoint"));
            if ap.is_none() {
                app.resource_mut::<WifiStatus>()
                    .scan
                    .as_mut()
                    .unwrap()
                    .valid = false;
            }
            if let Some(ap) = ap.and_then(ap_path) {
                if let Some(device) = app
                    .resource_mut::<WifiStatus>()
                    .scan
                    .as_mut()
                    .unwrap()
                    .observed
                    .devices
                    .get_mut(&path)
                {
                    device.ap = Some(ap.clone());
                }
                get_all(app, ap.clone(), AP, AllRequest::AccessPoint(ap));
            }
        }
        AllRequest::AccessPoint(path) => {
            if let Some(props) = props {
                if let Some(strength) = property::<u8>(&props, "Strength") {
                    let mut wifi = app.resource_mut::<WifiStatus>();
                    let scan = wifi.scan.as_mut().unwrap();
                    if scan.observed.active(&path) {
                        scan.observed.strengths.insert(path, strength);
                    }
                }
            } else {
                app.resource_mut::<WifiStatus>()
                    .scan
                    .as_mut()
                    .unwrap()
                    .valid = false;
            }
        }
    }
}

fn on_get_devices(app: &mut App, event: &MethodReply) {
    let paths = event
        .result
        .as_ref()
        .ok()
        .and_then(|msg| msg.body::<(Vec<ObjectPath>,)>().ok())
        .map(|(paths,)| paths);
    if let Some(paths) = paths {
        for path in paths {
            get_all(app, path.clone(), DEVICE, AllRequest::Device(path));
        }
    } else {
        eprintln!("[wifi] GetDevices failed");
        app.resource_mut::<WifiStatus>()
            .scan
            .as_mut()
            .unwrap()
            .valid = false;
    }
}

fn on_get(app: &mut App, request: PropertyRequest, event: &MethodReply) {
    let value = event
        .result
        .as_ref()
        .ok()
        .and_then(|msg| msg.body::<(Value,)>().ok())
        .map(|(value,)| value);
    let change = match (&request, value) {
        (PropertyRequest::RadioEnabled, Some(Value::Bool(value))) => {
            Some(Change::RadioEnabled(value))
        }
        (PropertyRequest::HardwareEnabled, Some(Value::Bool(value))) => {
            Some(Change::HardwareEnabled(value))
        }
        (PropertyRequest::DeviceState(path), Some(Value::U32(value))) => {
            Some(Change::DeviceState(path.clone(), value))
        }
        (PropertyRequest::ActiveAccessPoint(path), Some(Value::ObjectPath(value))) => {
            Some(Change::ActiveAccessPoint(path.clone(), ap_path(value)))
        }
        (PropertyRequest::Strength(path), Some(Value::U8(value))) => {
            Some(Change::Strength(path.clone(), value))
        }
        _ => {
            eprintln!("[wifi] failed to read {:?}", request);
            None
        }
    };
    if let Some(change) = change {
        apply_change(app, change);
    }
}

pub(super) fn on_reply(app: &mut App, event: &MethodReply) {
    let Some(request) = app.resource_mut::<WifiStatus>().pending.remove(&event.call) else {
        return;
    };
    match request {
        Request::Devices => on_get_devices(app, event),
        Request::All(request) => on_get_all(app, request, event),
        Request::Property(request) => on_get(app, request, event),
    }
    let wifi = app.resource::<WifiStatus>();
    if wifi.scan.is_none() || wifi.pending.values().any(Request::is_discovery) {
        return;
    }
    let scan = app.resource_mut::<WifiStatus>().scan.take().unwrap();
    finish_snapshot(app, scan);
}

fn finish_snapshot(app: &mut App, scan: Scan) {
    if scan.dirty {
        request_snapshot(app);
        return;
    }
    if !scan.valid {
        if scan.retry {
            eprintln!("[wifi] incomplete NetworkManager discovery; retrying once");
            start_snapshot(app, false);
        } else {
            eprintln!("[wifi] NetworkManager discovery failed again; keeping previous state");
        }
        return;
    }
    let mut observed = scan.observed;
    for change in &scan.changes {
        observed.apply(change);
    }
    // A signal may have activated a device or switched APs during discovery.
    let missing: Vec<_> = observed
        .devices
        .iter()
        .filter(|(_, device)| device.state == ACTIVATED)
        .filter_map(|(path, device)| match &device.ap {
            Some(ap) if !observed.strengths.contains_key(ap) => {
                Some(PropertyRequest::Strength(ap.clone()))
            }
            None => Some(PropertyRequest::ActiveAccessPoint(path.clone())),
            _ => None,
        })
        .collect();
    app.resource_mut::<WifiStatus>().observed = observed;
    publish_state(app);
    for request in missing {
        get_property(app, request);
    }
}

pub(super) fn apply_change(app: &mut App, change: Change) {
    if let Change::DeviceState(path, state) = &change
        && *state != ACTIVATED
    {
        // An AP read started during an earlier activation cannot describe
        // the next connection attempt.
        app.resource_mut::<WifiStatus>().pending.retain(|_, request| {
            !matches!(request, Request::Property(PropertyRequest::ActiveAccessPoint(p)) if p == path)
        });
    }
    let ap = {
        let mut wifi = app.resource_mut::<WifiStatus>();
        if let Some(scan) = wifi.scan.as_mut() {
            scan.changes.push(change.clone());
        }
        wifi.observed.apply(&change)
    };
    if let Some(ap) = ap {
        get_property(app, PropertyRequest::Strength(ap));
    }
    if let Change::DeviceState(path, ACTIVATED) = &change {
        // State may become activated without an accompanying AP change signal.
        if let Some(device) = app.resource::<WifiStatus>().observed.devices.get(path) {
            if let Some(ap) = &device.ap {
                if !app
                    .resource::<WifiStatus>()
                    .observed
                    .strengths
                    .contains_key(ap)
                {
                    get_property(app, PropertyRequest::Strength(ap.clone()));
                }
            } else {
                get_property(app, PropertyRequest::ActiveAccessPoint(path.clone()));
            }
        }
    }
    publish_state(app);
}

pub(super) fn cancel_property(app: &mut App, property: &PropertyRequest) {
    app.resource_mut::<WifiStatus>()
        .pending
        .retain(|_, request| !matches!(request, Request::Property(p) if p == property));
}

pub(super) fn reset(app: &mut App) {
    let mut wifi = app.resource_mut::<WifiStatus>();
    wifi.pending.clear();
    wifi.scan = None;
    wifi.observed = Observation::default();
    if wifi.state != WifiState::Disabled {
        wifi.state = WifiState::Disabled;
        let bars: Vec<NodeId> = app.widgets::<StatusBar>().map(|(id, _)| id).collect();
        app.emit(StatusUpdate::Wifi(WifiState::Disabled), bars);
    }
}

pub(super) fn on_disconnect(app: &mut App, event: &BusDisconnected) {
    if event.bus == BusId::System {
        eprintln!("[wifi] system D-Bus disconnected; resetting state");
        reset(app);
    }
}
