use super::interface::{ACTIVATED, AP, DEVICE, NM, PATH, WIFI_DEVICE, WIRELESS};
use super::types::{AllRequest, Change, Observation, PropertyRequest, Request, WifiDevice};
use super::{WifiModule, WifiState, WifiStatus};
use mecha_wayland::dbus::Value;
use mecha_wayland::prelude::*;
use std::collections::VecDeque;
use std::io::{ErrorKind, Read, Write};
use std::os::unix::net::UnixStream;

fn path(index: u8) -> ObjectPath {
    ObjectPath::new(&format!(
        "/org/freedesktop/NetworkManager/AccessPoint/{index}"
    ))
    .unwrap()
}

fn device(index: u8) -> ObjectPath {
    ObjectPath::new(&format!("/org/freedesktop/NetworkManager/Devices/{index}")).unwrap()
}

fn connected() -> Observation {
    let mut observed = Observation {
        enabled: Some(true),
        hardware_enabled: Some(true),
        ..Observation::default()
    };
    observed.devices.insert(
        device(1),
        WifiDevice {
            state: ACTIVATED,
            ap: Some(path(1)),
        },
    );
    observed
}

#[test]
fn radio_and_connection_transitions_are_immediate() {
    let mut observed = connected();
    assert_eq!(observed.state(), Some(WifiState::None));
    for state in [30, 40, 50, 110, 120] {
        observed.apply(&Change::DeviceState(device(1), state));
        assert_eq!(observed.state(), Some(WifiState::Disconnected));
    }
    observed.apply(&Change::DeviceState(device(1), ACTIVATED));
    assert_eq!(observed.state(), Some(WifiState::None));
    observed.apply(&Change::RadioEnabled(false));
    assert_eq!(observed.state(), Some(WifiState::Disabled));
    observed.apply(&Change::RadioEnabled(true));
    observed.apply(&Change::HardwareEnabled(false));
    assert_eq!(observed.state(), Some(WifiState::Disabled));
}

#[test]
fn strength_falls_and_old_ap_is_ignored_after_switch() {
    let mut observed = connected();
    observed.apply(&Change::Strength(path(1), 80));
    assert_eq!(observed.state(), Some(WifiState::High));
    observed.apply(&Change::Strength(path(1), 30));
    assert_eq!(observed.state(), Some(WifiState::Low));
    assert_eq!(
        observed.apply(&Change::ActiveAccessPoint(device(1), Some(path(2)))),
        Some(path(2))
    );
    assert_eq!(observed.state(), Some(WifiState::None));
    observed.apply(&Change::Strength(path(1), 95));
    assert_eq!(observed.state(), Some(WifiState::None));
    observed.apply(&Change::Strength(path(2), 50));
    assert_eq!(observed.state(), Some(WifiState::Medium));
}

#[test]
fn strongest_currently_activated_device_wins() {
    let mut observed = connected();
    observed.devices.insert(
        device(2),
        WifiDevice {
            state: ACTIVATED,
            ap: Some(path(2)),
        },
    );
    observed.apply(&Change::Strength(path(1), 20));
    observed.apply(&Change::Strength(path(2), 80));
    assert_eq!(observed.state(), Some(WifiState::High));
    observed.apply(&Change::DeviceState(device(2), 40));
    assert_eq!(observed.state(), Some(WifiState::None));
    observed.apply(&Change::DeviceState(device(1), 30));
    assert_eq!(observed.state(), Some(WifiState::Disconnected));
}

#[test]
fn incomplete_radio_does_not_guess_a_state() {
    let mut observed = Observation::default();
    assert_eq!(observed.state(), None);
    observed.apply(&Change::RadioEnabled(true));
    assert_eq!(observed.state(), None);
}

fn props(entries: &[(&str, Value)]) -> PropMap {
    entries
        .iter()
        .map(|(name, value)| (name.to_string(), value.clone()))
        .collect()
}

struct Harness {
    app: App,
    peer: UnixStream,
}

impl Harness {
    fn new() -> Self {
        let (client, mut peer) = UnixStream::pair().unwrap();
        peer.write_all(b"OK 0123456789abcdef0123456789abcdef\r\nERROR no fd passing\r\n")
            .unwrap();
        let hello = Message::method_return(1)
            .with_sender("org.freedesktop.DBus")
            .with_destination(":1.1")
            .with_body(&(":1.1",));
        peer.write_all(&hello.encode(1).0).unwrap();
        let mut app = App::new();
        app.add_module(RingModule::default())
            .add_module(DBusModule::new().over(BusId::System, client));
        let mut auth = Vec::new();
        while !auth.ends_with(b"BEGIN\r\n") {
            let mut byte = [0];
            peer.read_exact(&mut byte).unwrap();
            auth.push(byte[0]);
        }
        peer.set_nonblocking(true).unwrap();
        app.add_module(WifiModule);
        Self { app, peer }
    }

    fn call(&self, matches: impl Fn(&Request) -> bool) -> CallId {
        self.app
            .resource::<WifiStatus>()
            .pending
            .iter()
            .find(|(_, request)| matches(request))
            .map(|(id, _)| *id)
            .unwrap()
    }

    fn reply<B: mecha_wayland::dbus::Body>(&mut self, call: CallId, body: &B) {
        self.app.signal(MethodReply {
            call,
            result: Ok(Message::method_return(call.serial).with_body(body)),
        });
        self.app.flush();
    }

    fn change(
        &mut self,
        path: ObjectPath,
        interface: &str,
        changed: PropMap,
        invalidated: &[&str],
    ) {
        let watch = self.app.resource::<WifiStatus>().matches.properties;
        self.app.signal(SignalReceived {
            bus: BusId::System,
            matches: [watch].into_iter().collect(),
            msg: Message::signal(path, "org.freedesktop.DBus.Properties", "PropertiesChanged")
                .with_body(&(
                    interface,
                    changed,
                    invalidated
                        .iter()
                        .map(|v| v.to_string())
                        .collect::<Vec<_>>(),
                )),
        });
        self.app.flush();
    }

    fn initialize(&mut self) {
        let manager = self.call(|r| matches!(r, Request::All(AllRequest::Manager)));
        self.reply(
            manager,
            &(props(&[
                ("WirelessEnabled", Value::Bool(true)),
                ("WirelessHardwareEnabled", Value::Bool(true)),
            ]),),
        );
        let devices = self.call(|r| matches!(r, Request::Devices));
        self.reply(devices, &(vec![device(1)],));
        let wifi = self.call(|r| matches!(r, Request::All(AllRequest::Device(_))));
        self.reply(
            wifi,
            &(props(&[
                ("DeviceType", Value::U32(WIFI_DEVICE)),
                ("State", Value::U32(ACTIVATED)),
            ]),),
        );
        let wireless = self.call(|r| matches!(r, Request::All(AllRequest::Wireless(_))));
        self.reply(
            wireless,
            &(props(&[("ActiveAccessPoint", Value::ObjectPath(path(1)))]),),
        );
        let ap = self.call(|r| matches!(r, Request::All(AllRequest::AccessPoint(_))));
        self.reply(ap, &(props(&[("Strength", Value::U8(80))]),));
        assert_eq!(self.state(), WifiState::High);
    }

    fn state(&self) -> WifiState {
        self.app.resource::<WifiStatus>().state
    }
    fn no_scan(&self) {
        assert!(self.app.resource::<WifiStatus>().scan.is_none());
    }

    fn device_signal(&mut self, member: &'static str) {
        let watch = self.app.resource::<WifiStatus>().matches.devices;
        self.app.signal(SignalReceived {
            bus: BusId::System,
            matches: [watch].into_iter().collect(),
            msg: Message::signal(PATH, NM, member).with_body(&(device(1),)),
        });
        self.app.flush();
    }

    fn requests(&mut self) -> Vec<Message> {
        let barrier = self
            .app
            .resource_mut::<DBus>()
            .system()
            .send(MethodCall::new(
                "org.example.Test",
                "/org/example/Test",
                "org.example.Test",
                "Barrier",
            ));
        let mut bytes = Vec::new();
        let mut messages = Vec::new();
        let mut chunk = [0; 4096];
        for turn in 0..64 {
            let (wake, _) = Message::signal("/org/example/Test", "org.example.Test", "Wake")
                .with_sender(":1.2")
                .encode(turn + 2);
            self.peer.write_all(&wake).unwrap();
            Ring::turn(&mut self.app);
            loop {
                match self.peer.read(&mut chunk) {
                    Ok(0) => panic!("test bus disconnected"),
                    Ok(n) => bytes.extend_from_slice(&chunk[..n]),
                    Err(error) if error.kind() == ErrorKind::WouldBlock => break,
                    Err(error) => panic!("test bus read failed: {error}"),
                }
            }
            while let Some(len) = Message::frame_len(&bytes).unwrap() {
                let message = Message::parse(&bytes[..len], &mut VecDeque::new()).unwrap();
                bytes.drain(..len);
                if message.serial() == barrier.serial {
                    return messages;
                }
                messages.push(message);
            }
        }
        panic!("test D-Bus requests did not drain");
    }
}

#[test]
fn signals_change_state_and_strength_without_discovery_calls() {
    let mut h = Harness::new();
    h.initialize();
    h.change(path(1), AP, props(&[("Strength", Value::U8(30))]), &[]);
    assert_eq!(h.state(), WifiState::Low);
    h.change(device(1), DEVICE, props(&[("State", Value::U32(40))]), &[]);
    assert_eq!(h.state(), WifiState::Disconnected);
    h.change(
        device(1),
        DEVICE,
        props(&[("State", Value::U32(ACTIVATED))]),
        &[],
    );
    assert_eq!(h.state(), WifiState::None);
    h.change(
        ObjectPath::new(PATH).unwrap(),
        NM,
        props(&[("WirelessEnabled", Value::Bool(false))]),
        &[],
    );
    assert_eq!(h.state(), WifiState::Disabled);
    h.no_scan();
    assert!(
        h.app
            .resource::<WifiStatus>()
            .pending
            .values()
            .all(|r| matches!(r, Request::Property(_)))
    );
}

#[test]
fn ap_switch_reads_only_new_strength_and_ignores_stale_reply() {
    let mut h = Harness::new();
    h.initialize();
    h.change(
        device(1),
        WIRELESS,
        props(&[("ActiveAccessPoint", Value::ObjectPath(path(2)))]),
        &[],
    );
    assert_eq!(h.state(), WifiState::None);
    let read =
        h.call(|r| matches!(r, Request::Property(PropertyRequest::Strength(p)) if p == &path(2)));
    h.change(path(2), AP, props(&[("Strength", Value::U8(30))]), &[]);
    h.reply(read, &(Value::U8(90),));
    assert_eq!(h.state(), WifiState::Low);
    h.change(path(1), AP, props(&[("Strength", Value::U8(99))]), &[]);
    assert_eq!(h.state(), WifiState::Low);
    h.no_scan();
}

#[test]
fn invalidation_reads_one_property_and_later_signal_wins() {
    let mut h = Harness::new();
    h.initialize();
    h.change(path(1), AP, PropMap::new(), &["Strength"]);
    let read = h.call(|r| matches!(r, Request::Property(PropertyRequest::Strength(_))));
    h.change(path(1), AP, props(&[("Strength", Value::U8(50))]), &[]);
    h.reply(read, &(Value::U8(15),));
    assert_eq!(h.state(), WifiState::Medium);
    h.no_scan();
}

#[test]
fn newer_invalidation_replaces_an_in_flight_get() {
    let mut h = Harness::new();
    h.initialize();
    h.change(path(1), AP, PropMap::new(), &["Strength"]);
    let old = h.call(|r| matches!(r, Request::Property(PropertyRequest::Strength(_))));
    h.change(path(1), AP, PropMap::new(), &["Strength"]);
    let current = h.call(|r| matches!(r, Request::Property(PropertyRequest::Strength(_))));
    assert_ne!(old, current);
    h.reply(old, &(Value::U8(10),));
    assert_eq!(h.state(), WifiState::High);
    h.reply(current, &(Value::U8(25),));
    assert_eq!(h.state(), WifiState::Low);
    h.no_scan();
}

#[test]
fn failed_discovery_retries_once_then_waits_for_a_device_event() {
    let mut h = Harness::new();
    h.initialize();
    h.device_signal("DeviceRemoved");
    for attempt in 0..2 {
        let manager = h.call(|r| matches!(r, Request::All(AllRequest::Manager)));
        h.reply(
            manager,
            &(props(&[
                ("WirelessEnabled", Value::Bool(true)),
                ("WirelessHardwareEnabled", Value::Bool(true)),
            ]),),
        );
        let devices = h.call(|r| matches!(r, Request::Devices));
        h.reply(devices, &(0u32,)); // An invalid GetDevices reply.
        assert_eq!(h.state(), WifiState::High);
        assert_eq!(h.app.resource::<WifiStatus>().scan.is_some(), attempt == 0);
    }
    h.device_signal("DeviceAdded");
    assert!(h.app.resource::<WifiStatus>().scan.is_some());
}

#[test]
fn signal_during_discovery_is_replayed_over_older_get_all_reply() {
    let mut h = Harness::new();
    let manager = h.call(|r| matches!(r, Request::All(AllRequest::Manager)));
    h.change(device(1), DEVICE, props(&[("State", Value::U32(40))]), &[]);
    h.reply(
        manager,
        &(props(&[
            ("WirelessEnabled", Value::Bool(true)),
            ("WirelessHardwareEnabled", Value::Bool(true)),
        ]),),
    );
    let devices = h.call(|r| matches!(r, Request::Devices));
    h.reply(devices, &(vec![device(1)],));
    let wifi = h.call(|r| matches!(r, Request::All(AllRequest::Device(_))));
    h.reply(
        wifi,
        &(props(&[
            ("DeviceType", Value::U32(WIFI_DEVICE)),
            ("State", Value::U32(ACTIVATED)),
        ]),),
    );
    let wireless = h.call(|r| matches!(r, Request::All(AllRequest::Wireless(_))));
    h.reply(
        wireless,
        &(props(&[("ActiveAccessPoint", Value::ObjectPath(path(1)))]),),
    );
    let ap = h.call(|r| matches!(r, Request::All(AllRequest::AccessPoint(_))));
    h.reply(ap, &(props(&[("Strength", Value::U8(80))]),));
    assert_eq!(h.state(), WifiState::Disconnected);
    h.no_scan();
}

#[test]
fn successful_ap_read_updates_strength_without_rescanning() {
    let mut h = Harness::new();
    h.initialize();
    h.change(
        device(1),
        WIRELESS,
        props(&[("ActiveAccessPoint", Value::ObjectPath(path(2)))]),
        &[],
    );
    let read =
        h.call(|r| matches!(r, Request::Property(PropertyRequest::Strength(p)) if p == &path(2)));
    h.reply(read, &(Value::U8(25),));
    assert_eq!(h.state(), WifiState::Low);
    h.no_scan();
}

#[test]
fn ap_switch_sends_a_targeted_get_not_a_get_all() {
    let mut h = Harness::new();
    h.initialize();
    h.requests(); // Discard discovery requests.
    h.change(
        device(1),
        WIRELESS,
        props(&[("ActiveAccessPoint", Value::ObjectPath(path(2)))]),
        &[],
    );
    let requests = h.requests();
    assert_eq!(requests.len(), 1);
    let request = &requests[0];
    assert_eq!(request.destination().unwrap().as_str(), NM);
    assert_eq!(request.path(), Some(&path(2)));
    assert_eq!(request.member().unwrap().as_str(), "Get");
    assert_eq!(
        request.body::<(String, String)>().unwrap(),
        (AP.into(), "Strength".into())
    );
    h.no_scan();
}

#[test]
fn device_list_signal_rescans_but_property_signal_does_not() {
    let mut h = Harness::new();
    h.initialize();
    h.change(device(1), DEVICE, props(&[("State", Value::U32(40))]), &[]);
    h.no_scan();
    h.device_signal("DeviceRemoved");
    assert!(h.app.resource::<WifiStatus>().scan.is_some());
    assert!(matches!(
        h.app.resource::<WifiStatus>().pending.values().next(),
        Some(Request::All(AllRequest::Manager))
    ));
    let manager = h.call(|r| matches!(r, Request::All(AllRequest::Manager)));
    h.reply(
        manager,
        &(props(&[
            ("WirelessEnabled", Value::Bool(true)),
            ("WirelessHardwareEnabled", Value::Bool(true)),
        ]),),
    );
    let devices = h.call(|r| matches!(r, Request::Devices));
    h.reply(devices, &(Vec::<ObjectPath>::new(),));
    assert_eq!(h.state(), WifiState::Disconnected);
    h.no_scan();
}

#[test]
fn connecting_ap_signal_is_used_after_activation() {
    let mut h = Harness::new();
    h.initialize();
    h.change(device(1), DEVICE, props(&[("State", Value::U32(40))]), &[]);
    h.change(
        device(1),
        WIRELESS,
        props(&[("ActiveAccessPoint", Value::ObjectPath(path(2)))]),
        &[],
    );
    assert_eq!(h.state(), WifiState::Disconnected);
    assert!(h.app.resource::<WifiStatus>().pending.is_empty());
    h.change(
        device(1),
        DEVICE,
        props(&[("State", Value::U32(ACTIVATED))]),
        &[],
    );
    assert_eq!(h.state(), WifiState::None);
    let read =
        h.call(|r| matches!(r, Request::Property(PropertyRequest::Strength(p)) if p == &path(2)));
    h.reply(read, &(Value::U8(75),));
    assert_eq!(h.state(), WifiState::High);
    h.no_scan();
}

#[test]
fn old_active_ap_read_cannot_restore_a_previous_connection() {
    let mut h = Harness::new();
    h.initialize();
    h.change(device(1), DEVICE, props(&[("State", Value::U32(40))]), &[]);
    h.change(
        device(1),
        DEVICE,
        props(&[("State", Value::U32(ACTIVATED))]),
        &[],
    );
    let old = h.call(|r| matches!(r, Request::Property(PropertyRequest::ActiveAccessPoint(_))));
    h.change(device(1), DEVICE, props(&[("State", Value::U32(30))]), &[]);
    h.change(
        device(1),
        DEVICE,
        props(&[("State", Value::U32(ACTIVATED))]),
        &[],
    );
    let current = h.call(|r| matches!(r, Request::Property(PropertyRequest::ActiveAccessPoint(_))));
    assert_ne!(old, current);
    h.reply(old, &(Value::ObjectPath(path(1)),));
    assert_eq!(h.state(), WifiState::None);
    h.reply(current, &(Value::ObjectPath(path(2)),));
    assert!(
        h.app
            .resource::<WifiStatus>()
            .observed
            .devices
            .get(&device(1))
            .is_some_and(|d| d.ap == Some(path(2)))
    );
    h.no_scan();
}
