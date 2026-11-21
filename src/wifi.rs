use defmt::{error, info, warn};
use embassy_net::{Runner, Stack, StackResources};
use embassy_time::{Duration, Timer};
use esp_hal::rng::Rng;
pub use esp_radio::wifi::Interface;

use esp_radio::wifi::{Config, ControllerConfig, WifiController, sta::StationConfig};

extern crate alloc;

const SSID: &str = env!("SSID");

pub fn init(
    wifi: esp_hal::peripherals::WIFI<'static>,
) -> (
    WifiController<'static>,
    Stack<'static>,
    Runner<'static, Interface<'static>>,
) {
    static NETWORK_RESOURCES: static_cell::ConstStaticCell<StackResources<6>> =
        static_cell::ConstStaticCell::new(StackResources::new());
    let password = env!("PASS");
    let station_config = Config::Station(
        StationConfig::default()
            .with_ssid(SSID)
            .with_password(password.into()),
    );

    let (mut controller, interfaces) = esp_radio::wifi::new(
        wifi,
        ControllerConfig::default().with_initial_config(station_config),
    )
    .expect("Failed to initialize Wi-Fi controller");

    let configured_tx_power_dbm: i8 = env!("WIFI_TRANSMIT_POWER")
        .parse()
        .expect("WIFI_TRANSMIT_POWER must be a valid integer");
    controller
        .set_max_tx_power(configured_tx_power_dbm * 4)
        .expect("Failed to set max TX power");

    let config = embassy_net::Config::dhcpv4(embassy_net::DhcpConfig::default());
    let seed = u64::from(Rng::new().random()) << 32 | u64::from(Rng::new().random());

    let (stack, runner) =
        embassy_net::new(interfaces.station, config, NETWORK_RESOURCES.take(), seed);

    (controller, stack, runner)
}

pub trait WifiConnection {
    fn connect(&mut self) -> impl core::future::Future<Output = bool>;
    fn wait_for_disconnect(&mut self) -> impl core::future::Future<Output = ()>;
}

impl WifiConnection for WifiController<'static> {
    async fn connect(&mut self) -> bool {
        match self.connect_async().await {
            Ok(connection) => {
                info!("wifi: connected to \"{}\"", connection.ssid.as_str());
                true
            }
            Err(error) => {
                if let esp_radio::wifi::WifiError::Disconnected(connection) = error {
                    error!(
                        "wifi: disconnected: SSID: \"{}\", reason: {:?}, RSSI: {}",
                        connection.ssid.as_str(),
                        connection.reason,
                        connection.rssi,
                    );
                } else {
                    error!("wifi: connection failed: {:?}", error);
                }
                false
            }
        }
    }

    async fn wait_for_disconnect(&mut self) {
        match self.wait_for_disconnect_async().await {
            Ok(connection) => info!(
                "wifi: disconnected from \"{}\" (reason: {:?}, rssi: {})",
                connection.ssid.as_str(),
                connection.reason,
                connection.rssi,
            ),
            Err(error) => warn!("wifi: disconnect wait failed: {:?}", error),
        }
    }
}

pub async fn run_connection<Connection: WifiConnection>(mut controller: Connection) {
    let mut failed_attempts: u32 = 0;
    loop {
        info!("wifi: connecting to \"{}\"", SSID);
        if controller.connect().await {
            failed_attempts = 0;
            controller.wait_for_disconnect().await;
        } else {
            failed_attempts += 1;
            warn!("wifi: connection attempt {} failed", failed_attempts);
        }
        Timer::after(Duration::from_millis(5000)).await;
    }
}

#[embassy_executor::task]
pub async fn connection(controller: WifiController<'static>) {
    run_connection(controller).await;
}

#[embassy_executor::task]
pub async fn net_task(mut runner: Runner<'static, Interface<'static>>) {
    runner.run().await;
}
