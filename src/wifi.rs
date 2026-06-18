use defmt::{error, info, warn};
use embassy_net::{Runner, Stack, StackResources};
use embassy_time::{Duration, Timer};
use esp_hal::rng::Rng;
pub use esp_radio::wifi::Interface;

use esp_radio::wifi::{Config, ControllerConfig, WifiController, sta::StationConfig};

extern crate alloc;

const SSID: &str = env!("SSID");
const PASSWORD: &str = env!("PASS");
const WIFI_TRANSMIT_POWER: &str = env!("WIFI_TRANSMIT_POWER");

pub async fn init(
    wifi: esp_hal::peripherals::WIFI<'static>,
) -> (
    WifiController<'static>,
    Stack<'static>,
    Runner<'static, Interface<'static>>,
) {
    let station_config = Config::Station(
        StationConfig::default()
            .with_ssid(SSID)
            .with_password(PASSWORD.into()),
    );

    let (mut controller, interfaces) = esp_radio::wifi::new(
        wifi,
        ControllerConfig::default().with_initial_config(station_config),
    )
    .expect("Failed to initialize Wi-Fi controller");

    let max_tx_power_dbm: i8 = WIFI_TRANSMIT_POWER
        .parse()
        .expect("WIFI_TRANSMIT_POWER must be a valid integer");
    controller
        // NOTE: Power unit is 0.25dBm, range is [8, 84] corresponding to 2dBm - 20dBm.
        .set_max_tx_power(max_tx_power_dbm * 4)
        .expect("Failed to set max TX power");

    let config = embassy_net::Config::dhcpv4(Default::default());
    let seed = u64::from(Rng::new().random()) << 32 | u64::from(Rng::new().random());

    let (stack, runner) = embassy_net::new(
        interfaces.station,
        config,
        static_cell::make_static!(StackResources::<6>::new()),
        seed,
    );

    (controller, stack, runner)
}

#[embassy_executor::task]
pub async fn connection(mut controller: WifiController<'static>) {
    let mut failed_attempts: u32 = 0;

    loop {
        info!("wifi: connecting to \"{}\"", env!("SSID"));

        match controller.connect_async().await {
            Ok(info) => {
                failed_attempts = 0;
                info!("wifi: connected to \"{}\"", info.ssid.as_str());

                match controller.wait_for_disconnect_async().await {
                    Ok(info) => {
                        info!(
                            "wifi: disconnected from \"{}\" (reason: {:?}, rssi: {})",
                            info.ssid.as_str(),
                            info.reason,
                            info.rssi,
                        );
                    }
                    Err(e) => {
                        warn!("wifi: disconnect wait failed: {:?}", e);
                    }
                }
            }
            Err(err) => {
                failed_attempts += 1;
                warn!("wifi: connection attempt {} failed", failed_attempts);
                match err {
                    esp_radio::wifi::WifiError::Disconnected(info) => {
                        error!(
                            "wifi: disconnected: SSID: \"{}\", reason: {:?}, RSSI: {}",
                            info.ssid.as_str(),
                            info.reason,
                            info.rssi,
                        );
                    }
                    _ => {
                        error!("wifi: connection failed: {:?}", err);
                    }
                }
            }
        }

        Timer::after(Duration::from_millis(5000)).await;
    }
}

#[embassy_executor::task]
pub async fn net_task(mut runner: Runner<'static, Interface<'static>>) {
    runner.run().await;
}
