use crate::AppError;
use core::net::{IpAddr, SocketAddr};
use core::sync::atomic::{AtomicU32, Ordering};

use defmt::{error, info};
use embassy_net::{
    Stack,
    dns::DnsQueryType,
    udp::{PacketMetadata, UdpSocket},
};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_sync::signal::Signal;
use embassy_time::Instant;
use static_cell::{ConstStaticCell, StaticCell};

use embassy_time::{Duration, Timer};
use esp_hal::rtc_cntl::Rtc;
use jiff::Timestamp;

include!(concat!(env!("OUT_DIR"), "/timezone.rs"));

use sntpc::{NtpContext, NtpTimestampGenerator, get_time};
use sntpc_net_embassy::UdpSocketWrapper;

const MIN_VALID_EPOCH: u32 = 1_700_000_000;
const USEC_IN_SEC: u64 = 1_000_000;

static EPOCH_BASE: AtomicU32 = AtomicU32::new(0);
static INSTANT_BASE: AtomicU32 = AtomicU32::new(0);
static INSTANT_MILLIS_BASE: AtomicU32 = AtomicU32::new(0);

pub static RTC: StaticCell<Mutex<CriticalSectionRawMutex, Rtc<'static>>> = StaticCell::new();
pub static TIME_CHANGED: Signal<CriticalSectionRawMutex, ()> = Signal::new();

defmt::timestamp!(
    "{}",
    LocalTime {
        zoned: local_time(),
        tenth_of_second: tenth_of_second(),
    }
);

pub fn epoch_secs() -> Option<u32> {
    let base = EPOCH_BASE.load(Ordering::Acquire);
    if base == 0 {
        return None;
    }
    let now = Instant::now().as_secs();
    let ibase = u64::from(INSTANT_BASE.load(Ordering::Relaxed));
    let delta = now.saturating_sub(ibase) as u32;
    Some(base + delta)
}

fn tenth_of_second() -> u8 {
    if EPOCH_BASE.load(Ordering::Acquire) == 0 {
        return 0;
    }
    let now_millis = Instant::now().as_millis() as u32;
    let base_millis = INSTANT_MILLIS_BASE.load(Ordering::Relaxed);
    let subsec_millis = now_millis.wrapping_sub(base_millis) % 1000;
    (subsec_millis / 100) as u8
}

pub fn local_time() -> jiff::Zoned {
    let epoch_secs = epoch_secs().unwrap_or(0);
    let timestamp = jiff::Timestamp::new(i64::from(epoch_secs), 0).unwrap();

    timestamp.to_zoned(crate::time::TIMEZONE)
}

struct LocalTime {
    zoned: jiff::Zoned,
    tenth_of_second: u8,
}

impl defmt::Format for LocalTime {
    fn format(&self, formatter: defmt::Formatter) {
        defmt::write!(
            formatter,
            "{=u8:02}:{=u8:02}:{=u8:02}.{=u8}",
            self.zoned.hour() as u8,
            self.zoned.minute() as u8,
            self.zoned.second() as u8,
            self.tenth_of_second,
        );
    }
}

fn set_epoch(epoch_secs: u32) {
    INSTANT_BASE.store(Instant::now().as_secs() as u32, Ordering::Relaxed);
    INSTANT_MILLIS_BASE.store(Instant::now().as_millis() as u32, Ordering::Relaxed);
    EPOCH_BASE.store(epoch_secs, Ordering::Release);
    TIME_CHANGED.signal(());
}

pub fn seed_from_rtc(rtc: &Rtc) {
    let us = rtc.current_time_us();
    let secs = (us / USEC_IN_SEC) as u32;
    if secs > MIN_VALID_EPOCH {
        set_epoch(secs);
        let ts = Timestamp::new(i64::from(secs), 0).unwrap();
        let z = ts.to_zoned(TIMEZONE);
        info!(
            "time: rtc seeded: {=str} {=i16}-{=i8:02}-{=i8:02} {=i8:02}:{=i8:02}:{=i8:02}",
            TIMEZONE.iana_name().unwrap(),
            z.year(),
            z.month(),
            z.day(),
            z.hour(),
            z.minute(),
            z.second()
        );
    }
}

fn log_sync(correction: Option<i64>) {
    let corr = correction.unwrap_or(0);
    if corr >= 0 {
        info!("time: synced (+{=i64}s)", corr);
    } else {
        info!("time: synced ({=i64}s)", corr);
    }

    if let Some(epoch) = epoch_secs() {
        let ts = Timestamp::new(i64::from(epoch), 0).unwrap();
        let z = ts.to_zoned(TIMEZONE);
        info!(
            "time: {=str} {=i16}-{=i8:02}-{=i8:02} {=i8:02}:{=i8:02}:{=i8:02}",
            TIMEZONE.iana_name().unwrap(),
            z.year(),
            z.month(),
            z.day(),
            z.hour(),
            z.minute(),
            z.second()
        );
    }
}

#[derive(Clone, Copy)]
struct NtpTs {
    current_time_us: u64,
}

impl NtpTimestampGenerator for NtpTs {
    fn init(&mut self) {}
    fn timestamp_sec(&self) -> u64 {
        self.current_time_us / USEC_IN_SEC
    }
    fn timestamp_subsec_micros(&self) -> u32 {
        (self.current_time_us % USEC_IN_SEC) as u32
    }
}

pub trait TimeNetwork {
    fn wait_for_configuration(&mut self) -> impl core::future::Future<Output = ()>;
    fn resolve_server(&mut self) -> impl core::future::Future<Output = Result<IpAddr, AppError>>;
    fn request_time(
        &mut self,
        address: IpAddr,
        current_time_us: u64,
    ) -> impl core::future::Future<Output = Result<u64, AppError>>;
}

struct HardwareTimeNetwork {
    stack: Stack<'static>,
    receive_metadata: &'static mut [PacketMetadata; 16],
    receive_buffer: &'static mut [u8; 4096],
    transmit_metadata: &'static mut [PacketMetadata; 16],
    transmit_buffer: &'static mut [u8; 4096],
}

impl TimeNetwork for HardwareTimeNetwork {
    async fn wait_for_configuration(&mut self) {
        self.stack.wait_config_up().await;
    }

    async fn resolve_server(&mut self) -> Result<IpAddr, AppError> {
        let server = "pool.ntp.org";
        let addresses = self
            .stack
            .dns_query(server, DnsQueryType::A)
            .await
            .map_err(|_| AppError::Network("resolving ntp server failed"))?;
        addresses
            .first()
            .copied()
            .map(Into::into)
            .ok_or(AppError::Network("resolving ntp server failed"))
    }

    async fn request_time(
        &mut self,
        address: IpAddr,
        current_time_us: u64,
    ) -> Result<u64, AppError> {
        let mut socket = UdpSocket::new(
            self.stack,
            self.receive_metadata,
            self.receive_buffer,
            self.transmit_metadata,
            self.transmit_buffer,
        );
        socket.bind(0).expect("bind NTP socket");
        let socket = UdpSocketWrapper::new(socket);
        let time = get_time(
            SocketAddr::from((address, 123)),
            &socket,
            NtpContext::new(NtpTs { current_time_us }),
        )
        .await
        .map_err(|_| AppError::Network("ntp exchange failed"))?;
        Ok((u64::from(time.sec()) * USEC_IN_SEC)
            + ((u64::from(time.sec_fraction()) * USEC_IN_SEC) >> 32))
    }
}

pub async fn run_time<Network: TimeNetwork>(
    rtc: &Mutex<CriticalSectionRawMutex, Rtc<'static>>,
    mut network: Network,
) {
    seed_from_rtc(&*rtc.lock().await);
    loop {
        network.wait_for_configuration().await;
        let result = async {
            let address = network.resolve_server().await?;
            let current_time_us = rtc.lock().await.current_time_us();
            network.request_time(address, current_time_us).await
        }
        .await;
        match result {
            Ok(epoch_us) => {
                let old_epoch = epoch_secs();
                let new_epoch = (epoch_us / USEC_IN_SEC) as u32;
                let correction = old_epoch.map(|old| i64::from(new_epoch) - i64::from(old));
                rtc.lock().await.set_current_time_us(epoch_us);
                set_epoch(new_epoch);
                log_sync(correction);
            }
            Err(error) => error!("time: sync failed: {}", error),
        }
        Timer::after(Duration::from_secs(4 * 3600)).await;
    }
}

#[embassy_executor::task]
pub async fn task(rtc: Rtc<'static>, stack: Stack<'static>) {
    static RECEIVE_METADATA: ConstStaticCell<[PacketMetadata; 16]> =
        ConstStaticCell::new([PacketMetadata::EMPTY; 16]);
    static RECEIVE_BUFFER: ConstStaticCell<[u8; 4096]> = ConstStaticCell::new([0; 4096]);
    static TRANSMIT_METADATA: ConstStaticCell<[PacketMetadata; 16]> =
        ConstStaticCell::new([PacketMetadata::EMPTY; 16]);
    static TRANSMIT_BUFFER: ConstStaticCell<[u8; 4096]> = ConstStaticCell::new([0; 4096]);
    let network = HardwareTimeNetwork {
        stack,
        receive_metadata: RECEIVE_METADATA.take(),
        receive_buffer: RECEIVE_BUFFER.take(),
        transmit_metadata: TRANSMIT_METADATA.take(),
        transmit_buffer: TRANSMIT_BUFFER.take(),
    };
    let rtc = RTC.init(Mutex::new(rtc));
    run_time(rtc, network).await;
}
