use core::str;

use defmt::{Format, error, info};
use edge_ws::{FrameHeader, FrameType, io};
use embassy_net::Stack;
use embassy_net::dns::DnsQueryType;
use embassy_net::tcp::TcpSocket;
use embassy_time::{Duration, TimeoutError, Timer, WithTimeout, with_timeout};
use embedded_io_async::ErrorType;
use embedded_io_async::{Read, Write};
use embedded_tls::{Aes128GcmSha256, TlsConfig, TlsConnection, TlsContext, UnsecureProvider};
use heapless::String;
use rand_core::{CryptoRng, RngCore};

use crate::{AppError, DISPLAY_CHANNEL};

use AppError::{Network, Timeout};

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
    let reconnect_delay = Duration::from_secs(10);
    stack.wait_config_up().await;
    info!("ws: network ready");

    loop {
        if let Err(e) = connect(stack, &mut random_generator).await {
            error!("ws: disconnected: {}", e);
        }
        Timer::after(reconnect_delay).await;
    }
}

async fn resolve_host(
    stack: Stack<'_>,
    hostname: &str,
) -> Result<embassy_net::IpAddress, AppError> {
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
            Network("dns lookup timed out")
        })?
        .map_err(|_| {
            error!("ws: dns failed for {}", hostname);
            Network("dns lookup failed")
        })?;
        addresses.first().copied().ok_or_else(|| {
            error!("ws: no dns results for {}", hostname);
            Network("dns lookup returned no addresses")
        })
    }
}

async fn resolve_endpoint(
    stack: Stack<'_>,
) -> Result<((embassy_net::IpAddress, u16), &'static str, bool), AppError> {
    let host_with_port = env!("NOTIFICATIONS_HOST");
    let (hostname, port_string) = host_with_port
        .split_once(':')
        .unwrap_or((host_with_port, ""));
    let use_tls = env!("USE_TLS") == "true";
    let default_port = if use_tls { 443 } else { 80 };
    let port: u16 = port_string.parse().ok().unwrap_or(default_port);
    let address = resolve_host(stack, hostname).await?;

    Ok(((address, port), hostname, use_tls))
}

async fn connect(
    stack: Stack<'_>,
    random_generator: &mut esp_hal::rng::Rng,
) -> Result<(), AppError> {
    let (endpoint, hostname, use_tls) = resolve_endpoint(stack).await?;

    let mut tcp_read_buffer = [0u8; 4096];
    let mut tcp_write_buffer = [0u8; 1024];
    let mut socket = TcpSocket::new(stack, &mut tcp_read_buffer, &mut tcp_write_buffer);

    let connect = async {
        socket.connect(endpoint).await.map_err(|error| {
            error!("ws: connect: {}", error);
            Network("tcp connection failed")
        })
    };
    with_timeout(Duration::from_secs(15), connect)
        .await
        .map_err(|TimeoutError| {
            error!("ws: tcp connect timed out");
            Timeout
        })??;
    info!("ws: connected");

    if use_tls {
        run_tls_websocket_loop(socket, random_generator, hostname).await
    } else {
        run_websocket_loop(&mut socket, random_generator).await
    }
}

async fn run_tls_websocket_loop(
    socket: TcpSocket<'_>,
    random_generator: &mut esp_hal::rng::Rng,
    hostname: &'static str,
) -> Result<(), AppError> {
    let mut tls_read_buffer = [0u8; 16640];
    let mut tls_write_buffer = [0u8; 16384];
    let config = TlsConfig::new().with_server_name(hostname);
    let provider = UnsecureProvider::new::<Aes128GcmSha256>(RngCrypto(*random_generator));

    let mut tls = TlsConnection::new(socket, &mut tls_read_buffer, &mut tls_write_buffer);
    let handshake = async {
        tls.open(TlsContext::new(&config, provider))
            .await
            .map_err(|e| {
                error!("wss: tls handshake: {}", e);
                Network("tls negotiation failed")
            })
    };
    with_timeout(Duration::from_secs(15), handshake)
        .await
        .map_err(|TimeoutError| {
            error!("ws: tls handshake timed out");
            Timeout
        })??;
    info!("ws: tls established");

    run_websocket_loop(&mut tls, random_generator).await
}

async fn receive_frame<'a, R: Read + Write>(
    stream: &mut R,
    buffer: &'a mut [u8],
) -> Result<(FrameType, &'a [u8]), AppError>
where
    <R as ErrorType>::Error: Format,
{
    let server_ping_interval = Duration::from_secs(30);
    let idle_limit = server_ping_interval * 3;

    let frame = async {
        let header = FrameHeader::recv(&mut *stream).await.map_err(|e| {
            error!("ws: receive header: {}", e);
            Network("reading frame header failed")
        })?;
        let payload = header
            .recv_payload(&mut *stream, buffer)
            .await
            .map_err(|e| {
                error!("ws: receive payload: {}", e);
                Network("reading frame payload failed")
            })?;
        Ok((header.frame_type, payload))
    };

    frame
        .with_timeout(idle_limit)
        .await
        .map_err(|TimeoutError| Timeout)?
}

async fn handle_frame<R: Read + Write>(
    stream: &mut R,
    frame_type: FrameType,
    payload: &[u8],
) -> Result<(), AppError>
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
            Err(Network("server closed the connection"))
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

    let task_name_raw = &message[value_start..value_start + end];
    let mut task_name = String::<256>::new();
    for character in task_name_raw
        .chars()
        .filter(|c| c.is_ascii_graphic() || *c == ' ')
    {
        if task_name.push(character).is_err() {
            break;
        }
    }

    info!("ws: task: {}", task_name);
    Some(task_name)
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
) -> Result<(), AppError>
where
    <R as ErrorType>::Error: Format,
{
    with_timeout(
        Duration::from_secs(15),
        handshake(&mut *stream, random_generator),
    )
    .await
    .map_err(|TimeoutError| {
        error!("ws: handshake timed out");
        Timeout
    })??;
    info!("ws: handshake ok");

    let mut buffer = [0u8; 2048];
    loop {
        let (frame_type, payload) = receive_frame(&mut *stream, &mut buffer).await?;
        handle_frame(&mut *stream, frame_type, payload).await?;
    }
}

async fn write_request_data<R: Write>(stream: &mut R, data: &[u8]) -> Result<(), AppError> {
    stream
        .write_all(data)
        .await
        .map_err(|_| Network("sending handshake request failed"))
}

async fn handshake<R: Read + Write>(
    stream: &mut R,
    random_generator: &mut esp_hal::rng::Rng,
) -> Result<(), AppError>
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

    stream
        .flush()
        .await
        .map_err(|_| Network("sending handshake request failed"))?;
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
                Network("reading handshake response failed")
            })?;
        if bytes_read == 0 {
            error!("ws: handshake eof");
            return Err(Network("handshake response ended early"));
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
            return Err(Network("handshake response exceeded buffer"));
        }
    }

    let response = str::from_utf8(&buffer[..position]).map_err(|_| {
        error!("ws: handshake utf8");
        Network("handshake response is not utf-8")
    })?;
    if !response.starts_with("HTTP/1.1 101") {
        error!("ws: handshake failed: {}", response);
        return Err(Network("handshake response status is not 101"));
    }
    Ok(())
}
