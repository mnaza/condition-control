// Pure AC domain logic — no ESP dependencies, unit-tested on the host.
// State strings follow Home Assistant MQTT climate payloads; the IR frame
// follows IRremoteESP8266's ELECTRA_AC layout (ir_Electra.h) plus the
// live-confirmed YKR-L/201E power-off fix (byte 11 = 0x05).

pub const MIN_TEMP: u8 = 16;
pub const MAX_TEMP: u8 = 32;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Auto,
    Cool,
    Dry,
    Fan,
    Heat,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Fan {
    Auto,
    Low,
    Medium,
    High,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct AcState {
    pub power: bool,
    pub mode: Mode,
    pub temp: u8,
    pub fan: Fan,
    pub swing: bool,
}

impl Default for AcState {
    fn default() -> Self {
        AcState { power: false, mode: Mode::Cool, temp: 24, fan: Fan::Auto, swing: false }
    }
}

impl AcState {
    pub fn set_temp(&mut self, t: i32) {
        self.temp = t.clamp(MIN_TEMP as i32, MAX_TEMP as i32) as u8;
    }

    /// "off" while power is down, otherwise the HA mode name.
    pub fn mode_str(&self) -> &'static str {
        if !self.power {
            return "off";
        }
        match self.mode {
            Mode::Auto => "auto",
            Mode::Cool => "cool",
            Mode::Dry => "dry",
            Mode::Fan => "fan_only",
            Mode::Heat => "heat",
        }
    }

    pub fn fan_str(&self) -> &'static str {
        match self.fan {
            Fan::Auto => "auto",
            Fan::Low => "low",
            Fan::Medium => "medium",
            Fan::High => "high",
        }
    }

    /// Applies one web/MQTT parameter. Keys: power=on|off|toggle,
    /// mode=off|auto|cool|dry|fan_only|heat, temp=<int>, fan, swing=on|off.
    /// Returns false (state untouched) on unknown key or bad value.
    pub fn apply(&mut self, key: &str, value: &str) -> bool {
        match key {
            "power" => match value {
                "on" => self.power = true,
                "off" => self.power = false,
                "toggle" => self.power = !self.power,
                _ => return false,
            },
            "mode" => match value {
                "off" => self.power = false, // mode kept, like the C++ version
                "auto" => (self.power, self.mode) = (true, Mode::Auto),
                "cool" => (self.power, self.mode) = (true, Mode::Cool),
                "dry" => (self.power, self.mode) = (true, Mode::Dry),
                "fan_only" => (self.power, self.mode) = (true, Mode::Fan),
                "heat" => (self.power, self.mode) = (true, Mode::Heat),
                _ => return false,
            },
            "temp" => match value.parse::<f32>() {
                Ok(t) => self.set_temp(t.round() as i32),
                Err(_) => return false,
            },
            "fan" => match value {
                "auto" => self.fan = Fan::Auto,
                "low" => self.fan = Fan::Low,
                "medium" => self.fan = Fan::Medium,
                "high" => self.fan = Fan::High,
                _ => return false,
            },
            "swing" => match value {
                "on" => self.swing = true,
                "off" => self.swing = false,
                _ => return false,
            },
            _ => return false,
        }
        true
    }
}

/// The /api/status JSON body. `batt_mv` = 0 means "no reading yet".
pub fn status_json(s: &AcState, wifi: bool, mqtt: bool, off_variant: u8, batt_mv: u16) -> String {
    let pct = battery_percent(batt_mv);
    format!(
        "{{\"power\":{},\"mode\":\"{}\",\"temp\":{},\"fan\":\"{}\",\
         \"swing\":{},\"wifi\":{},\"mqtt\":{},\"offVariant\":{},\
         \"battMv\":{},\"battPct\":{},\"battMin\":{},\"battChg\":{}}}",
        s.power, s.mode_str(), s.temp, s.fan_str(), s.swing, wifi, mqtt, off_variant,
        batt_mv, pct, battery_runtime_min(pct), battery_charging(batt_mv)
    )
}

/// True only while a charger actually pushes the cell above its resting
/// range: a full LiPo rests at 4.15-4.20 V right after unplugging, so the
/// threshold sits above that.
pub fn battery_charging(mv: u16) -> bool {
    mv >= 4230
}

// --- battery ------------------------------------------------------------------

/// Rough LiPo state-of-charge from open-ish-circuit voltage (piecewise
/// linear over a typical 1-cell discharge curve).
pub fn battery_percent(mv: u16) -> u8 {
    const CURVE: [(u16, u8); 9] = [
        (3300, 0),
        (3500, 10),
        (3600, 25),
        (3700, 45),
        (3800, 60),
        (3900, 75),
        (4000, 85),
        (4100, 95),
        (4200, 100),
    ];
    if mv <= CURVE[0].0 {
        return 0;
    }
    if mv >= CURVE[CURVE.len() - 1].0 {
        return 100;
    }
    for w in CURVE.windows(2) {
        let ((v0, p0), (v1, p1)) = (w[0], w[1]);
        if mv < v1 {
            return p0 + ((mv - v0) as u32 * (p1 - p0) as u32 / (v1 - v0) as u32) as u8;
        }
    }
    100
}

/// Ballpark minutes left: 200 mAh StickC Plus2 cell, ~25 mA average draw
/// with power management on (STA + modem sleep, backlight mostly off).
pub fn battery_runtime_min(percent: u8) -> u32 {
    const CAPACITY_MAH: u32 = 200;
    const AVG_DRAW_MA: u32 = 25;
    CAPACITY_MAH * 60 * percent.min(100) as u32 / 100 / AVG_DRAW_MA
}

/// Splits an application/x-www-form-urlencoded body (or URL query string)
/// into decoded key/value pairs. Lenient: bad escapes pass through verbatim,
/// keys without '=' get an empty value.
pub fn form_pairs(s: &str) -> Vec<(String, String)> {
    s.split('&')
        .filter(|kv| !kv.is_empty())
        .map(|kv| {
            let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
            (url_decode(k), url_decode(v))
        })
        .collect()
}

/// Minimal JSON string escaping (quotes, backslashes, control chars) —
/// enough to embed scanned SSIDs and stored settings in hand-built JSON.
pub fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

fn url_decode(s: &str) -> String {
    fn hex(b: u8) -> Option<u8> {
        match b {
            b'0'..=b'9' => Some(b - b'0'),
            b'a'..=b'f' => Some(b - b'a' + 10),
            b'A'..=b'F' => Some(b - b'A' + 10),
            _ => None,
        }
    }
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < bytes.len() => match (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                (Some(hi), Some(lo)) => {
                    out.push(hi << 4 | lo);
                    i += 2;
                }
                _ => out.push(b'%'),
            },
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

// --- ELECTRA_AC frame -------------------------------------------------------

pub const OFF_VARIANT_COUNT: u8 = 4;
/// Live-confirmed on the YKR-L/201E: only byte11=0x05 makes it accept OFF.
pub const OFF_VARIANT_DEFAULT: u8 = 2;

const SWING_ON: u8 = 0b000;
const SWING_OFF: u8 = 0b111;
const TEMP_DELTA: u8 = 8;

fn mode_bits(m: Mode) -> u8 {
    match m {
        Mode::Auto => 0b000,
        Mode::Cool => 0b001,
        Mode::Dry => 0b010,
        Mode::Heat => 0b100,
        Mode::Fan => 0b110,
    }
}

fn fan_bits(f: Fan) -> u8 {
    match f {
        Fan::Auto => 0b101,
        Fan::Low => 0b011,
        Fan::Medium => 0b010,
        Fan::High => 0b001,
    }
}

/// Builds the full 13-byte ELECTRA_AC state frame, checksum included.
/// `off_variant` (0..=3) picks the power-off encoding experiment arm; it only
/// affects frames with power=false (see the Arduino twin in electra_off.h).
pub fn electra_frame(s: &AcState, off_variant: u8) -> [u8; 13] {
    let mut f = [0u8; 13];
    f[0] = 0xC3;
    let temp = s.temp.clamp(MIN_TEMP, MAX_TEMP);
    f[1] = ((temp - TEMP_DELTA) << 3) | if s.swing { SWING_ON } else { SWING_OFF };
    f[2] = SWING_OFF << 5; // horizontal swing not exposed, always off
    f[4] = fan_bits(s.fan) << 5;
    f[6] = mode_bits(s.mode) << 5;
    f[9] = if s.power { 0x20 } else { 0x00 };
    f[11] = 0x08; // library stateReset light byte
    if !s.power {
        match off_variant {
            1 => f[9] |= 0x10,
            2 => f[11] = 0x05,
            3 => {
                f[9] |= 0x10;
                f[11] = 0x05;
            }
            _ => (),
        }
    }
    let sum: u32 = f[..12].iter().map(|&b| b as u32).sum();
    f[12] = (sum & 0xFF) as u8;
    f
}

const HDR_MARK: u32 = 9166;
const HDR_SPACE: u32 = 4470;
const BIT_MARK: u32 = 646;
const ONE_SPACE: u32 = 1647;
const ZERO_SPACE: u32 = 547;

/// Flattens a frame into alternating mark/space durations in µs, starting
/// and ending with a mark (header, 104 LSB-first bits, footer). The 38 kHz
/// carrier is the transmitter's job.
pub fn electra_pulses(frame: &[u8; 13]) -> Vec<u32> {
    let mut p = Vec::with_capacity(2 + frame.len() * 16 + 1);
    p.push(HDR_MARK);
    p.push(HDR_SPACE);
    for &byte in frame {
        for bit in 0..8 {
            p.push(BIT_MARK);
            p.push(if byte >> bit & 1 == 1 { ONE_SPACE } else { ZERO_SPACE });
        }
    }
    p.push(BIT_MARK);
    p
}
