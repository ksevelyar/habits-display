use core::net::{IpAddr, SocketAddr};
use core::sync::atomic::{AtomicU32, Ordering};

use defmt::{error, info};
use embassy_net::{
    Stack,
    dns::DnsQueryType,
    udp::{PacketMetadata, UdpSocket},
};
use embassy_time::Instant;

use embassy_time::{Duration, Timer};
use esp_hal::rtc_cntl::Rtc;
use jiff::Timestamp;

include!(concat!(env!("OUT_DIR"), "/timezone.rs"));

use sntpc::{NtpContext, NtpTimestampGenerator, get_time};
use sntpc_net_embassy::UdpSocketWrapper;

const NTP_SERVER: &str = "pool.ntp.org";
const MIN_VALID_EPOCH: u32 = 1_700_000_000;
const USEC_IN_SEC: u64 = 1_000_000;

static EPOCH_BASE: AtomicU32 = AtomicU32::new(0);
static INSTANT_BASE: AtomicU32 = AtomicU32::new(0);

defmt::timestamp!(
    "{=u8:02}:{=u8:02}:{=u8:02}",
    { local_time().hour() as u8 },
    { local_time().minute() as u8 },
    { local_time().second() as u8 },
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

fn local_time() -> jiff::Zoned {
    let epoch_secs = epoch_secs().unwrap_or(0);
    let timestamp = jiff::Timestamp::new(i64::from(epoch_secs), 0).unwrap();

    timestamp.to_zoned(crate::time::TIMEZONE)
}

fn set_epoch(epoch_secs: u32) {
    INSTANT_BASE.store(Instant::now().as_secs() as u32, Ordering::Relaxed);
    EPOCH_BASE.store(epoch_secs, Ordering::Release);
}

fn seed_from_rtc(rtc: &Rtc) {
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

async fn sync_with_ntp(
    rtc: &Rtc<'static>,
    stack: &Stack<'static>,
    rx_meta: &mut [PacketMetadata; 16],
    rx_buffer: &mut [u8; 4096],
    tx_meta: &mut [PacketMetadata; 16],
    tx_buffer: &mut [u8; 4096],
) {
    let ntp_addrs = match stack.dns_query(NTP_SERVER, DnsQueryType::A).await {
        Ok(addrs) if !addrs.is_empty() => addrs,
        _ => {
            error!("time: DNS failed");
            return;
        }
    };

    let socket = {
        let mut s = UdpSocket::new(*stack, rx_meta, rx_buffer, tx_meta, tx_buffer);
        s.bind(0).unwrap();
        UdpSocketWrapper::new(s)
    };

    let addr: IpAddr = ntp_addrs[0].into();
    let current_time_us = rtc.current_time_us();
    let result = get_time(
        SocketAddr::from((addr, 123)),
        &socket,
        NtpContext::new(NtpTs { current_time_us }),
    )
    .await;

    match result {
        Ok(time) => {
            let old_epoch = epoch_secs();

            let epoch_us = (u64::from(time.sec()) * USEC_IN_SEC)
                + ((u64::from(time.sec_fraction()) * USEC_IN_SEC) >> 32);

            let new_epoch = (epoch_us / USEC_IN_SEC) as u32;

            let correction = old_epoch.map(|old| i64::from(new_epoch) - i64::from(old));

            rtc.set_current_time_us(epoch_us);
            set_epoch(new_epoch);

            log_sync(correction);
        }
        Err(_e) => {
            error!("time: NTP failed");
        }
    }
}

#[embassy_executor::task]
pub async fn task(rtc: Rtc<'static>, stack: Stack<'static>) {
    seed_from_rtc(&rtc);

    let mut rx_meta = [PacketMetadata::EMPTY; 16];
    let mut rx_buffer = [0; 4096];
    let mut tx_meta = [PacketMetadata::EMPTY; 16];
    let mut tx_buffer = [0; 4096];

    loop {
        stack.wait_config_up().await;
        sync_with_ntp(
            &rtc,
            &stack,
            &mut rx_meta,
            &mut rx_buffer,
            &mut tx_meta,
            &mut tx_buffer,
        )
        .await;

        Timer::after(Duration::from_secs(4 * 3600)).await;
    }
}
