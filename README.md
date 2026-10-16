# Mechanix Status Bar

A lightweight Wayland status bar built with [`mecha-wayland`](https://github.com/mecha-org) using the Layer Shell protocol.

## Features

- **Wi-Fi (`WifiModule`):** Monitors radio state, device connection, and signal strength via NetworkManager.
- **Bluetooth (`BluetoothModule`):** Tracks adapter power and active connections via BlueZ.
- **Battery (`UpowerModule`):** Displays live battery percentage and presence via UPower.

## Getting Started

```bash
# Run the status bar (requires an active Wayland session)
cargo run

# Run test suite
cargo test
```

