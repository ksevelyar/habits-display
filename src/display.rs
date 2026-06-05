use defmt::error;
use embedded_graphics::{
    geometry::Point,
    mono_font::{MonoTextStyle, ascii::FONT_10X20},
    pixelcolor::Rgb565,
    prelude::*,
    text::{Alignment, Text, TextStyle},
};
use embedded_hal_bus::spi::ExclusiveDevice;
use esp_hal::{
    Blocking,
    delay::Delay,
    gpio::{Level, Output, OutputConfig},
    peripherals::GPIO7,
    peripherals::GPIO8,
    peripherals::GPIO9,
    spi::master::Spi,
};
use heapless::String;
use mipidsi::{
    Builder, interface::SpiInterface, models::ST7789, options::ColorInversion,
    options::Orientation, options::Rotation,
};
use static_cell::StaticCell;

use crate::DISPLAY_CHANNEL;

static BUF: StaticCell<[u8; 512]> = StaticCell::new();

#[embassy_executor::task]
pub async fn task(
    spi: Spi<'static, Blocking>,
    cs: GPIO7<'static>,
    dc: GPIO8<'static>,
    rst: GPIO9<'static>,
) {
    let mut delay = Delay::new();

    let cs = Output::new(cs, Level::High, OutputConfig::default());
    let dc = Output::new(dc, Level::Low, OutputConfig::default());
    let rst = Output::new(rst, Level::High, OutputConfig::default());

    let buf: &'static mut [u8; 512] = BUF.init([0u8; 512]);
    let device = ExclusiveDevice::new_no_delay(spi, cs).unwrap();
    let di = SpiInterface::new(device, dc, buf);

    let mut display = Builder::new(ST7789, di)
        .reset_pin(rst)
        .display_size(240, 320)
        .orientation(Orientation::new().rotate(Rotation::Deg180))
        .invert_colors(ColorInversion::Inverted)
        .init(&mut delay)
        .unwrap();

    let mut msgs: [Option<String<256>>; 3] = [None, None, None];
    let style = MonoTextStyle::new(&FONT_10X20, Rgb565::new(255, 255, 255));
    let text_style = TextStyle::with_alignment(Alignment::Center);

    loop {
        let msg = DISPLAY_CHANNEL.receive().await;

        msgs.rotate_left(1);
        msgs[2] = Some(msg);

        if display.clear(Rgb565::new(0, 0, 0)).is_err() {
            error!("display: clear failed");
        }

        let count = msgs.iter().filter(|item| item.is_some()).count();
        for (index, msg) in msgs.iter().flatten().enumerate() {
            let y_pos = 160 + index as i32 * 40 - (count as i32 - 1) * 20;
            if Text::with_text_style(msg, Point::new(120, y_pos), style, text_style)
                .draw(&mut display)
                .is_err()
            {
                error!("display: draw failed");
            }
        }
    }
}
