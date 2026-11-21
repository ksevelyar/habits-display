use core::cell::RefCell;
use embassy_sync::blocking_mutex::{Mutex, raw::CriticalSectionRawMutex};
use embassy_sync::channel::Channel;
use embedded_io_async::{ErrorType, Read, Write};
use heapless::Vec;

static SERVER_BYTES: Channel<CriticalSectionRawMutex, u8, 1024> = Channel::new();
static REQUEST: Mutex<CriticalSectionRawMutex, RefCell<Vec<u8, 1024>>> =
    Mutex::new(RefCell::new(Vec::new()));

#[derive(Debug, defmt::Format)]
struct NetworkError;

impl core::fmt::Display for NetworkError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str("simulated network error")
    }
}

impl core::error::Error for NetworkError {}

impl embedded_io_async::Error for NetworkError {
    fn kind(&self) -> embedded_io_async::ErrorKind {
        embedded_io_async::ErrorKind::Other
    }
}

pub fn reset() {
    while SERVER_BYTES.try_receive().is_ok() {}
    REQUEST.lock(|request| request.borrow_mut().clear());
}

pub async fn send_handshake() {
    send_bytes(b"HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\n\r\n").await;
    send_event(r#"{"event":"UserAuthenticated","user":{"id":1,"email":"person@example.com","timezone":"Europe/London"}}"#).await;
}

pub async fn send_event(event: &str) {
    SERVER_BYTES.send(0x81).await;
    SERVER_BYTES
        .send(u8::try_from(event.len()).expect("event fits a short frame"))
        .await;
    send_bytes(event.as_bytes()).await;
}

async fn send_bytes(bytes: &[u8]) {
    for byte in bytes {
        SERVER_BYTES.send(*byte).await;
    }
}

pub fn request() -> Vec<u8, 1024> {
    REQUEST.lock(|request| request.borrow().clone())
}

struct Connection;

impl ErrorType for Connection {
    type Error = NetworkError;
}

impl Read for Connection {
    async fn read(&mut self, buffer: &mut [u8]) -> Result<usize, Self::Error> {
        if buffer.is_empty() {
            return Ok(0);
        }
        buffer[0] = SERVER_BYTES.receive().await;
        let mut count = 1;
        while count < buffer.len() {
            let Ok(byte) = SERVER_BYTES.try_receive() else {
                break;
            };
            buffer[count] = byte;
            count += 1;
        }
        Ok(count)
    }
}

impl Write for Connection {
    async fn write(&mut self, buffer: &[u8]) -> Result<usize, Self::Error> {
        REQUEST.lock(|request| {
            request
                .borrow_mut()
                .extend_from_slice(buffer)
                .expect("request fits");
        });
        Ok(buffer.len())
    }

    async fn flush(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

#[embassy_executor::task]
pub async fn run_websocket_task() {
    super::wifi::WEBSOCKET_CONFIGURATION_READY.wait().await;
    let mut connection = Connection;
    let mut random_generator = esp_hal::rng::Rng::new();
    habits_display::websocket::run_websocket_loop(&mut connection, &mut random_generator)
        .await
        .expect("WebSocket connection remains open");
}
