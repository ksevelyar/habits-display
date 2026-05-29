# Habits Display

## Build & Flash
```
nix develop

cargo run --release
    Finished `release` profile [optimized + debuginfo] target(s) in 0.24s
     Running `probe-rs run --chip=esp32c3 --preverify --always-print-stacktrace --no-location --catch-hardfault target/riscv32imc-unknown-none-elf/release/habits-display`
    Verifying ✔ 100% [####################] 281.96 KiB @  73.55 KiB/s (took 4s)           Finished in 3.99s
Embassy initialized!
01:51:26.489: ws: waiting for network...
01:51:26.489: ntp: waiting for network...
01:51:26.489: wifi: starting... (attempt 1)
01:51:26.599: wifi: connecting to 'fellowship-of-the-ring-2'...
01:51:27.922: wifi: connected, monitoring link...
01:51:30.380: ntp: network ready
01:51:30.380: ntp: start
01:51:30.380: ntp: socket created
01:51:30.380: ntp: bind ok
01:51:30.380: ntp: target set
01:51:30.380: ntp: send result = true
01:51:30.380: ntp: waiting response...
01:51:30.380: ws: network ready
01:51:30.380: ws: connecting...
01:51:30.380: ntp: recv raw = true
01:51:30.380: ntp: received len = 48
01:51:30.380: ntp: raw secs = 3988997490
01:51:30.380: ntp: done
01:51:30.380: ntp: synced, unix = 1780008690
01:51:31.542: ws: connected
01:51:31.542: ws: handshake sent
01:51:31.542: ws: resp = HTTP/1.1 101 Switching Protocols
01:51:31.542: connection: upgrade
01:51:31.542: upgrade: websocket
01:51:31.542: sec-websocket-accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo=
01:51:31.542: vary: origin, access-control-request-method, access-control-request-headers
01:51:31.542: access-control-allow-credentials: true
01:51:31.542: access-control-allow-origin: http://habits.lcl:3000
01:51:31.542: date: Thu, 28 May 2026 22:51:31 GMT
01:51:31.542:
01:51:31.542:
01:51:31.542: ws: handshake ok
01:51:31.542: ws: {"event":"UserAuthenticated","user":{"email":"ksevelyar@gmail.com","id":1}}
```

## udev setup for ESP32-C3 with probe-rs

```nix
services.udev.extraRules = ''
  # NOTE: esp32c3
  SUBSYSTEM=="usb", ATTR{idVendor}=="303a", ATTR{idProduct}=="1001", MODE="0660", GROUP="dialout"
'';
```
