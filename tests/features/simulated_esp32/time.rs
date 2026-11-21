use core::net::{IpAddr, Ipv4Addr};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_sync::signal::Signal;
use esp_hal::rtc_cntl::Rtc;
use habits_display::{AppError, time};

pub static TIME_REQUESTED: Signal<CriticalSectionRawMutex, ()> = Signal::new();

pub struct TimeNetwork {
    pub epoch_microseconds: u64,
}

impl time::TimeNetwork for TimeNetwork {
    async fn wait_for_configuration(&mut self) {
        super::wifi::TIME_CONFIGURATION_READY.wait().await;
    }

    async fn resolve_server(&mut self) -> Result<IpAddr, AppError> {
        Ok(IpAddr::V4(Ipv4Addr::LOCALHOST))
    }

    async fn request_time(
        &mut self,
        _address: IpAddr,
        _current_time_us: u64,
    ) -> Result<u64, AppError> {
        TIME_REQUESTED.signal(());
        Ok(self.epoch_microseconds)
    }
}

pub async fn set_time(rtc: &Mutex<CriticalSectionRawMutex, Rtc<'static>>, epoch: u32) {
    let rtc = rtc.lock().await;
    rtc.set_current_time_us(u64::from(epoch) * 1_000_000);
    time::seed_from_rtc(&rtc);
}

#[embassy_executor::task]
pub async fn run_time_task(
    rtc: &'static Mutex<CriticalSectionRawMutex, Rtc<'static>>,
    network: TimeNetwork,
) {
    time::run_time(rtc, network).await;
}
