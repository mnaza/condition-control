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

/// The /api/status JSON body (same shape as the Arduino firmware).
pub fn status_json(s: &AcState, wifi: bool, mqtt: bool, off_variant: u8) -> String {
    format!(
        "{{\"power\":{},\"mode\":\"{}\",\"temp\":{},\"fan\":\"{}\",\
         \"swing\":{},\"wifi\":{},\"mqtt\":{},\"offVariant\":{}}}",
        s.power, s.mode_str(), s.temp, s.fan_str(), s.swing, wifi, mqtt, off_variant
    )
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
