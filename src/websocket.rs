use core::str;

use defmt::{Format, error, info};
use edge_ws::{FrameHeader, FrameType, io};
use embassy_net::Stack;
use embassy_net::dns::DnsQueryType;
use embassy_net::tcp::TcpSocket;
use embassy_time::{Duration, Timer, with_timeout};
use embedded_io_async::ErrorType;
use embedded_io_async::{Read, Write};
use heapless::String;
use rand_core::{CryptoRng, RngCore};

use crate::DISPLAY_CHANNEL;

#[derive(Format)]
enum Error {
    Dns,
    Connect,
    Tls,
    Handshake,
    Protocol,
    Close,
}

#[derive(Clone, Copy)]
struct RngCrypto(esp_hal::rng::Rng);

impl RngCore for RngCrypto {
    fn next_u32(&mut self) -> u32 {
        self.0.next_u32()
    }

    fn next_u64(&mut self) -> u64 {
        self.0.next_u64()
    }

    fn fill_bytes(&mut self, destination: &mut [u8]) {
        self.0.fill_bytes(destination);
    }

    fn try_fill_bytes(&mut self, destination: &mut [u8]) -> Result<(), rand_core::Error> {
        self.0.try_fill_bytes(destination)
    }
}

impl CryptoRng for RngCrypto {}

#[embassy_executor::task]
pub async fn task(stack: Stack<'static>, mut random_generator: esp_hal::rng::Rng) -> ! {
    stack.wait_config_up().await;
    info!("ws: network ready");

    loop {
        if let Err(e) = connect(stack, &mut random_generator).await {
            error!("ws: disconnected: {}", e);
        }
        Timer::after(Duration::from_secs(10)).await;
    }
}

async fn resolve_host(stack: Stack<'_>, hostname: &str) -> Result<embassy_net::IpAddress, Error> {
    if let Ok(ipv4) = hostname.parse::<embassy_net::Ipv4Address>() {
        Ok(embassy_net::IpAddress::Ipv4(ipv4))
    } else {
        let addresses = with_timeout(
            Duration::from_secs(5),
            stack.dns_query(hostname, DnsQueryType::A),
        )
        .await
        .map_err(|_| {
            error!("ws: dns timeout for {}", hostname);
            Error::Dns
        })?
        .map_err(|_| {
            error!("ws: dns failed for {}", hostname);
            Error::Dns
        })?;
        addresses.first().copied().ok_or_else(|| {
            error!("ws: no dns results for {}", hostname);
            Error::Dns
        })
    }
}

async fn connect(stack: Stack<'_>, random_generator: &mut esp_hal::rng::Rng) -> Result<(), Error> {
    let host_with_port = env!("NOTIFICATIONS_HOST");
    let mut segments = host_with_port.split(':');
    let hostname = segments.next().unwrap_or(host_with_port);
    let port: u16 = segments
        .next()
        .and_then(|port_string| port_string.parse().ok())
        .unwrap_or(80);

    let address = resolve_host(stack, hostname).await?;
    let endpoint = (address, port);

    let mut tcp_read_buffer = [0u8; 4096];
    let mut tcp_write_buffer = [0u8; 1024];
    let mut socket = TcpSocket::new(stack, &mut tcp_read_buffer, &mut tcp_write_buffer);

    socket.connect(endpoint).await.map_err(|error| {
        error!("ws: connect: {}", error);
        Error::Connect
    })?;
    info!("ws: connected");

    let use_tls = env!("USE_TLS") == "true";
    if use_tls {
        use embedded_tls::{
            Aes128GcmSha256, TlsConfig, TlsConnection, TlsContext, UnsecureProvider,
        };

        let mut tls_read_buffer = [0u8; 16640];
        let mut tls_write_buffer = [0u8; 16384];
        let config = TlsConfig::new().with_server_name(hostname);
        let provider = UnsecureProvider::new::<Aes128GcmSha256>(RngCrypto(*random_generator));

        let mut tls = TlsConnection::new(socket, &mut tls_read_buffer, &mut tls_write_buffer);
        tls.open(TlsContext::new(&config, provider))
            .await
            .map_err(|e| {
                error!("wss: tls handshake: {}", e);
                Error::Tls
            })?;
        info!("ws: tls established");

        run_websocket_loop(&mut tls, random_generator).await
    } else {
        run_websocket_loop(&mut socket, random_generator).await
    }
}

async fn receive_frame<'a, R: Read + Write>(
    stream: &mut R,
    buffer: &'a mut [u8],
) -> Result<(FrameType, &'a [u8]), Error>
where
    <R as ErrorType>::Error: Format,
{
    let header = FrameHeader::recv(&mut *stream).await.map_err(|e| {
        error!("ws: receive header: {}", e);
        Error::Protocol
    })?;
    let payload = header
        .recv_payload(&mut *stream, buffer)
        .await
        .map_err(|e| {
            error!("ws: receive payload: {}", e);
            Error::Protocol
        })?;
    Ok((header.frame_type, payload))
}

async fn handle_frame<R: Read + Write>(
    stream: &mut R,
    frame_type: FrameType,
    payload: &[u8],
) -> Result<(), Error>
where
    <R as ErrorType>::Error: Format,
{
    match frame_type {
        FrameType::Text(_) | FrameType::Binary(_) => {
            if let Some(task_name) = parse_notification(payload)
                && let Err(msg) = DISPLAY_CHANNEL.try_send(task_name)
            {
                error!("ws: channel full: {}", msg);
            }
            Ok(())
        }
        FrameType::Close => {
            info!("ws: close");
            Err(Error::Close)
        }
        FrameType::Ping => {
            info!("ws: ping");
            if let Err(e) = io::send(&mut *stream, FrameType::Pong, Some(0), payload).await {
                error!("ws: send pong: {}", e);
            }
            if let Err(e) = stream.flush().await {
                error!("ws: flush pong: {}", e);
            }
            Ok(())
        }
        FrameType::Pong => {
            info!("ws: pong");
            Ok(())
        }
        FrameType::Continue(_) => {
            info!("ws: continue");
            Ok(())
        }
    }
}

fn parse_notification(payload: &[u8]) -> Option<String<256>> {
    let Ok(message) = str::from_utf8(payload) else {
        return None;
    };

    let prefix = "\"task_name\":\"";
    let start = message.find(prefix)?;
    let value_start = start + prefix.len();
    let end = message[value_start..].find('"')?;

    let task_name = &message[value_start..value_start + end];

    info!("ws: task: {}", task_name);
    String::try_from(task_name).ok()
}

fn encode_websocket_key(input: &[u8; 16]) -> [u8; 24] {
    const BASE64_ALPHABET: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = [0u8; 24];
    for (index, chunk) in input.chunks(3).enumerate() {
        let byte_0 = u32::from(chunk[0]);
        let byte_1 = u32::from(chunk.get(1).copied().unwrap_or(0));
        let byte_2 = u32::from(chunk.get(2).copied().unwrap_or(0));
        let triple = (byte_0 << 16) | (byte_1 << 8) | byte_2;
        let output_offset = index * 4;
        output[output_offset] = BASE64_ALPHABET[((triple >> 18) & 0x3F) as usize];
        output[output_offset + 1] = BASE64_ALPHABET[((triple >> 12) & 0x3F) as usize];
        output[output_offset + 2] = if chunk.len() > 1 {
            BASE64_ALPHABET[((triple >> 6) & 0x3F) as usize]
        } else {
            b'='
        };
        output[output_offset + 3] = if chunk.len() > 2 {
            BASE64_ALPHABET[(triple & 0x3F) as usize]
        } else {
            b'='
        };
    }
    output
}

async fn run_websocket_loop<R: Read + Write>(
    stream: &mut R,
    random_generator: &mut esp_hal::rng::Rng,
) -> Result<(), Error>
where
    <R as ErrorType>::Error: Format,
{
    handshake(&mut *stream, random_generator).await?;
    info!("ws: handshake ok");

    let mut buffer = [0u8; 2048];
    loop {
        let (frame_type, payload) = receive_frame(&mut *stream, &mut buffer).await?;
        handle_frame(&mut *stream, frame_type, payload).await?;
    }
}

async fn write_request_data<R: Write>(stream: &mut R, data: &[u8]) -> Result<(), Error> {
    stream.write_all(data).await.map_err(|_| Error::Handshake)
}

async fn handshake<R: Read + Write>(
    stream: &mut R,
    random_generator: &mut esp_hal::rng::Rng,
) -> Result<(), Error>
where
    <R as ErrorType>::Error: Format,
{
    let mut websocket_key = [0u8; 16];
    random_generator.fill_bytes(&mut websocket_key);
    let websocket_key_base64 = encode_websocket_key(&websocket_key);

    write_request_data(stream, b"GET /websocket/notifications HTTP/1.1\r\n").await?;
    write_request_data(stream, b"Host: ").await?;
    write_request_data(stream, env!("NOTIFICATIONS_HOST").as_bytes()).await?;
    write_request_data(stream, b"\r\n").await?;
    write_request_data(stream, b"Upgrade: websocket\r\n").await?;
    write_request_data(stream, b"Connection: Upgrade\r\n").await?;
    write_request_data(stream, b"Sec-WebSocket-Key: ").await?;
    write_request_data(stream, &websocket_key_base64).await?;
    write_request_data(stream, b"\r\n").await?;
    write_request_data(stream, b"Sec-WebSocket-Version: 13\r\n").await?;
    write_request_data(stream, b"Authorization: Bearer ").await?;
    write_request_data(stream, env!("JWT_TOKEN").as_bytes()).await?;
    write_request_data(stream, b"\r\n\r\n").await?;

    stream.flush().await.map_err(|_| Error::Handshake)?;
    info!("ws: handshake sent");

    let mut buffer = [0u8; 512];
    let mut position = 0;
    let mut search_from = 0;
    loop {
        let bytes_read = stream
            .read(&mut buffer[position..])
            .await
            .map_err(|error| {
                error!("ws: handshake read: {}", error);
                Error::Handshake
            })?;
        if bytes_read == 0 {
            error!("ws: handshake eof");
            return Err(Error::Handshake);
        }
        position += bytes_read;
        if buffer[search_from..position]
            .windows(4)
            .any(|w| w == b"\r\n\r\n")
        {
            break;
        }
        search_from = position.saturating_sub(3);
        if position >= buffer.len() {
            error!("ws: handshake buf full");
            return Err(Error::Handshake);
        }
    }

    let response = str::from_utf8(&buffer[..position]).map_err(|_| {
        error!("ws: handshake utf8");
        Error::Handshake
    })?;
    if !response.contains(" 101 ") {
        error!("ws: handshake failed: {}", response);
        return Err(Error::Handshake);
    }
    Ok(())
}
