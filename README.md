# Mechanix Status Bar
Wi-Fi status is read from NetworkManager on the system D-Bus using
mecha-wayland's `DBusModule`. `WifiModule` monitors the radio, activated
wireless devices, and active access-point strength, then sends
`StatusUpdate::Wifi` to the status bar. Other indicators still use their
existing default states.

The Wi-Fi indicator is hidden when the software or hardware radio is off
or NetworkManager is unavailable. With the radio enabled it shows the
disconnected icon until a Wi-Fi device is activated. Signal strength maps
to none (0–29%), low (30–69%), and high (70–100%); with multiple active
Wi-Fi connections, the strongest signal is shown. Device changes and
NetworkManager restarts trigger a refresh. The underlying D-Bus module
does not reconnect after the system bus itself disconnects.

Run `cargo run` from a Wayland session with NetworkManager available.
Changing the connection or Wi-Fi radio through your network settings
should update the indicator live. Run `cargo test` for the widget and
Wi-Fi state mapping checks.
