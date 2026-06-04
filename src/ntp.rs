use core::net::{IpAddr, SocketAddr};

use embassy_net::{
    Stack,
    dns::DnsQueryType,
    udp::{PacketMetadata, UdpSocket},
};
use embassy_time::{Duration, Timer};
use esp_hal::rtc_cntl::Rtc;
use sntpc::{NtpContext, NtpTimestampGenerator, get_time};
use sntpc_net_embassy::UdpSocketWrapper;

use defmt::{error, info};

const NTP_SERVER: &str = "pool.ntp.org";
const USEC_IN_SEC: u64 = 1_000_000;

#[derive(Clone, Copy)]
struct Timestamp {
    current_time_us: u64,
}

impl NtpTimestampGenerator for Timestamp {
    fn init(&mut self) {}

    fn timestamp_sec(&self) -> u64 {
        self.current_time_us / USEC_IN_SEC
    }

    fn timestamp_subsec_micros(&self) -> u32 {
        (self.current_time_us % USEC_IN_SEC) as u32
    }
}

#[embassy_executor::task]
pub async fn task(stack: Stack<'static>, rtc: Rtc<'static>) {
    let mut rx_meta = [PacketMetadata::EMPTY; 16];
    let mut rx_buffer = [0; 4096];
    let mut tx_meta = [PacketMetadata::EMPTY; 16];
    let mut tx_buffer = [0; 4096];

    loop {
        stack.wait_config_up().await;
        info!("NTP: got IP");

        let ntp_addrs = stack.dns_query(NTP_SERVER, DnsQueryType::A).await.unwrap();

        if ntp_addrs.is_empty() {
            error!("Failed to resolve DNS. Empty result");
            Timer::after(Duration::from_secs(10)).await;
            continue;
        }

        let mut socket = UdpSocket::new(
            stack,
            &mut rx_meta,
            &mut rx_buffer,
            &mut tx_meta,
            &mut tx_buffer,
        );

        socket.bind(123).unwrap();
        let socket = UdpSocketWrapper::new(socket);

        loop {
            let current_time_us = rtc.current_time_us();

            let addr: IpAddr = ntp_addrs[0].into();
            let result = get_time(
                SocketAddr::from((addr, 123)),
                &socket,
                NtpContext::new(Timestamp { current_time_us }),
            )
            .await;

            match result {
                Ok(time) => {
                    rtc.set_current_time_us(
                        (time.sec() as u64 * USEC_IN_SEC)
                            + ((time.sec_fraction() as u64 * USEC_IN_SEC) >> 32),
                    );
                    info!("Rtc after update:{}", time.sec());
                }
                Err(_e) => {
                    error!("NTP error");
                    break;
                }
            }

            Timer::after(Duration::from_secs(600)).await;
            info!("NTP tick!");
        }
    }
}
