use std::collections::HashSet;

use super::WifiState;
use super::backend::relevant_change;
use super::interface::{ACTIVATED, AP, DEVICE, NM, PATH, WIFI_DEVICE, WIRELESS};
use super::types::Snapshot;
use mecha_wayland::dbus::Value;
use mecha_wayland::prelude::*;

fn props(entries: &[(&str, Value)]) -> PropMap {
    entries
        .iter()
        .map(|(key, value)| (key.to_string(), value.clone()))
        .collect()
}

#[test]
fn radio_and_connection_states() {
    let mut snapshot = Snapshot {
        valid: true,
        radio: Some(false),
        connected: true,
        ..Snapshot::default()
    };
    assert_eq!(snapshot.state(), Some(WifiState::Disabled));
    snapshot.radio = Some(true);
    snapshot.connected = false;
    assert_eq!(snapshot.state(), Some(WifiState::Disconnected));
    snapshot.connected = true;
    assert_eq!(snapshot.state(), Some(WifiState::None));
    for (strength, expected) in [
        (24, WifiState::None),
        (25, WifiState::Low),
        (49, WifiState::Low),
        (50, WifiState::Medium),
        (74, WifiState::Medium),
        (75, WifiState::High),
    ] {
        snapshot.strength = Some(strength);
        assert_eq!(snapshot.state(), Some(expected));
    }
    snapshot.strength = None;
    snapshot.record_strength(20);
    snapshot.record_strength(75);
    snapshot.record_strength(30);
    assert_eq!(snapshot.strength, Some(75));
}

#[test]
fn incomplete_scan_does_not_publish_disconnected() {
    let mut snapshot = Snapshot {
        valid: true,
        ..Snapshot::default()
    };
    assert_eq!(snapshot.state(), None);
    snapshot.radio = Some(true);
    snapshot.valid = false;
    assert_eq!(snapshot.state(), None);
}

#[test]
fn manager_requires_a_complete_radio_read() {
    for (enabled, hardware, expected) in [
        (true, true, Some(WifiState::Disconnected)),
        (false, true, Some(WifiState::Disabled)),
        (true, false, Some(WifiState::Disabled)),
        (false, false, Some(WifiState::Disabled)),
    ] {
        let mut snapshot = Snapshot {
            valid: true,
            ..Snapshot::default()
        };
        let radio = props(&[
            ("WirelessEnabled", Value::Bool(enabled)),
            ("WirelessHardwareEnabled", Value::Bool(hardware)),
        ]);
        assert_eq!(snapshot.manager(Some(&radio)), enabled && hardware);
        assert_eq!(snapshot.state(), expected);
    }
    for radio in [
        None,
        Some(props(&[("WirelessEnabled", Value::Bool(true))])),
        Some(props(&[("WirelessEnabled", Value::U32(1))])),
    ] {
        let mut snapshot = Snapshot {
            valid: true,
            ..Snapshot::default()
        };
        assert!(!snapshot.manager(radio.as_ref()));
        assert_eq!(snapshot.state(), None);
    }
}

#[test]
fn devices_only_follow_activated_wifi_and_preserve_unknown_scans() {
    let mut snapshot = Snapshot {
        radio: Some(true),
        valid: true,
        ..Snapshot::default()
    };
    assert!(!snapshot.device(Some(&props(&[
        ("DeviceType", Value::U32(1)),
        ("State", Value::U32(ACTIVATED)),
    ]))));
    assert!(!snapshot.device(Some(&props(&[
        ("DeviceType", Value::U32(WIFI_DEVICE)),
        ("State", Value::U32(30)),
    ]))));
    assert_eq!(snapshot.state(), Some(WifiState::Disconnected));
    assert!(snapshot.device(Some(&props(&[
        ("DeviceType", Value::U32(WIFI_DEVICE)),
        ("State", Value::U32(ACTIVATED)),
    ]))));
    assert_eq!(snapshot.state(), Some(WifiState::None));
    assert!(!snapshot.device(None));
    assert_eq!(snapshot.state(), None);

    for missing in [
        props(&[("State", Value::U32(ACTIVATED))]),
        props(&[("DeviceType", Value::U32(WIFI_DEVICE))]),
        props(&[("DeviceType", Value::Bool(true))]),
    ] {
        let mut snapshot = Snapshot {
            valid: true,
            radio: Some(true),
            ..Snapshot::default()
        };
        assert!(!snapshot.device(Some(&missing)));
        assert_eq!(snapshot.state(), None);
    }
}

#[test]
fn wireless_ap_reads_deduplicate_and_choose_strongest() {
    let mut snapshot = Snapshot {
        valid: true,
        radio: Some(true),
        connected: true,
        ..Snapshot::default()
    };
    let first = ObjectPath::new("/org/freedesktop/NetworkManager/AccessPoint/1").unwrap();
    let second = ObjectPath::new("/org/freedesktop/NetworkManager/AccessPoint/2").unwrap();
    let no_ap = props(&[(
        "ActiveAccessPoint",
        Value::ObjectPath(ObjectPath::new("/").unwrap()),
    )]);
    assert_eq!(snapshot.wireless(Some(&no_ap)), None);
    assert_eq!(snapshot.state(), Some(WifiState::None));
    let first_props = props(&[("ActiveAccessPoint", Value::ObjectPath(first.clone()))]);
    assert_eq!(snapshot.wireless(Some(&first_props)), Some(first.clone()));
    assert_eq!(snapshot.wireless(Some(&first_props)), None);
    snapshot.access_point(&first, Some(&props(&[("Strength", Value::U8(35))])));
    assert_eq!(snapshot.state(), Some(WifiState::Low));
    let second_props = props(&[("ActiveAccessPoint", Value::ObjectPath(second.clone()))]);
    assert_eq!(snapshot.wireless(Some(&second_props)), Some(second.clone()));
    snapshot.access_point(&second, Some(&props(&[("Strength", Value::U8(80))])));
    assert_eq!(snapshot.state(), Some(WifiState::High));
    snapshot.access_point(&first, Some(&props(&[("Strength", Value::U8(15))])));
    assert_eq!(snapshot.state(), Some(WifiState::High));
    let unrelated = ObjectPath::new("/org/freedesktop/NetworkManager/AccessPoint/3").unwrap();
    snapshot.access_point(&unrelated, Some(&props(&[("Strength", Value::U8(100))])));
    assert_eq!(snapshot.strength, Some(80));
}

#[test]
fn missing_ap_properties_do_not_forge_a_connection_or_strength() {
    for wireless in [None, Some(PropMap::new())] {
        let mut snapshot = Snapshot {
            valid: true,
            radio: Some(true),
            connected: true,
            ..Snapshot::default()
        };
        assert_eq!(snapshot.wireless(wireless.as_ref()), None);
        assert_eq!(snapshot.state(), None);
    }
    let ap = ObjectPath::new("/org/freedesktop/NetworkManager/AccessPoint/1").unwrap();
    let mut snapshot = Snapshot {
        valid: true,
        radio: Some(true),
        connected: true,
        ..Snapshot::default()
    };
    snapshot.active_aps.insert(ap.clone());
    snapshot.access_point(&ap, Some(&PropMap::new()));
    assert_eq!(snapshot.state(), Some(WifiState::None));
    snapshot.access_point(&ap, None);
    assert_eq!(snapshot.state(), None);
}

#[test]
fn only_relevant_property_signals_trigger_refresh() {
    let manager = ObjectPath::new(PATH).unwrap();
    let ap = ObjectPath::new("/org/freedesktop/NetworkManager/AccessPoint/1").unwrap();
    let other = ObjectPath::new("/org/freedesktop/NetworkManager/AccessPoint/2").unwrap();
    let empty = HashSet::new();
    let active = HashSet::from([ap.clone()]);
    let changed = props(&[("Strength", Value::U8(50))]);
    assert!(relevant_change(
        AP,
        Some(&ap),
        &changed,
        &[],
        &active,
        &empty
    ));
    assert!(relevant_change(
        AP,
        Some(&ap),
        &changed,
        &[],
        &empty,
        &active
    ));
    assert!(!relevant_change(
        AP,
        Some(&other),
        &changed,
        &[],
        &active,
        &empty
    ));
    assert!(!relevant_change(AP, None, &changed, &[], &active, &empty));
    assert!(relevant_change(
        WIRELESS,
        Some(&other),
        &PropMap::new(),
        &["ActiveAccessPoint".into()],
        &empty,
        &empty,
    ));
    assert!(relevant_change(
        DEVICE,
        Some(&other),
        &props(&[("DeviceType", Value::U32(WIFI_DEVICE))]),
        &[],
        &empty,
        &empty,
    ));
    assert!(relevant_change(
        NM,
        Some(&manager),
        &props(&[("WirelessEnabled", Value::Bool(false))]),
        &[],
        &empty,
        &empty,
    ));
    assert!(!relevant_change(
        NM,
        Some(&other),
        &props(&[("WirelessEnabled", Value::Bool(false))]),
        &[],
        &empty,
        &empty,
    ));
    assert!(!relevant_change(
        DEVICE,
        Some(&other),
        &props(&[("Ip4Config", Value::U32(1))]),
        &[],
        &empty,
        &empty,
    ));

    let signal = Message::signal(
        ap.clone(),
        "org.freedesktop.DBus.Properties",
        "PropertiesChanged",
    )
    .with_body(&(AP, PropMap::new(), vec!["Strength".to_string()]));
    let (interface, changed, invalidated) = PropertiesChanged::parse(&signal).unwrap();
    assert!(relevant_change(
        interface.as_str(),
        signal.path(),
        &changed,
        &invalidated,
        &active,
        &empty,
    ));
}
