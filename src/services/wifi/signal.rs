//! Decode NetworkManager notifications into cache updates or targeted reads.
use super::WifiStatus;
use super::backend::{
    ap_path, apply_change, cancel_property, get_property, request_snapshot, reset,
};
use super::interface::{AP, DEVICE, NM, PATH, WIRELESS};
use super::types::{Change, PropertyRequest};
use mecha_wayland::dbus::Value;
use mecha_wayland::prelude::*;

enum Action {
    Update(PropertyRequest, Change),
    Read(PropertyRequest),
}

fn decode(msg: &Message, known_device: bool, active_ap: bool) -> Vec<Action> {
    let Ok((interface, changed, invalidated)) = PropertiesChanged::parse(msg) else {
        return Vec::new();
    };
    let Some(path) = msg.path() else {
        return Vec::new();
    };
    let candidates: [Option<PropertyRequest>; 2] = match interface.as_str() {
        NM if path.as_str() == PATH => [
            Some(PropertyRequest::RadioEnabled),
            Some(PropertyRequest::HardwareEnabled),
        ],
        DEVICE if known_device => [Some(PropertyRequest::DeviceState(path.clone())), None],
        WIRELESS if known_device => [Some(PropertyRequest::ActiveAccessPoint(path.clone())), None],
        AP if active_ap => [Some(PropertyRequest::Strength(path.clone())), None],
        _ => return Vec::new(),
    };
    candidates
        .into_iter()
        .flatten()
        .filter_map(|request| {
            let (_, _, name) = request.target();
            // An invalidated property has no authoritative value in this signal.
            if invalidated.iter().any(|key| key == name) {
                return Some(Action::Read(request));
            }
            let value = changed.get(name)?;
            let update = match (&request, value) {
                (PropertyRequest::RadioEnabled, Value::Bool(v)) => Change::RadioEnabled(*v),
                (PropertyRequest::HardwareEnabled, Value::Bool(v)) => Change::HardwareEnabled(*v),
                (PropertyRequest::DeviceState(p), Value::U32(v)) => {
                    Change::DeviceState(p.clone(), *v)
                }
                (PropertyRequest::ActiveAccessPoint(p), Value::ObjectPath(v)) => {
                    Change::ActiveAccessPoint(p.clone(), ap_path(v.clone()))
                }
                (PropertyRequest::Strength(p), Value::U8(v)) => Change::Strength(p.clone(), *v),
                _ => return Some(Action::Read(request)),
            };
            Some(Action::Update(request, update))
        })
        .collect()
}

pub(super) fn on_signal(app: &mut App, event: &SignalReceived) {
    if event.bus != BusId::System {
        return;
    }
    let wifi = app.resource::<WifiStatus>();
    if event.matches.contains(&wifi.matches.owner) {
        if let Ok((_, _, new_owner)) = event.msg.body::<(String, String, String)>() {
            reset(app);
            if !new_owner.is_empty() {
                request_snapshot(app);
            }
        }
    } else if event.matches.contains(&wifi.matches.devices) {
        if matches!(
            event.msg.member().map(|m| m.as_str()),
            Some("DeviceAdded" | "DeviceRemoved")
        ) {
            request_snapshot(app);
        }
    } else if event.matches.contains(&wifi.matches.properties) {
        let Some(path) = event.msg.path() else {
            return;
        };
        // Before discovery completes, queue even unknown paths: the device or AP
        // may not have appeared in a GetAll reply yet.
        let known_device = wifi.scan.is_some() || wifi.observed.devices.contains_key(path);
        let active_ap = wifi.scan.is_some() || wifi.observed.active(path);
        for action in decode(&event.msg, known_device, active_ap) {
            match action {
                Action::Update(property, change) => {
                    cancel_property(app, &property);
                    apply_change(app, change);
                }
                Action::Read(property) => {
                    // Never let an older in-flight Get satisfy a new invalidation.
                    cancel_property(app, &property);
                    get_property(app, property);
                }
            }
        }
    }
}
