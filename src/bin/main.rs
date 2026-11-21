#![no_std]
#![no_main]

use esp_alloc as _;
use esp_hal::{clock::CpuClock, rng::Rng, rtc_cntl::Rtc, timer::timg::TimerGroup};
use habits_display::{display, time, websocket, wifi};
use panic_rtt_target as _;

use defmt::info;
use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};

extern crate alloc;

esp_bootloader_esp_idf::esp_app_desc!();

#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    rtt_target::rtt_init_defmt!();

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);
    let rtc = Rtc::new(peripherals.LPWR);

    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 66320);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let sw_interrupt =
        esp_hal::interrupt::software::SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    esp_rtos::start(timg0.timer0, sw_interrupt.software_interrupt0);
    info!("init: embassy initialized");

    let (controller, stack, runner) = wifi::init(peripherals.WIFI);

    let spi = display::initialize_spi(peripherals.SPI2, peripherals.GPIO4, peripherals.GPIO6);
    let cs = peripherals.GPIO7;
    let dc = peripherals.GPIO8;
    let rst = peripherals.GPIO9;

    let rng = Rng::new();

    spawner.spawn(wifi::connection(controller).unwrap());
    spawner.spawn(wifi::net_task(runner).unwrap());
    spawner.spawn(time::task(rtc, stack).unwrap());
    spawner.spawn(websocket::task(stack, rng).unwrap());
    spawner.spawn(display::task(spi, cs, dc, rst).unwrap());

    loop {
        Timer::after(Duration::from_secs(3600)).await;
    }
}
