use super::BluetoothState;
use super::backend::decode_change;
use super::interface::{ADAPTER, DEVICE, OBJECT_MANAGER};
use super::types::{BluetoothObservation, ManagedObjects};
use mecha_wayland::dbus::Value;
use mecha_wayland::prelude::*;
use std::collections::HashMap;

#[test]
fn snapshot_and_property_signals() {
    let adapter = ObjectPath::new("/org/bluez/hci0").unwrap();
    let device = ObjectPath::new("/org/bluez/hci0/dev_00_11_22_33_44_55").unwrap();
    let objects = ManagedObjects::from([
        (
            adapter.clone(),
            HashMap::from([(
                ADAPTER.into(),
                PropMap::from([("Powered".into(), Value::Bool(false))]),
            )]),
        ),
        (
            device.clone(),
            HashMap::from([(
                DEVICE.into(),
                PropMap::from([("Connected".into(), Value::Bool(false))]),
            )]),
        ),
    ]);
    let mut observed = BluetoothObservation::from_managed_objects(objects);
    assert_eq!(observed.state(), BluetoothState::Off);
    assert!(observed.connected_devices.is_empty());

    let property = |path: ObjectPath, interface: &str, name: &str, value: bool| {
        Message::signal(path, "org.freedesktop.DBus.Properties", "PropertiesChanged").with_body(&(
            interface,
            PropMap::from([(name.to_string(), Value::Bool(value))]),
            Vec::<String>::new(),
        ))
    };
    assert!(observed.apply_change(
        decode_change(&property(adapter.clone(), ADAPTER, "Powered", true)).unwrap()
    ));
    assert_eq!(observed.state(), BluetoothState::On);
    assert!(observed.apply_change(
        decode_change(&property(device.clone(), DEVICE, "Connected", true)).unwrap()
    ));
    assert_eq!(observed.state(), BluetoothState::Connected);
    assert_eq!(observed.connected_devices.len(), 1);
    assert!(
        observed
            .apply_change(decode_change(&property(device, DEVICE, "Connected", false)).unwrap())
    );
    assert_eq!(observed.state(), BluetoothState::On);
    assert!(observed.connected_devices.is_empty());
    assert!(decode_change(&property(adapter, ADAPTER, "Alias", true)).is_none());
}

#[test]
fn object_manager_removal_uses_body_path() {
    let adapter = ObjectPath::new("/org/bluez/hci0").unwrap();
    let device = ObjectPath::new("/org/bluez/hci0/dev_00_11_22_33_44_55").unwrap();
    let mut observed = BluetoothObservation::default();
    observed.adapters.insert(adapter.clone(), true);
    observed.connected_devices.insert(device.clone());

    let removed = |path: ObjectPath, interface: &str| {
        Message::signal("/", OBJECT_MANAGER, "InterfacesRemoved")
            .with_body(&(path, vec![interface.to_string()]))
    };
    let msg = removed(device, DEVICE);
    assert_eq!(msg.path().unwrap().as_str(), "/");
    assert!(observed.apply_change(decode_change(&msg).unwrap()));
    assert_eq!(observed.state(), BluetoothState::On);

    assert!(observed.apply_change(decode_change(&removed(adapter, ADAPTER)).unwrap()));
    assert_eq!(observed.state(), BluetoothState::Off);
}

#[test]
fn added_devices_keep_connected_until_last_one_is_removed() {
    let adapter = ObjectPath::new("/org/bluez/hci0").unwrap();
    let first = ObjectPath::new("/org/bluez/hci0/dev_00_11_22_33_44_55").unwrap();
    let second = ObjectPath::new("/org/bluez/hci0/dev_00_11_22_33_44_66").unwrap();
    let mut observed = BluetoothObservation::default();
    observed.adapters.insert(adapter, true);

    for path in [&first, &second] {
        let added = Message::signal("/", OBJECT_MANAGER, "InterfacesAdded").with_body(&(
            path.clone(),
            HashMap::from([(
                DEVICE.to_string(),
                PropMap::from([("Connected".into(), Value::Bool(true))]),
            )]),
        ));
        assert!(observed.apply_change(decode_change(&added).unwrap()));
        assert_eq!(observed.state(), BluetoothState::Connected);
    }
    for (path, expected) in [
        (first, BluetoothState::Connected),
        (second, BluetoothState::On),
    ] {
        let removed = Message::signal("/", OBJECT_MANAGER, "InterfacesRemoved")
            .with_body(&(path, vec![DEVICE.to_string()]));
        assert!(observed.apply_change(decode_change(&removed).unwrap()));
        assert_eq!(observed.state(), expected);
    }
}

#[test]
fn invalidation_requests_snapshot() {
    let adapter = ObjectPath::new("/org/bluez/hci0").unwrap();
    let mut observed = BluetoothObservation::default();
    observed.adapters.insert(adapter.clone(), true);
    let msg = Message::signal(
        adapter,
        "org.freedesktop.DBus.Properties",
        "PropertiesChanged",
    )
    .with_body(&(ADAPTER, PropMap::new(), vec!["Powered".to_string()]));
    assert!(!observed.apply_change(decode_change(&msg).unwrap()));
    assert_eq!(observed.state(), BluetoothState::On);
}
