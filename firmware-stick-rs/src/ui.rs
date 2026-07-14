// ST7789V2 display (135x240 panel, used landscape: 240x135) + the two
// buttons. Layout mirrors the Arduino firmware: ON/OFF top-left, IP
// top-center, WiFi/MQTT badges top-right, big set-temperature in the
// middle, mode/fan/swing along the bottom.
// StickC Plus2 pins: SPI2 SCLK=13 MOSI=15 CS=5, DC=14, RST=12, BL=27;
// BtnA=37 (power toggle), BtnB=39 (temp cycle) — active low.
use ac_core::{AcState, MAX_TEMP, MIN_TEMP};
use anyhow::Result;
use embedded_graphics::mono_font::ascii::{FONT_10X20, FONT_6X10};
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::pixelcolor::{Rgb565, RgbColor, WebColors};
use embedded_graphics::prelude::*;
use embedded_graphics::text::{Alignment, Text};
use esp_idf_svc::hal::delay::Delay;
use esp_idf_svc::hal::gpio::{
    AnyIOPin, Gpio12, Gpio13, Gpio14, Gpio15, Gpio27, Gpio37, Gpio39, Gpio5, Input, Output,
    PinDriver,
};
use esp_idf_svc::hal::spi::config::Config as SpiConfig;
use esp_idf_svc::hal::spi::{SpiDeviceDriver, SpiDriver, SpiDriverConfig, SPI2};
use esp_idf_svc::hal::units::FromValueType;
use mipidsi::interface::SpiInterface;
use mipidsi::models::ST7789;
use mipidsi::options::{ColorInversion, Orientation, Rotation};
use mipidsi::{Builder, Display as MipiDisplay};
use profont::PROFONT_24_POINT;

const W: i32 = 240;
const H: i32 = 135;

type Disp = MipiDisplay<
    SpiInterface<
        'static,
        SpiDeviceDriver<'static, SpiDriver<'static>>,
        PinDriver<'static, Gpio14, Output>,
    >,
    ST7789,
    PinDriver<'static, Gpio12, Output>,
>;

pub struct Ui {
    display: Disp,
    _backlight: PinDriver<'static, Gpio27, Output>,
    btn_a: PinDriver<'static, Gpio37, Input>,
    btn_b: PinDriver<'static, Gpio39, Input>,
    a_was_down: bool,
    b_was_down: bool,
    last_drawn: Option<(AcState, bool, bool, String)>,
}

pub struct Pins {
    pub spi: SPI2,
    pub sclk: Gpio13,
    pub mosi: Gpio15,
    pub cs: Gpio5,
    pub dc: Gpio14,
    pub rst: Gpio12,
    pub backlight: Gpio27,
    pub btn_a: Gpio37,
    pub btn_b: Gpio39,
}

impl Ui {
    pub fn new(p: Pins) -> Result<Self> {
        let driver = SpiDriver::new(p.spi, p.sclk, p.mosi, None::<AnyIOPin>, &SpiDriverConfig::new())?;
        let spi = SpiDeviceDriver::new(driver, Some(p.cs), &SpiConfig::new().baudrate(26.MHz().into()))?;
        let dc = PinDriver::output(p.dc)?;
        let rst = PinDriver::output(p.rst)?;
        let mut backlight = PinDriver::output(p.backlight)?;
        backlight.set_high()?;

        let buf: &'static mut [u8] = Box::leak(Box::new([0u8; 512]));
        let mut delay = Delay::new_default();
        let display = Builder::new(ST7789, SpiInterface::new(spi, dc, buf))
            .display_size(135, 240)
            .display_offset(52, 40)
            .invert_colors(ColorInversion::Inverted)
            .orientation(Orientation::new().rotate(Rotation::Deg90))
            .reset_pin(rst)
            .init(&mut delay)
            .map_err(|e| anyhow::anyhow!("display init: {e:?}"))?;

        Ok(Self {
            display,
            _backlight: backlight,
            btn_a: PinDriver::input(p.btn_a)?,
            btn_b: PinDriver::input(p.btn_b)?,
            a_was_down: false,
            b_was_down: false,
            last_drawn: None,
        })
    }

    /// Polls buttons; mutates s and returns true on a user change.
    /// BtnA toggles power, BtnB cycles the temperature (local fallback).
    pub fn handle_buttons(&mut self, s: &mut AcState) -> bool {
        let mut changed = false;
        let a_down = self.btn_a.is_low();
        if a_down && !self.a_was_down {
            s.power = !s.power;
            changed = true;
        }
        self.a_was_down = a_down;

        let b_down = self.btn_b.is_low();
        if b_down && !self.b_was_down {
            s.temp = if s.temp >= MAX_TEMP { MIN_TEMP } else { s.temp + 1 };
            changed = true;
        }
        self.b_was_down = b_down;
        changed
    }

    /// Redraws only when something visible changed.
    pub fn update(&mut self, s: &AcState, wifi: bool, mqtt: bool, ip: &str) {
        let key = (*s, wifi, mqtt, ip.to_string());
        if self.last_drawn.as_ref() == Some(&key) {
            return;
        }
        self.last_drawn = Some(key);
        let _ = self.draw(s, wifi, mqtt, ip);
    }

    fn draw(&mut self, s: &AcState, wifi: bool, mqtt: bool, ip: &str) -> Result<(), ()> {
        let d = &mut self.display;
        d.clear(Rgb565::BLACK).map_err(|_| ())?;
        let grey = Rgb565::new(12, 25, 12);
        let header = MonoTextStyle::new(&FONT_10X20, if s.power { Rgb565::GREEN } else { grey });
        let small = |color| MonoTextStyle::new(&FONT_6X10, color);
        let huge = MonoTextStyle::new(&PROFONT_24_POINT, if s.power { Rgb565::WHITE } else { grey });

        // Header: power state, web address, link badges.
        Text::new(if s.power { "ON" } else { "OFF" }, Point::new(8, 22), header)
            .draw(d)
            .map_err(|_| ())?;
        if !ip.is_empty() {
            Text::with_alignment(
                ip,
                Point::new(W / 2, 14),
                small(Rgb565::CSS_LIGHT_GRAY),
                Alignment::Center,
            )
            .draw(d)
            .map_err(|_| ())?;
        }
        Text::with_alignment(
            if wifi { "WiFi" } else { "WiFi x" },
            Point::new(W - 8, 14),
            small(if wifi { Rgb565::GREEN } else { Rgb565::RED }),
            Alignment::Right,
        )
        .draw(d)
        .map_err(|_| ())?;
        Text::with_alignment(
            if mqtt { "MQTT" } else { "MQTT x" },
            Point::new(W - 8, 28),
            small(if mqtt { Rgb565::GREEN } else { Rgb565::RED }),
            Alignment::Right,
        )
        .draw(d)
        .map_err(|_| ())?;

        // Big set-temperature in the middle.
        let temp = format!("{}C", s.temp);
        Text::with_alignment(&temp, Point::new(W / 2, H / 2 + 18), huge, Alignment::Center)
            .draw(d)
            .map_err(|_| ())?;

        // Footer: mode / fan / swing.
        Text::new(s.mode_str(), Point::new(8, H - 6), small(Rgb565::CYAN))
            .draw(d)
            .map_err(|_| ())?;
        Text::with_alignment(
            s.fan_str(),
            Point::new(W / 2, H - 6),
            small(Rgb565::YELLOW),
            Alignment::Center,
        )
        .draw(d)
        .map_err(|_| ())?;
        Text::with_alignment(
            if s.swing { "swing" } else { "fixed" },
            Point::new(W - 8, H - 6),
            small(Rgb565::MAGENTA),
            Alignment::Right,
        )
        .draw(d)
        .map_err(|_| ())?;
        Ok(())
    }
}
