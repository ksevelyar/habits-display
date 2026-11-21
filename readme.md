# Habits Display

A small desktop display that shows calendar notifications from habits-axum over WebSocket.

## Features
* displays the latest due task reminder and replaces it when a new reminder arrives
* switches display colors between day and night using local time
* synchronizes time via NTP on boot and every four hours
* uses an ESP32-C3 Super Mini and a 3D-printed case

## Documentation
* [Notifications](doc/features/notifications.md)
* [Night mode](doc/features/night-mode.md)
* [Wiring](doc/wiring.md)
* [Project boundaries](doc/boundaries.md)

## Tests
Tests require a connected MCU.

```sh
nix develop
cargo test
```

## Build and flash
```sh
nix develop

JWT_TOKEN="<jwt>" SSID="skynet-2" PASS="<pass>" USE_TLS=true NOTIFICATIONS_HOST="api.habits.rusty-cluster.net:443" cargo run --release

03:00:00.0 [INFO ] init: embassy initialized
13:15:50.0 [INFO ] time: rtc seeded: Europe/Moscow 2026-01-01 13:15:50
13:15:50.0 [INFO ] wifi: connecting to "skynet-2"
13:15:51.9 [INFO ] wifi: connected to "skynet-2"
13:15:51.9 [INFO ] ws: network ready
13:15:52.3 [INFO ] ws: connected
19:42:44.0 [INFO ] time: synced (+28016812s)
19:42:44.0 [INFO ] time: Europe/Moscow 2026-11-21 19:42:44
19:42:45.1 [INFO ] ws: tls established
19:42:45.2 [INFO ] ws: handshake sent
19:42:45.2 [INFO ] ws: handshake ok
19:42:45.2 [INFO ] ws: task:  Vacuum floor
19:42:45.3 [INFO ] ws: ping
```

## Logs
```sh
nix develop
cargo build --release
probe-rs attach --no-location --always-print-stacktrace target/riscv32imc-unknown-none-elf/release/habits-display
```
