# Notifications

## Requirements
* On boot, the device connects to the backend's notification service using its configured credentials.
* The device displays the most recent due task reminder received from the backend.
* When a new task reminder arrives, it replaces the previous reminder on the display.

## User scenarios

### 1. User receives the last notification on boot
1. A task reminder is due before the device starts.
2. The user turns on the device.
3. The device connects to the backend.
4. The backend sends the most recent due task reminder.
5. The display shows the task name.

### 2. User receives a new notification
1. The device is connected to the backend and displays a task reminder.
2. A new task reminder is due.
3. The backend sends the new reminder.
4. The display replaces the previous task name with the new one.
