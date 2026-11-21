use embassy_executor::Spawner;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use esp_hal::rtc_cntl::Rtc;

pub mod display;
pub mod time;
pub mod websocket;
pub mod wifi;

pub struct Application {
    pub rtc: &'static Mutex<CriticalSectionRawMutex, Rtc<'static>>,
    epoch: u32,
    display: (
        esp_hal::spi::master::Spi<'static, esp_hal::Blocking>,
        esp_hal::peripherals::GPIO7<'static>,
        esp_hal::peripherals::GPIO8<'static>,
        esp_hal::peripherals::GPIO9<'static>,
    ),
}

pub fn build_application(epoch: u32) -> Application {
    let peripherals = esp_hal::init(esp_hal::Config::default());
    let timer = esp_hal::timer::timg::TimerGroup::new(peripherals.TIMG0);
    let interrupts =
        esp_hal::interrupt::software::SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    esp_rtos::start(timer.timer0, interrupts.software_interrupt0);
    let rtc = Rtc::new(peripherals.LPWR);
    rtc.set_current_time_us(0);
    display::reset();
    websocket::reset();
    wifi::reset();
    time::TIME_REQUESTED.reset();
    habits_display::time::TIME_CHANGED.reset();
    let rtc = habits_display::time::RTC.init(Mutex::new(rtc));
    let spi = habits_display::display::initialize_spi(
        peripherals.SPI2,
        peripherals.GPIO4,
        peripherals.GPIO6,
    );
    Application {
        rtc,
        epoch,
        display: (spi, peripherals.GPIO7, peripherals.GPIO8, peripherals.GPIO9),
    }
}

pub fn run_application(spawner: Spawner, application: Application) {
    let (spi, chip_select, data_command, reset) = application.display;
    spawner.spawn(
        display::run_display_task(spi, chip_select, data_command, reset)
            .expect("create display task"),
    );
    let time_network = time::TimeNetwork {
        epoch_microseconds: u64::from(application.epoch) * 1_000_000,
    };
    spawner.spawn(time::run_time_task(application.rtc, time_network).expect("create time task"));
    spawner.spawn(websocket::run_websocket_task().expect("create WebSocket task"));
    spawner.spawn(wifi::run_wifi_task().expect("create Wi-Fi task"));
    spawner.spawn(wifi::run_network_task().expect("create network task"));
}

pub fn local_epoch(hour: i8, minute: i8, second: i8) -> u32 {
    let local_time = jiff::civil::date(2026, 1, 1)
        .at(hour, minute, second, 0)
        .to_zoned(habits_display::time::TIMEZONE)
        .expect("local time exists");
    u32::try_from(local_time.timestamp().as_second()).expect("epoch fits")
}
