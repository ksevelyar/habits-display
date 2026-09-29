use core::convert::Infallible;

use defmt::error;
use embedded_graphics::{
    geometry::{Point, Size},
    primitives::Rectangle,
    draw_target::DrawTarget,
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
use mipidsi::{
    Builder, interface::SpiInterface, models::ST7789, options::ColorInversion,
    options::Orientation, options::Rotation,
};
use static_cell::StaticCell;

use crate::DISPLAY_CHANNEL;

static BUF: StaticCell<[u8; 512]> = StaticCell::new();



struct ScaleView<'a, D>
where
    D: DrawTarget<Color = Rgb565>,
{
    parent: &'a mut D,
    scale: u32,
}

impl<D: DrawTarget<Color = Rgb565>> ScaleView<'_, D> {
    fn scaled_point(&self, point: Point) -> Point {
        Point::new(point.x * self.scale as i32, point.y * self.scale as i32)
    }

    fn scaled_pixels(origin: Point, scale: u32, color: Rgb565) -> impl Iterator<Item = Pixel<Rgb565>> {
        (0..scale * scale).map(move |offset| {
            Pixel(
                origin + Point::new((offset % scale) as i32, (offset / scale) as i32),
                color,
            )
        })
    }
}

impl<D: DrawTarget<Color = Rgb565>> DrawTarget for ScaleView<'_, D> {
    type Color = Rgb565;
    type Error = Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Rgb565>>,
    {
        for pixel in pixels {
            let origin = self.scaled_point(pixel.0);
            for scaled in Self::scaled_pixels(origin, self.scale, pixel.1) {
                let _ = self.parent.draw_iter([scaled]);
            }
        }
        Ok(())
    }

    fn fill_solid(&mut self, area: &Rectangle, color: Rgb565) -> Result<(), Self::Error> {
        let top_left = self.scaled_point(area.top_left);
        let scaled_size = Size::new(area.size.width * self.scale, area.size.height * self.scale);
        let _ = self
            .parent
            .fill_solid(&Rectangle::new(top_left, scaled_size), color);
        Ok(())
    }
}

impl<D: DrawTarget<Color = Rgb565>> Dimensions for ScaleView<'_, D> {
    fn bounding_box(&self) -> Rectangle {
        let box_ = self.parent.bounding_box();
        Rectangle::new(
            self.scaled_point(box_.top_left),
            Size::new(box_.size.width * self.scale, box_.size.height * self.scale),
        )
    }
}

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
        .display_size(240, 300)
        .orientation(Orientation::new().rotate(Rotation::Deg90))
        .invert_colors(ColorInversion::Inverted)
        .init(&mut delay)
        .unwrap();

    let midnight_commander_background = Rgb565::new(0, 0, 20);
    let midnight_commander_foreground = Rgb565::new(31, 52, 9);

    display.clear(midnight_commander_background).unwrap();

    let style = MonoTextStyle::new(&FONT_10X20, midnight_commander_foreground);
    let text_style = TextStyle::with_alignment(Alignment::Center);

    loop {
        let task = DISPLAY_CHANNEL.receive().await;

        display.clear(midnight_commander_background).unwrap();

        let mut lines: heapless::Vec<heapless::String<32>, 8> = heapless::Vec::new();
        for word in task.split(' ') {
            let line: heapless::String<32> =
                word.chars().filter(char::is_ascii_graphic).collect();
            if line.is_empty() {
                continue;
            }
            if lines.push(line).is_err() {
                error!("display: too many words");
                break;
            }
        }

        if lines.is_empty() {
            continue;
        }

        let line_height = 40;
        let baseline_offset = 30;
        let line_count = lines.len() as i32;
        let block_top = (240 - line_count * line_height) / 2;

        let mut view = ScaleView { parent: &mut display, scale: 2 };
        for (index, line) in lines.iter().enumerate() {
            let baseline = (block_top + baseline_offset + index as i32 * line_height) / 2;
            if Text::with_text_style(line, Point::new(75, baseline), style, text_style)
                .draw(&mut view)
                .is_err()
            {
                error!("display: draw failed");
            }
        }
    }
}
