use embassy_net::Stack;
use embassy_net::tcp::TcpSocket;
use embassy_time::{Duration, Timer};
use embedded_io_async::Write;
use rtt_target::rprintln;

#[embassy_executor::task]
pub async fn task(stack: Stack<'static>) -> ! {
    rprintln!("ws: waiting for network...");
    loop {
        if stack.config_v4().is_some() {
            break;
        }
        Timer::after(Duration::from_millis(500)).await;
    }
    rprintln!("ws: network ready");

    loop {
        connect_and_read(stack).await;
        rprintln!("ws: disconnected, retrying in 10s");
        Timer::after(Duration::from_secs(10)).await;
    }
}

async fn connect_and_read(stack: Stack<'_>) {
    let mut rx_buf = [0u8; 4096];
    let mut tx_buf = [0u8; 1024];
    let mut socket = TcpSocket::new(stack, &mut rx_buf, &mut tx_buf);

    let addr = (embassy_net::Ipv4Address::new(192, 168, 1, 13), 3003);

    rprintln!("ws: connecting...");
    if socket.connect(addr).await.is_err() {
        rprintln!("ws: connect failed");
        return;
    }

    rprintln!("ws: connected");
    let request = b"GET /websocket/notifications HTTP/1.1\r\n\
                     Host: 192.168.1.13:3003\r\n\
                     Upgrade: websocket\r\n\
                     Connection: Upgrade\r\n\
                     Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\
                     Sec-WebSocket-Version: 13\r\n\
                     \r\n";

    if socket.write_all(request).await.is_err() {
        rprintln!("ws: send failed");
        return;
    }
    rprintln!("ws: handshake sent");

    let mut buf = [0u8; 512];
    let mut pos = 0;
    loop {
        let n = match socket.read(&mut buf[pos..]).await {
            Ok(0) => break,
            Ok(n) => n,
            Err(_) => {
                rprintln!("ws: read error");
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
    rprintln!("ws: resp = {}", resp);

    if !resp.contains(" 101 ") {
        rprintln!("ws: handshake failed");
        return;
    }
    rprintln!("ws: handshake ok");

    let mut frame = [0u8; 128];
    let n = match embassy_time::with_timeout(Duration::from_secs(10), socket.read(&mut frame)).await
    {
        Ok(Ok(n)) if n > 0 => n,
        _ => {
            rprintln!("ws: frame timeout/error");
            return;
        }
    };

    rprintln!("ws: raw frame ({}) = {:02x?}", n, &frame[..n]);

    if n < 2 {
        rprintln!("ws: frame too short");
        return;
    }

    let opcode = frame[0] & 0x0F;
    let masked = (frame[1] & 0x80) != 0;
    let mut len = (frame[1] & 0x7F) as usize;
    let mut off = 2usize;

    if len == 126 {
        if n < 4 {
            rprintln!("ws: short ext len");
            return;
        }
        len = u16::from_be_bytes([frame[2], frame[3]]) as usize;
        off = 4;
    } else if len == 127 {
        if n < 10 {
            rprintln!("ws: short ext len 64");
            return;
        }
        len = u64::from_be_bytes(frame[2..10].try_into().unwrap()) as usize;
        off = 10;
    }

    if masked {
        off += 4;
    }

    if off + len > n {
        rprintln!("ws: frame truncated (off={} len={} n={})", off, len, n);
        return;
    }

    let payload = &frame[off..off + len];

    match opcode {
        1 => {
            if let Ok(msg) = core::str::from_utf8(payload) {
                rprintln!("ws: msg = {}", msg);
            } else {
                rprintln!("ws: binary = {:02x?}", payload);
            }
        }
        8 => rprintln!("ws: close frame"),
        9 | 10 => rprintln!("ws: ping/pong"),
        _ => rprintln!("ws: unknown opcode {}", opcode),
    }
}
