# Habits Display

A small desktop display that shows calendar notifications from habits-axum.

## Tests

Tests require a connected MCU.

```sh
nix develop
cargo test
```

## Build and flash

```sh
nix develop

cargo run --release
[INFO ] Connected to "fellowship-of-the-ring-2" (channel: 4)
[INFO ] ws: network ready
[INFO ] NTP: got IP
[INFO ] Rtc after update:1780660466
[INFO ] ws: connected
[INFO ] ws: handshake sent
[INFO ] ws: handshake ok
[INFO ] ws: {"event":"UserAuthenticated","user":{"email":"ksevelyar@gmail.com","id":1,"timezone":"Europe/Moscow"}}
[INFO ] ws: task: take a walk
```
