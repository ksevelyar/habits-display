use embassy_net::Stack;
use embassy_time::{Duration, Timer};
use esp_radio::wifi::{ClientConfig, ModeConfig, WifiController};
use rtt_target::rprintln;

#[embassy_executor::task]
pub async fn task(mut controller: WifiController<'static>, stack: Stack<'static>) -> ! {
    let ssid = env!("SSID");
    let pass = env!("PASS");

    controller
        .set_config(&ModeConfig::Client(
            ClientConfig::default()
                .with_ssid(ssid.into())
                .with_password(pass.into()),
        ))
        .unwrap();

    let mut attempt: u32 = 0;
    loop {
        attempt += 1;
        rprintln!("wifi: starting... (attempt {})", attempt);

        controller.stop().ok();
        Timer::after(Duration::from_millis(100)).await;

        if let Err(e) = controller.start() {
            rprintln!("wifi: start failed: {:?}", e);
            Timer::after(Duration::from_secs(3)).await;
            continue;
        }

        rprintln!("wifi: connecting to '{}'...", ssid);
        controller.connect_async().await.ok();

        rprintln!("wifi: connected, monitoring link...");

        while stack.is_link_up() {
            Timer::after(Duration::from_secs(1)).await;
        }

        rprintln!("wifi: link lost, reconnecting...");
        Timer::after(Duration::from_secs(1)).await;
    }
}
