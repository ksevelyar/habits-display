use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;

static CONNECTED: Signal<CriticalSectionRawMutex, ()> = Signal::new();
pub static TIME_CONFIGURATION_READY: Signal<CriticalSectionRawMutex, ()> = Signal::new();
pub static WEBSOCKET_CONFIGURATION_READY: Signal<CriticalSectionRawMutex, ()> = Signal::new();

pub fn reset() {
    CONNECTED.reset();
    TIME_CONFIGURATION_READY.reset();
    WEBSOCKET_CONFIGURATION_READY.reset();
}

struct WifiController;

impl habits_display::wifi::WifiConnection for WifiController {
    async fn connect(&mut self) -> bool {
        CONNECTED.signal(());
        true
    }

    async fn wait_for_disconnect(&mut self) {
        core::future::pending().await
    }
}

#[embassy_executor::task]
pub async fn run_wifi_task() {
    habits_display::wifi::run_connection(WifiController).await;
}

#[embassy_executor::task]
pub async fn run_network_task() {
    CONNECTED.wait().await;
    TIME_CONFIGURATION_READY.signal(());
    WEBSOCKET_CONFIGURATION_READY.signal(());
}
