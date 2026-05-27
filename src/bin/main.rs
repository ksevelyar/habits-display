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

use embassy_net::tcp::TcpSocket;
use embassy_net::udp::{PacketMetadata, UdpSocket};
use embassy_net::{Config as NetConfig, StackResources};
use embedded_io_async::Write;

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

async fn websocket_connect(stack: embassy_net::Stack<'_>) {
    let mut rx_buf = [0u8; 4096];
    let mut tx_buf = [0u8; 1024];
    let mut socket = TcpSocket::new(stack, &mut rx_buf, &mut tx_buf);

    let addr = (embassy_net::Ipv4Address::new(192, 168, 1, 13), 3003);

    rprintln!("WS: connecting...");
    if socket.connect(addr).await.is_err() {
        rprintln!("WS: connect failed");
        return;
    }

    rprintln!("WS: connected");
    let request = b"GET /websocket/notifications HTTP/1.1\r\n\
                     Host: 192.168.1.13:3003\r\n\
                     Upgrade: websocket\r\n\
                     Connection: Upgrade\r\n\
                     Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\
                     Sec-WebSocket-Version: 13\r\n\
                     \r\n";

    if socket.write_all(request).await.is_err() {
        rprintln!("WS: send failed");
        return;
    }
    rprintln!("WS: handshake sent");

    // read HTTP response until \r\n\r\n
    let mut buf = [0u8; 512];
    let mut pos = 0;
    loop {
        let n = match socket.read(&mut buf[pos..]).await {
            Ok(0) => break,
            Ok(n) => n,
            Err(_) => {
                rprintln!("WS: read error");
                return;
            }
        };
        pos += n;
        if pos >= 4 && buf[pos - 4..pos] == *b"\r\n\r\n" {
            break;
        }
        if pos >= buf.len() {
            break;
        }
    }

    let resp = core::str::from_utf8(&buf[..pos]).unwrap_or("???");
    rprintln!("WS: resp = {}", resp);

    if !resp.contains(" 101 ") {
        rprintln!("WS: handshake failed");
        return;
    }
    rprintln!("WS: handshake ok");

    // read first WebSocket frame
    let mut frame = [0u8; 128];
    let n = match embassy_time::with_timeout(Duration::from_secs(10), socket.read(&mut frame)).await
    {
        Ok(Ok(n)) if n > 0 => n,
        _ => {
            rprintln!("WS: frame timeout/error");
            return;
        }
    };

    rprintln!("WS: raw frame ({}) = {:02x?}", n, &frame[..n]);

    if n < 2 {
        rprintln!("WS: frame too short");
        return;
    }

    let opcode = frame[0] & 0x0F;
    let masked = (frame[1] & 0x80) != 0;
    let mut len = (frame[1] & 0x7F) as usize;
    let mut off = 2usize;

    if len == 126 {
        if n < 4 {
            rprintln!("WS: short ext len");
            return;
        }
        len = u16::from_be_bytes([frame[2], frame[3]]) as usize;
        off = 4;
    } else if len == 127 {
        if n < 10 {
            rprintln!("WS: short ext len 64");
            return;
        }
        len = u64::from_be_bytes(frame[2..10].try_into().unwrap()) as usize;
        off = 10;
    }

    if masked {
        off += 4;
    }

    if off + len > n {
        rprintln!("WS: frame truncated (off={} len={} n={})", off, len, n);
        return;
    }

    let payload = &frame[off..off + len];

    match opcode {
        1 => {
            if let Ok(msg) = core::str::from_utf8(payload) {
                rprintln!("WS: msg = {}", msg);
            } else {
                rprintln!("WS: binary = {:02x?}", payload);
            }
        }
        8 => rprintln!("WS: close frame"),
        9 | 10 => rprintln!("WS: ping/pong"),
        _ => rprintln!("WS: unknown opcode {}", opcode),
    }
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
    let mut ws_done = false;

    loop {
        if let Some(cfg) = stack.config_v4() {
            rprintln!("IP: {}", cfg.address);

            if !ntp_done {
                rprintln!("starting NTP...");
                cached = get_ntp_time(stack).await;
                rprintln!("ntp result: {:?}", cached);
                ntp_done = true;
            }

            if !ws_done && ntp_done {
                websocket_connect(stack).await;
                ws_done = true;
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
