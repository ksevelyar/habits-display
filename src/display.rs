use defmt::error;
use embassy_futures::select::{Either, select};
use embassy_time::{Duration, with_timeout};
use embedded_graphics::{
    draw_target::DrawTarget,
    geometry::{Point, Size},
    mono_font::{MonoTextStyle, ascii::FONT_10X20},
    pixelcolor::Rgb565,
    prelude::*,
    primitives::Rectangle,
    text::{Alignment, Text, TextStyle},
};
use embedded_hal_bus::spi::ExclusiveDevice;
use esp_hal::{
    Blocking,
    delay::Delay,
    gpio::{Level, Output, OutputConfig},
    peripherals::{GPIO4, GPIO6, GPIO7, GPIO8, GPIO9, SPI2},
    spi::{
        Mode,
        master::{Config, Spi},
    },
    time::Rate,
};
use mipidsi::{
    Builder, interface::SpiInterface, models::ST7789, options::ColorInversion,
    options::Orientation, options::Rotation,
};
use static_cell::StaticCell;

use crate::DISPLAY_CHANNEL;

static BUF: StaticCell<[u8; 512]> = StaticCell::new();

#[derive(Clone, Copy, Debug, PartialEq)]
struct Theme {
    background: Rgb565,
    foreground: Rgb565,
}

const DAY_THEME: Theme = Theme {
    background: Rgb565::new(16, 4, 20),
    foreground: Rgb565::new(63, 63, 63),
};

const NIGHT_THEME: Theme = Theme {
    background: Rgb565::new(0, 0, 0),
    foreground: Rgb565::new(18, 12, 2),
};

fn select_theme(hour: u8) -> Theme {
    let (day_start_hour, night_start_hour) = (8, 17);
    if (day_start_hour..night_start_hour).contains(&hour) {
        DAY_THEME
    } else {
        NIGHT_THEME
    }
}

struct DisplayState {
    theme: Theme,
    notification: Option<heapless::String<256>>,
}

impl DisplayState {
    fn new(hour: u8) -> Self {
        Self {
            theme: select_theme(hour),
            notification: None,
        }
    }

    fn update(&mut self, hour: u8, notification: Option<heapless::String<256>>) -> bool {
        let next_theme = select_theme(hour);
        let changed = notification.is_some() || next_theme != self.theme;
        self.theme = next_theme;
        if let Some(notification) = notification {
            self.notification = Some(notification);
        }
        changed && self.notification.is_some()
    }

    fn theme(&self) -> Theme {
        self.theme
    }

    fn notification(&self) -> Option<&str> {
        self.notification.as_deref()
    }
}

struct ScaleView<'a, D>
where
    D: DrawTarget<Color = Rgb565>,
{
    parent: &'a mut D,
    scale: u32,
}

impl<D: DrawTarget<Color = Rgb565>> ScaleView<'_, D> {
    fn scaled_point(&self, point: Point) -> Point {
        let scale = i32::try_from(self.scale).expect("display scale fits in i32");
        Point::new(point.x * scale, point.y * scale)
    }

    fn scaled_pixels(
        origin: Point,
        scale: u32,
        color: Rgb565,
    ) -> impl Iterator<Item = Pixel<Rgb565>> {
        (0..scale * scale).map(move |offset| {
            Pixel(
                origin
                    + Point::new(
                        i32::try_from(offset % scale).expect("horizontal offset fits in i32"),
                        i32::try_from(offset / scale).expect("vertical offset fits in i32"),
                    ),
                color,
            )
        })
    }
}

impl<D: DrawTarget<Color = Rgb565>> DrawTarget for ScaleView<'_, D> {
    type Color = Rgb565;
    type Error = D::Error;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Rgb565>>,
    {
        for pixel in pixels {
            let origin = self.scaled_point(pixel.0);
            for scaled in Self::scaled_pixels(origin, self.scale, pixel.1) {
                self.parent.draw_iter([scaled])?;
            }
        }
        Ok(())
    }

    fn fill_solid(&mut self, area: &Rectangle, color: Rgb565) -> Result<(), Self::Error> {
        let top_left = self.scaled_point(area.top_left);
        let scaled_size = Size::new(area.size.width * self.scale, area.size.height * self.scale);
        self.parent
            .fill_solid(&Rectangle::new(top_left, scaled_size), color)
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

pub fn initialize_spi(
    peripheral: SPI2<'static>,
    clock: GPIO4<'static>,
    data: GPIO6<'static>,
) -> Spi<'static, Blocking> {
    Spi::new(
        peripheral,
        Config::default()
            .with_frequency(Rate::from_khz(40000))
            .with_mode(Mode::_0),
    )
    .expect("initialize display SPI")
    .with_sck(clock)
    .with_mosi(data)
}

#[embassy_executor::task]
pub async fn task(
    spi: Spi<'static, Blocking>,
    cs: GPIO7<'static>,
    dc: GPIO8<'static>,
    rst: GPIO9<'static>,
) {
    let mut display = initialize_display(spi, cs, dc, rst);
    run_display(&mut display).await;
}

pub fn initialize_display(
    spi: Spi<'static, Blocking>,
    cs: GPIO7<'static>,
    dc: GPIO8<'static>,
    rst: GPIO9<'static>,
) -> impl DrawTarget<Color = Rgb565, Error: core::fmt::Debug> {
    let mut delay = Delay::new();

    let cs = Output::new(cs, Level::High, OutputConfig::default());
    let dc = Output::new(dc, Level::Low, OutputConfig::default());
    let rst = Output::new(rst, Level::High, OutputConfig::default());

    let buf: &'static mut [u8; 512] = BUF.init([0u8; 512]);
    let device = ExclusiveDevice::new_no_delay(spi, cs).unwrap();
    let di = SpiInterface::new(device, dc, buf);

    Builder::new(ST7789, di)
        .reset_pin(rst)
        .display_size(240, 300)
        .orientation(Orientation::new().rotate(Rotation::Deg90))
        .invert_colors(ColorInversion::Inverted)
        .init(&mut delay)
        .expect("initialize display")
}

pub async fn run_display<D: DrawTarget<Color = Rgb565>>(display: &mut D)
where
    D::Error: core::fmt::Debug,
{
    run_display_with_observer(display, |_, _, _| {}).await;
}

pub async fn run_display_with_observer<D: DrawTarget<Color = Rgb565>>(
    display: &mut D,
    mut observe: impl FnMut(&str, Rgb565, Rgb565),
) where
    D::Error: core::fmt::Debug,
{
    let hour = crate::time::local_time().hour() as u8;
    let mut state = DisplayState::new(hour);

    display
        .clear(state.theme().background)
        .expect("clear display");

    loop {
        let phase_poll_interval = Duration::from_secs(60);
        let incoming = match with_timeout(
            phase_poll_interval,
            select(DISPLAY_CHANNEL.receive(), crate::time::TIME_CHANGED.wait()),
        )
        .await
        {
            Ok(Either::First(notification)) => Some(notification),
            Ok(Either::Second(())) | Err(_) => None,
        };

        let hour = crate::time::local_time().hour() as u8;
        if !state.update(hour, incoming) {
            continue;
        }

        let Some(task) = state.notification() else {
            continue;
        };

        display
            .clear(state.theme().background)
            .expect("clear display");

        let style = MonoTextStyle::new(&FONT_10X20, state.theme().foreground);
        let text_style = TextStyle::with_alignment(Alignment::Center);

        let mut lines: heapless::Vec<heapless::String<32>, 8> = heapless::Vec::new();
        for word in task.split(' ') {
            let line: heapless::String<32> = word.chars().filter(char::is_ascii_graphic).collect();
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
        let line_count = i32::try_from(lines.len()).expect("line count fits in i32");
        let block_top = (240 - line_count * line_height) / 2;

        let mut view = ScaleView {
            parent: display,
            scale: 2,
        };
        for (index, line) in lines.iter().enumerate() {
            let line_index = i32::try_from(index).expect("line index fits in i32");
            let baseline = (block_top + baseline_offset + line_index * line_height) / 2;
            Text::with_text_style(line, Point::new(75, baseline), style, text_style)
                .draw(&mut view)
                .expect("draw notification");
        }
        observe(task, state.theme().background, state.theme().foreground);
    }
}
