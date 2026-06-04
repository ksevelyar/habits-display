use defmt::{error, info, warn};
use embassy_net::{Runner, Stack, StackResources};
use embassy_time::{Duration, Timer};
use esp_hal::rng::Rng;
pub use esp_radio::wifi::Interface;

use esp_radio::wifi::{Config, ControllerConfig, WifiController, sta::StationConfig};

extern crate alloc;

const SSID: &str = env!("SSID");
const PASSWORD: &str = env!("PASS");

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

    info!("starting wifi");
    let (mut controller, interfaces) = esp_radio::wifi::new(
        wifi,
        ControllerConfig::default().with_initial_config(station_config),
    )
    .expect("Failed to initialize Wi-Fi controller");

    // NOTE: Power unit is 0.25dBm, range is [8, 84] corresponding to 2dBm - 20dBm.
    controller
        .set_max_tx_power(64)
        .expect("Failed to set max TX power");

    let config = embassy_net::Config::dhcpv4(Default::default());
    let seed = (Rng::new().random() as u64) << 32 | Rng::new().random() as u64;

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
    info!("start connection task");
    let mut failed_attempts: u32 = 0;

    loop {
        info!("About to connect...");

        match controller.connect_async().await {
            Ok(info) => {
                failed_attempts = 0;
                info!(
                    "Connected to \"{}\" (channel: {})",
                    info.ssid.as_str(),
                    info.channel,
                );

                match controller.wait_for_disconnect_async().await {
                    Ok(info) => {
                        info!(
                            "Disconnected from \"{}\" (reason: {:?}, rssi: {})",
                            info.ssid.as_str(),
                            info.reason,
                            info.rssi,
                        );
                    }
                    Err(e) => {
                        warn!("Disconnect wait failed: {:?}", e);
                    }
                }
            }
            Err(err) => {
                failed_attempts += 1;
                warn!("Connection attempt {} failed", failed_attempts);
                match err {
                    esp_radio::wifi::WifiError::Disconnected(info) => {
                        error!(
                            "SSID: \"{}\", reason: {:?}, RSSI: {}",
                            info.ssid.as_str(),
                            info.reason,
                            info.rssi,
                        );
                    }
                    _ => {
                        error!("Failed to connect to wifi: {:?}", err);
                    }
                }
            }
        }

        Timer::after(Duration::from_millis(5000)).await
    }
}

#[embassy_executor::task]
pub async fn net_task(mut runner: Runner<'static, Interface<'static>>) {
    runner.run().await
}
