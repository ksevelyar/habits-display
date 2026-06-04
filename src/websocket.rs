use core::str;

use defmt::{error, info};
use edge_ws::{FrameHeader, FrameType};
use embassy_net::Stack;
use embassy_net::dns::DnsQueryType;
use embassy_net::tcp::TcpSocket;
use embassy_time::{Duration, Timer, with_timeout};
use embedded_io_async::Write;

#[embassy_executor::task]
pub async fn task(stack: Stack<'static>) -> ! {
    stack.wait_config_up().await;
    info!("ws: network ready");

    loop {
        if let Err(()) = run(stack).await {
            error!("ws: disconnected");
        }
        Timer::after(Duration::from_secs(10)).await;
    }
}

async fn run(stack: Stack<'_>) -> Result<(), ()> {
    let mut rx_buf = [0u8; 4096];
    let mut tx_buf = [0u8; 1024];
    let mut socket = TcpSocket::new(stack, &mut rx_buf, &mut tx_buf);
    let host = env!("NOTIFICATIONS_HOST");
    let hostname = host.split(':').next().unwrap_or(host);
    let port: u16 = host
        .split(':')
        .nth(1)
        .and_then(|p| p.parse().ok())
        .unwrap_or(80);

    let ip = if let Ok(v4) = hostname.parse::<embassy_net::Ipv4Address>() {
        embassy_net::IpAddress::Ipv4(v4)
    } else {
        let addrs = with_timeout(
            Duration::from_secs(5),
            stack.dns_query(hostname, DnsQueryType::A),
        )
        .await
        .map_err(|_| error!("ws: dns timeout for {}", hostname))?
        .map_err(|_| error!("ws: dns failed for {}", hostname))?;
        addrs
            .first()
            .copied()
            .ok_or_else(|| error!("ws: no dns results for {}", hostname))?
    };
    let addr = (ip, port);

    socket
        .connect(addr)
        .await
        .map_err(|e| error!("ws: connect: {}", e))?;
    info!("ws: connected");

    handshake(&mut socket).await?;
    info!("ws: handshake ok");

    let mut buf = [0u8; 2048];
    loop {
        let header = FrameHeader::recv(&mut socket)
            .await
            .map_err(|e| error!("ws: recv header: {}", e))?;
        let payload = header
            .recv_payload(&mut socket, &mut buf)
            .await
            .map_err(|e| error!("ws: recv payload: {}", e))?;

        match header.frame_type {
            FrameType::Text(_) | FrameType::Binary(_) => {
                if let Ok(msg) = str::from_utf8(payload) {
                    info!("ws: {}", msg);
                } else {
                    info!("ws: binary = {:02x}", payload);
                }
            }
            FrameType::Close => {
                info!("ws: close");
                return Err(());
            }
            FrameType::Ping => info!("ws: ping"),
            FrameType::Pong => info!("ws: pong"),
            FrameType::Continue(_) => info!("ws: continue"),
        }
    }
}

async fn handshake(socket: &mut TcpSocket<'_>) -> Result<(), ()> {
    let request = concat!(
        "GET /websocket/notifications HTTP/1.1\r\n",
        "Host: ",
        env!("NOTIFICATIONS_HOST"),
        "\r\n",
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

    socket
        .write_all(request)
        .await
        .map_err(|e| error!("ws: handshake send: {}", e))?;
    info!("ws: handshake sent");

    let mut buf = [0u8; 512];
    let mut pos = 0;
    loop {
        let n = socket
            .read(&mut buf[pos..])
            .await
            .map_err(|e| error!("ws: handshake read: {}", e))?;
        if n == 0 {
            error!("ws: handshake eof");
            return Err(());
        }
        pos += n;
        if buf[..pos].windows(4).any(|w| w == b"\r\n\r\n") {
            break;
        }
        if pos >= buf.len() {
            error!("ws: handshake buf full");
            return Err(());
        }
    }

    let resp = str::from_utf8(&buf[..pos]).map_err(|_| error!("ws: handshake utf8"))?;
    if !resp.contains(" 101 ") {
        error!("ws: handshake failed: {}", resp);
        return Err(());
    }
    Ok(())
}
