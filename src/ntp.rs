use core::cell::Cell;
use critical_section::Mutex;
use embassy_net::Stack;
use embassy_net::udp::{PacketMetadata, UdpSocket};
use embassy_time::{Duration, Instant, Timer};
use rtt_target::rprintln;

#[embassy_executor::task]
pub async fn task(stack: Stack<'static>) -> ! {
    rprintln!("ntp: waiting for network...");
    loop {
        if stack.config_v4().is_some() {
            break;
        }
        Timer::after(Duration::from_millis(500)).await;
    }
    rprintln!("ntp: network ready");

    loop {
        match fetch(stack).await {
            Some(secs) => {
                let now = Instant::now();
                critical_section::with(|cs| {
                    NTP_SECS.borrow(cs).replace(Some(secs));
                    NTP_INSTANT.borrow(cs).replace(Some(now));
                });
                rprintln!("ntp: synced, unix = {}", secs);
            }
            None => rprintln!("ntp: fetch failed"),
        }
        Timer::after(Duration::from_secs(86400)).await;
    }
}

pub fn current_moscow_hms() -> Option<(u8, u8, u8)> {
    let secs = current_unix_secs()?;
    let msk = secs + 3 * 3600;
    let t = msk % 86400;
    Some(((t / 3600) as u8, ((t % 3600) / 60) as u8, (t % 60) as u8))
}

fn current_unix_secs() -> Option<u64> {
    critical_section::with(|cs| {
        let base = NTP_SECS.borrow(cs).get()?;
        let instant = NTP_INSTANT.borrow(cs).get()?;
        let elapsed = Instant::now().saturating_duration_since(instant).as_secs();
        Some(base + elapsed)
    })
}

static NTP_SECS: Mutex<Cell<Option<u64>>> = Mutex::new(Cell::new(None));
static NTP_INSTANT: Mutex<Cell<Option<Instant>>> = Mutex::new(Cell::new(None));

async fn fetch(stack: Stack<'_>) -> Option<u64> {
    rprintln!("ntp: start");

    let mut rx_meta = [PacketMetadata::EMPTY; 1];
    let mut rx_buf = [0u8; 128];
    let mut tx_meta = [PacketMetadata::EMPTY; 1];
    let mut tx_buf = [0u8; 128];

    let mut sock = UdpSocket::new(stack, &mut rx_meta, &mut rx_buf, &mut tx_meta, &mut tx_buf);

    rprintln!("ntp: socket created");

    sock.bind(0).ok()?;
    rprintln!("ntp: bind ok");

    let mut req = [0u8; 48];
    req[0] = 0x1B;

    let ntp_ip = embassy_net::Ipv4Address::new(216, 239, 35, 0);
    rprintln!("ntp: target set");

    let send = sock.send_to(&req, (ntp_ip, 123)).await;
    rprintln!("ntp: send result = {:?}", send.is_ok());
    send.ok()?;

    rprintln!("ntp: waiting response...");

    let mut resp = [0u8; 48];

    let recv = embassy_time::with_timeout(Duration::from_secs(3), sock.recv_from(&mut resp)).await;

    rprintln!("ntp: recv raw = {:?}", recv.is_ok());

    let Ok(Ok((len, _))) = recv else {
        rprintln!("ntp: timeout/fail");
        return None;
    };

    rprintln!("ntp: received len = {}", len);

    if len < 48 {
        rprintln!("ntp: invalid packet size");
        return None;
    }

    let secs = u32::from_be_bytes([resp[40], resp[41], resp[42], resp[43]]);
    rprintln!("ntp: raw secs = {}", secs);

    let unix = secs.wrapping_sub(2_208_988_800) as u64;

    rprintln!("ntp: done");

    Some(unix)
}
