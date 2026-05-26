# Habits Display

## Udev

```nix
services.udev.extraRules = ''
  # NOTE: esp32c3
  SUBSYSTEM=="usb", ATTR{idVendor}=="303a", ATTR{idProduct}=="1001", MODE="0660", GROUP="dialout"
'';
```
