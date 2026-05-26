#![no_std]
#![no_main]

extern crate alloc;

use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};

use esp_hal::clock::CpuClock;
use esp_hal::timer::timg::TimerGroup;

use rtt_target::rprintln;

use static_cell::StaticCell;

use esp_radio::Controller;
use esp_radio::wifi;

use embassy_net::udp::{PacketMetadata, UdpSocket};
use embassy_net::{Config as NetConfig, StackResources};

static RADIO: StaticCell<Controller<'static>> = StaticCell::new();
static RES: StaticCell<StackResources<3>> = StaticCell::new();

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

#[embassy_executor::task]
async fn net_task(
    mut runner: embassy_net::Runner<'static, esp_radio::wifi::WifiDevice<'static>>,
) -> ! {
    runner.run().await
}

async fn get_ntp_time(stack: embassy_net::Stack<'static>) -> Option<(u8, u8, u8)> {
    rprintln!("NTP: start");

    let mut rx_meta = [PacketMetadata::EMPTY; 1];
    let mut rx_buf = [0u8; 128];
    let mut tx_meta = [PacketMetadata::EMPTY; 1];
    let mut tx_buf = [0u8; 128];

    let mut sock = UdpSocket::new(stack, &mut rx_meta, &mut rx_buf, &mut tx_meta, &mut tx_buf);

    rprintln!("NTP: socket created");

    sock.bind(0).ok()?;
    rprintln!("NTP: bind ok");

    let mut req = [0u8; 48];
    req[0] = 0x1B;

    let ntp_ip = embassy_net::Ipv4Address::new(216, 239, 35, 0);
    rprintln!("NTP: target set");

    let send = sock.send_to(&req, (ntp_ip, 123)).await;
    rprintln!("NTP: send result = {:?}", send.is_ok());
    send.ok()?;

    rprintln!("NTP: waiting response...");

    let mut resp = [0u8; 48];

    let recv = embassy_time::with_timeout(Duration::from_secs(3), sock.recv_from(&mut resp)).await;

    rprintln!("NTP: recv raw = {:?}", recv.is_ok());

    let Ok(Ok((len, _))) = recv else {
        rprintln!("NTP: timeout/fail");
        return None;
    };

    rprintln!("NTP: received len = {}", len);

    if len < 48 {
        rprintln!("NTP: invalid packet size");
        return None;
    }

    let secs = u32::from_be_bytes([resp[40], resp[41], resp[42], resp[43]]);
    rprintln!("NTP: raw secs = {}", secs);

    let unix = secs.wrapping_sub(2_208_988_800);
    let t = unix % 86400;

    rprintln!("NTP: done");

    Some(((t / 3600) as u8, ((t % 3600) / 60) as u8, (t % 60) as u8))
}

#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    rtt_target::rtt_init_print!();

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 66320);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let sw_interrupt =
        esp_hal::interrupt::software::SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);

    esp_rtos::start(timg0.timer0, sw_interrupt.software_interrupt0);
    rprintln!("Embassy initialized!");

    let radio = RADIO.init(esp_radio::init().unwrap());

    let (mut wifi, interfaces) = esp_radio::wifi::new(radio, peripherals.WIFI, Default::default())
        .expect("Failed to initialize Wi-Fi controller");

    let ssid = env!("SSID");
    let pass = env!("PASS");

    wifi.set_config(&wifi::ModeConfig::Client(
        wifi::ClientConfig::default()
            .with_ssid(ssid.into())
            .with_password(pass.into()),
    ))
    .unwrap();

    wifi.start().unwrap();

    rprintln!("connecting...");
    wifi.connect_async().await.ok();

    let net_cfg = NetConfig::dhcpv4(Default::default());

    let (stack, runner) = embassy_net::new(
        interfaces.sta,
        net_cfg,
        RES.init(StackResources::new()),
        1234,
    );

    spawner.spawn(net_task(runner)).ok();

    rprintln!("WiFi stack up");
    let mut cached = None;
    let mut ntp_done = false;

    loop {
        if let Some(cfg) = stack.config_v4() {
            rprintln!("IP: {}", cfg.address);

            if !ntp_done {
                rprintln!("starting NTP...");
                cached = get_ntp_time(stack).await;
                rprintln!("ntp result: {:?}", cached);
                ntp_done = true;
            }

            if let Some((h, m, s)) = cached {
                rprintln!("time: {:02}:{:02}:{:02}", h, m, s);
            }
        } else {
            rprintln!("no ipv4 config yet");
        }

        Timer::after(Duration::from_secs(5)).await;
    }
}
