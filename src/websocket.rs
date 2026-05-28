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

async fn process_frame(data: &[u8]) -> Result<(), ()> {
    if data.len() < 2 {
        rprintln!("ws: frame too short");
        return Err(());
    }

    let opcode = data[0] & 0x0F;
    let masked = (data[1] & 0x80) != 0;
    let mut len = (data[1] & 0x7F) as usize;
    let mut off = 2usize;

    if len == 126 {
        if data.len() < 4 {
            rprintln!("ws: short ext len");
            return Err(());
        }
        len = u16::from_be_bytes([data[2], data[3]]) as usize;
        off = 4;
    } else if len == 127 {
        if data.len() < 10 {
            rprintln!("ws: short ext len 64");
            return Err(());
        }
        len = u64::from_be_bytes(data[2..10].try_into().unwrap()) as usize;
        off = 10;
    }

    if masked {
        off += 4;
    }

    if off + len > data.len() {
        rprintln!(
            "ws: frame truncated (off={} len={} n={})",
            off,
            len,
            data.len()
        );
        return Err(());
    }

    let payload = &data[off..off + len];

    match opcode {
        1 => {
            if let Ok(msg) = core::str::from_utf8(payload) {
                rprintln!("ws: msg = {}", msg);
            } else {
                rprintln!("ws: binary = {:02x?}", payload);
            }
        }
        8 => {
            rprintln!("ws: close frame");
            return Err(());
        }
        9 => rprintln!("ws: ping"),
        10 => rprintln!("ws: pong"),
        _ => rprintln!("ws: unknown opcode {}", opcode),
    }

    Ok(())
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

    let request = concat!(
        "GET /websocket/notifications HTTP/1.1\r\n",
        "Host: 192.168.1.13:3003\r\n",
        "Upgrade: websocket\r\n",
        "Connection: Upgrade\r\n",
        "Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n",
        "Sec-WebSocket-Version: 13\r\n",
        "Authorization: Bearer ",
        env!("JWT_TOKEN"),
        "\r\n",
        "\r\n",
    )
    .as_bytes();

    if socket.write_all(request).await.is_err() {
        rprintln!("ws: send failed");
        return;
    }
    rprintln!("ws: handshake sent");

    let mut buf = [0u8; 512];
    let mut pos = 0;
    let mut headers_end = 0;
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
        if let Some(idx) = buf[..pos].windows(4).position(|w| w == b"\r\n\r\n") {
            headers_end = idx + 4;
            break;
        }
        if pos >= buf.len() {
            break;
        }
    }

    let resp = core::str::from_utf8(&buf[..headers_end]).unwrap_or("???");
    rprintln!("ws: resp = {}", resp);

    if !resp.contains(" 101 ") {
        rprintln!("ws: handshake failed");
        return;
    }
    rprintln!("ws: handshake ok");

    let leftover = if pos > headers_end {
        &buf[headers_end..pos]
    } else {
        &[]
    };

    if !leftover.is_empty() && process_frame(leftover).await.is_err() {
        return;
    }

    loop {
        let mut frame_buf = [0u8; 2048];
        let mut frame_pos = 0usize;

        loop {
            match socket.read(&mut frame_buf[frame_pos..]).await {
                Ok(0) => {
                    rprintln!("ws: connection closed");
                    return;
                }
                Ok(n) => frame_pos += n,
                Err(_) => {
                    rprintln!("ws: read error");
                    return;
                }
            }
            if frame_pos > 0 {
                break;
            }
        }

        rprintln!(
            "ws: raw frame ({}) = {:02x?}",
            frame_pos,
            &frame_buf[..frame_pos]
        );

        if process_frame(&frame_buf[..frame_pos]).await.is_err() {
            return;
        }
    }
}
