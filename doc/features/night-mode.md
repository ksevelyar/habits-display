# Night Mode

## Requirements
* The device attempts to synchronize its clock via NTP on boot and every four hours thereafter.
* The display uses the configured local time to select its colors.
* From 08:00 until 17:00, the display uses the day colors.
* From 17:00 until 08:00, the display uses the night colors.
* When the mode changes, the display redraws the current reminder in the new colors.

## User scenarios

### 1. User sees the correct mode after the device synchronizes its clock
1. The user turns on the device.
2. The device connects to Wi-Fi and synchronizes its clock via NTP.
3. The local time is 20:00.
4. The display shows the task reminder in night colors.

### 2. User sees the display switch to night mode
1. The display shows a task reminder in day colors.
2. The local time reaches 17:00.
3. The display redraws the reminder in night colors.

### 3. User sees the display switch to day mode
1. The display shows a task reminder in night colors.
2. The local time reaches 08:00.
3. The display redraws the reminder in day colors.
