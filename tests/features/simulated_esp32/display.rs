use core::cell::RefCell;
use embassy_sync::blocking_mutex::{Mutex, raw::CriticalSectionRawMutex};
use embassy_sync::signal::Signal;
use embedded_graphics::pixelcolor::Rgb565;
use esp_hal::Blocking;
use esp_hal::peripherals::{GPIO7, GPIO8, GPIO9};
use esp_hal::spi::master::Spi;

#[derive(Clone)]
pub struct Frame {
    pub number: u32,
    pub notification: heapless::String<256>,
    pub background: Rgb565,
    pub foreground: Rgb565,
}

static FRAME: Mutex<CriticalSectionRawMutex, RefCell<Option<Frame>>> =
    Mutex::new(RefCell::new(None));
static CHANGED: Signal<CriticalSectionRawMutex, ()> = Signal::new();

pub fn reset() {
    FRAME.lock(|frame| *frame.borrow_mut() = None);
    CHANGED.reset();
}

pub async fn wait_for_frame(after: u32) -> Frame {
    loop {
        if let Some(frame) = FRAME.lock(|frame| frame.borrow().clone())
            && frame.number > after
        {
            embassy_time::Timer::after_secs(1).await;
            return frame;
        }
        CHANGED.wait().await;
    }
}

#[embassy_executor::task]
pub async fn run_display_task(
    spi: Spi<'static, Blocking>,
    chip_select: GPIO7<'static>,
    data_command: GPIO8<'static>,
    reset: GPIO9<'static>,
) {
    let mut display =
        habits_display::display::initialize_display(spi, chip_select, data_command, reset);
    habits_display::display::run_display_with_observer(
        &mut display,
        |notification, background, foreground| {
            FRAME.lock(|frame| {
                let mut frame = frame.borrow_mut();
                let number = frame.as_ref().map_or(1, |previous| previous.number + 1);
                *frame = Some(Frame {
                    number,
                    notification: heapless::String::try_from(notification)
                        .expect("notification fits"),
                    background,
                    foreground,
                });
            });
            CHANGED.signal(());
        },
    )
    .await;
}
