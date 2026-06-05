# Habits Display

## Build & Flash
```
nix develop

cargo run --release
   Compiling habits-display v0.1.0 (/home/ksevelyar/code/habits-display)
    Finished `release` profile [optimized + debuginfo] target(s) in 3.19s
     Running `probe-rs run --chip=esp32c3 --preverify --always-print-stacktrace --no-location target/riscv32imc-unknown-none-elf/release/habits-display`
      Erasing ✔ 100% [####################] 640.00 KiB @ 643.39 KiB/s (took 1s)
  Programming ✔ 100% [####################] 328.90 KiB @  83.70 KiB/s (took 4s)           Finished in 5.73s
[INFO ] Embassy initialized!
[INFO ] starting wifi
[INFO ] IPv4: DOWN
[INFO ] start connection task
[INFO ] About to connect...
[INFO ] link_up = true
[INFO ] IPv4: DOWN
[INFO ] Connected to "fellowship-of-the-ring-2" (channel: 4)
[INFO ] ws: network ready
[INFO ] NTP: got IP
[INFO ] Rtc after update:1780660466
[INFO ] ws: connected
[INFO ] ws: handshake sent
[INFO ] ws: handshake ok
[INFO ] ws: {"event":"UserAuthenticated","user":{"email":"ksevelyar@gmail.com","id":1,"timezone":"Europe/Moscow"}}
[INFO ] ws: ping
[INFO ] ws: ping
[INFO ] ws: task: debug 🐗 piglet
[INFO ] ws: ping
[INFO ] ws: ping
[INFO ] ws: task: debug 🐗 piglet
```

## udev setup for ESP32-C3 with probe-rs

```nix
services.udev.extraRules = ''
  # NOTE: esp32c3
  SUBSYSTEM=="usb", ATTR{idVendor}=="303a", ATTR{idProduct}=="1001", MODE="0660", GROUP="dialout"
'';
```
