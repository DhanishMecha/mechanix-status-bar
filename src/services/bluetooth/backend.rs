//! Read-only BlueZ client on mecha-wayland's system D-Bus connection.
use std::collections::HashMap;

use super::BluetoothStatus;
use super::interface::{ADAPTER, BLUEZ, DEVICE, OBJECT_MANAGER};
use super::types::{BluetoothChange, BluetoothObservation, ManagedObjects};
use crate::{BluetoothState, StatusBar, StatusUpdate};
use mecha_wayland::prelude::*;

pub(super) fn decode_change(msg: &Message) -> Option<BluetoothChange> {
    match msg.member()?.as_str() {
        "InterfacesAdded" => {
            let (path, interfaces) = msg.body::<(ObjectPath, HashMap<String, PropMap>)>().ok()?;
            (interfaces.contains_key(ADAPTER) || interfaces.contains_key(DEVICE))
                .then_some(BluetoothChange::Added(path, interfaces))
        }
        "InterfacesRemoved" => {
            let (path, interfaces) = msg.body::<(ObjectPath, Vec<String>)>().ok()?;
            interfaces
                .iter()
                .any(|name| name == ADAPTER || name == DEVICE)
                .then_some(BluetoothChange::Removed(path, interfaces))
        }
        "PropertiesChanged" => {
            let (interface, props, invalidated) = PropertiesChanged::parse(msg).ok()?;
            let (interface, key) = match interface.as_str() {
                ADAPTER => (ADAPTER, "Powered"),
                DEVICE => (DEVICE, "Connected"),
                _ => return None,
            };
            if !props.contains_key(key) && !invalidated.iter().any(|name| name == key) {
                return None;
            }
            let value = if invalidated.iter().any(|name| name == key) {
                None
            } else {
                props.get_as::<bool>(key).and_then(Result::ok)
            };
            Some(BluetoothChange::Property(
                msg.path()?.clone(),
                interface,
                value,
            ))
        }
        _ => None,
    }
}

pub(super) fn request_snapshot(app: &mut App) {
    if app.resource::<BluetoothStatus>().pending.is_some() {
        app.resource_mut::<BluetoothStatus>().dirty = true;
        return;
    }
    let call = app.resource_mut::<DBus>().system().object(BLUEZ, "/").call(
        OBJECT_MANAGER,
        "GetManagedObjects",
        &(),
    );
    let mut bluetooth = app.resource_mut::<BluetoothStatus>();
    bluetooth.pending = Some(call);
    bluetooth.dirty = false;
}

fn publish_state(app: &mut App, state: BluetoothState) {
    if app.resource::<BluetoothStatus>().state == state {
        return;
    }
    // eprintln!(
    //     "[bluetooth] state: {:?} -> {:?}",
    //     app.resource::<BluetoothStatus>().state,
    //     state
    // );
    app.resource_mut::<BluetoothStatus>().state = state;
    let bars: Vec<NodeId> = app.widgets::<StatusBar>().map(|(id, _)| id).collect();
    app.emit(StatusUpdate::Bluetooth(state), bars);
}

pub(super) fn on_snapshot_reply(app: &mut App, event: &MethodReply) {
    if app.resource::<BluetoothStatus>().pending != Some(event.call) {
        return;
    }
    let objects = event
        .result
        .as_ref()
        .map_err(|e| eprintln!("[bluetooth] GetManagedObjects failed: {e:?}"))
        .ok()
        .and_then(|message| {
            message
                .body::<(ManagedObjects,)>()
                .map_err(|e| {
                    eprintln!("[bluetooth] failed to parse GetManagedObjects reply: {e:?}")
                })
                .ok()
        });
    let (dirty, state) = {
        let mut bluetooth = app.resource_mut::<BluetoothStatus>();
        bluetooth.pending = None;
        if !bluetooth.dirty {
            bluetooth.observed = objects
                .map(|(objects,)| BluetoothObservation::from_managed_objects(objects))
                .unwrap_or_default();
        }
        (bluetooth.dirty, bluetooth.observed.state())
    };
    if dirty {
        request_snapshot(app);
    } else {
        publish_state(app, state);
    }
}

pub(super) fn on_signal(app: &mut App, event: &SignalReceived) {
    let bluetooth = app.resource::<BluetoothStatus>();
    let owner = event.matches.contains(&bluetooth.matches.owner);
    if owner {
        if let Ok((_, _, new_owner)) = event.msg.body::<(String, String, String)>() {
            let mut bluetooth = app.resource_mut::<BluetoothStatus>();
            bluetooth.pending = None;
            bluetooth.dirty = false;
            bluetooth.observed = BluetoothObservation::default();
            publish_state(app, BluetoothState::Off);
            if !new_owner.is_empty() {
                request_snapshot(app);
            }
        }
    } else if event.matches.contains(&bluetooth.matches.properties)
        || event.matches.contains(&bluetooth.matches.interfaces_added)
        || event
            .matches
            .contains(&bluetooth.matches.interfaces_removed)
    {
        if let Some(change) = decode_change(&event.msg) {
            if app.resource::<BluetoothStatus>().pending.is_some() {
                // The snapshot may predate this signal; request a fresh one once it completes.
                request_snapshot(app);
            } else {
                let (applied, state) = {
                    let mut bluetooth = app.resource_mut::<BluetoothStatus>();
                    let applied = bluetooth.observed.apply_change(change);
                    (applied, bluetooth.observed.state())
                };
                if applied {
                    publish_state(app, state);
                } else {
                    request_snapshot(app);
                }
            }
        }
    }
}

pub(super) fn on_disconnect(app: &mut App, event: &BusDisconnected) {
    if event.bus == BusId::System {
        eprintln!("[bluetooth] system D-Bus disconnected; resetting state");
        let mut bluetooth = app.resource_mut::<BluetoothStatus>();
        bluetooth.pending = None;
        bluetooth.dirty = false;
        bluetooth.observed = BluetoothObservation::default();
        publish_state(app, BluetoothState::Off);
    }
}
