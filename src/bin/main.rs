#![no_std]
#![no_main]

extern crate alloc;

use embassy_executor::Spawner;
use embassy_net::StackResources;
use embassy_time::{Duration, Timer};
use esp_hal::clock::CpuClock;
use esp_hal::timer::timg::TimerGroup;
use esp_radio::Controller;
use rtt_target::rprintln;
use static_cell::StaticCell;

use habits_display::{ntp, websocket, wifi};

static RADIO: StaticCell<Controller<'static>> = StaticCell::new();
static RES: StaticCell<StackResources<3>> = StaticCell::new();

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

    let (controller, interfaces) =
        esp_radio::wifi::new(radio, peripherals.WIFI, Default::default())
            .expect("wifi: init failed");

    let net_cfg = embassy_net::Config::dhcpv4(Default::default());
    let (stack, runner) = embassy_net::new(
        interfaces.sta,
        net_cfg,
        RES.init(StackResources::new()),
        1234,
    );

    spawner.spawn(net_task(runner)).ok();
    spawner.spawn(wifi::task(controller, stack)).ok();
    spawner.spawn(ntp::task(stack)).ok();
    spawner.spawn(websocket::task(stack)).ok();
    spawner.spawn(clock_display()).ok();

    loop {
        Timer::after(Duration::from_secs(3600)).await;
    }
}

#[embassy_executor::task]
async fn clock_display() -> ! {
    loop {
        Timer::after(Duration::from_secs(60)).await;
        if let Some((h, m, s)) = ntp::current_moscow_hms() {
            rprintln!("clock: {:02}:{:02}:{:02} MSK", h, m, s);
        } else {
            rprintln!("clock: not synced");
        }
    }
}
