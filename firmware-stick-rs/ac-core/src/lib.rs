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
    /// Set temperature in HALF degrees (°C × 2, 32..=64) — the UI and HA
    /// work in 0.5° steps; the IR frames round to whole degrees.
    pub temp2: u8,
    pub fan: Fan,
    pub swing: bool,
}

impl Default for AcState {
    fn default() -> Self {
        AcState { power: false, mode: Mode::Cool, temp2: 48, fan: Fan::Auto, swing: false }
    }
}

impl AcState {
    pub fn set_temp_c(&mut self, t: f32) {
        self.temp2 =
            ((t * 2.0).round() as i32).clamp(MIN_TEMP as i32 * 2, MAX_TEMP as i32 * 2) as u8;
    }

    /// Whole degrees for the IR frames (halves round up: 24.5 -> 25).
    pub fn temp_whole(&self) -> u8 {
        self.temp2.div_ceil(2).clamp(MIN_TEMP, MAX_TEMP)
    }

    /// "24" or "24.5" — for JSON bodies and MQTT payloads.
    pub fn temp_str(&self) -> String {
        if self.temp2.is_multiple_of(2) {
            (self.temp2 / 2).to_string()
        } else {
            format!("{}.5", self.temp2 / 2)
        }
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
                Ok(t) => self.set_temp_c(t),
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

/// The IR protocol used to talk to the AC. Electra is the live-confirmed
/// one for the Baxi/AUX YKR-L/201E; Coolix (Midea & many OEMs) and Gree
/// mirror IRremoteESP8266's encoders so the bridge can drive other units.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Protocol {
    #[default]
    Electra,
    Coolix,
    Gree,
}

impl Protocol {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "electra" => Some(Protocol::Electra),
            "coolix" => Some(Protocol::Coolix),
            "gree" => Some(Protocol::Gree),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Protocol::Electra => "electra",
            Protocol::Coolix => "coolix",
            Protocol::Gree => "gree",
        }
    }

    /// NVS form; anything unknown falls back to Electra.
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => Protocol::Coolix,
            2 => Protocol::Gree,
            _ => Protocol::Electra,
        }
    }

    pub fn as_u8(self) -> u8 {
        match self {
            Protocol::Electra => 0,
            Protocol::Coolix => 1,
            Protocol::Gree => 2,
        }
    }
}

/// Full-state pulse train for the selected protocol. `off_variant` only
/// matters for Electra OFF frames.
pub fn ir_pulses(proto: Protocol, s: &AcState, off_variant: u8) -> Vec<u32> {
    match proto {
        Protocol::Electra => electra_pulses(&electra_frame(s, off_variant)),
        Protocol::Coolix => coolix_pulses(coolix_code(s)),
        Protocol::Gree => gree_pulses(&gree_state(s)),
    }
}

/// Coolix has no swing bit in the state — the remote sends a dedicated
/// toggle code. Returns that extra frame when the protocol needs one.
pub fn swing_toggle_pulses(proto: Protocol) -> Option<Vec<u32>> {
    match proto {
        Protocol::Coolix => Some(coolix_pulses(COOLIX_SWING_TOGGLE)),
        _ => None,
    }
}

/// The /api/status JSON body. `batt_mv` = 0 means "no reading yet";
/// `charging` comes from the ChargeDetector.
pub fn status_json(
    s: &AcState,
    wifi: bool,
    mqtt: bool,
    off_variant: u8,
    proto: Protocol,
    batt_mv: u16,
    charging: bool,
) -> String {
    let pct = battery_percent(batt_mv);
    format!(
        "{{\"power\":{},\"mode\":\"{}\",\"temp\":{},\"fan\":\"{}\",\
         \"swing\":{},\"wifi\":{},\"mqtt\":{},\"offVariant\":{},\
         \"proto\":\"{}\",\
         \"battMv\":{},\"battPct\":{},\"battMin\":{},\"battChg\":{}}}",
        s.power, s.mode_str(), s.temp_str(), s.fan_str(), s.swing, wifi, mqtt, off_variant,
        proto.as_str(), batt_mv, pct, battery_runtime_min(pct), charging
    )
}

/// Detects USB plug/unplug from voltage *steps* between samples (~10 s
/// apart) instead of absolute thresholds — this unit's ADC reads a resting
/// full cell at ~4.24 V, so no absolute cut-off can work. Plugging jumps
/// the reading tens of mV up within one sample; unplugging drops it the
/// same way; slow charge/discharge drift never moves that fast.
pub struct ChargeDetector {
    prev: u16,
    charging: bool,
}

const STEP_MV: u16 = 25;
const CHARGER_MIN_MV: u16 = 4150; // a jump below this is a load transient
const BOOT_CHARGING_MV: u16 = 4230; // first-sample guess only

impl ChargeDetector {
    pub fn new(mv: u16) -> Self {
        Self { prev: mv, charging: mv >= BOOT_CHARGING_MV }
    }

    pub fn charging(&self) -> bool {
        self.charging
    }

    /// Feed one periodic reading; returns the current charging verdict.
    pub fn update(&mut self, mv: u16) -> bool {
        if self.prev >= mv + STEP_MV {
            self.charging = false;
        } else if mv >= self.prev + STEP_MV && mv >= CHARGER_MIN_MV {
            self.charging = true;
        }
        if mv < 4000 {
            self.charging = false; // no charger leaves the cell this low
        }
        self.prev = mv;
        self.charging
    }
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

/// Pulls `tag_name` and the first `.bin` asset download URL out of a GitHub
/// "latest release" JSON body. String-scanning on purpose — no JSON dep.
pub fn gh_release_parse(json: &str) -> Option<(String, String)> {
    fn value_after<'a>(s: &'a str, key: &str) -> Option<&'a str> {
        let start = s.find(key)? + key.len();
        let rest = &s[start..];
        let open = rest.find('"')? + 1;
        let close = open + rest[open..].find('"')?;
        Some(&rest[open..close])
    }
    let tag = value_after(json, "\"tag_name\":")?;
    let mut rest = json;
    loop {
        let start = rest.find("\"browser_download_url\":")?;
        let url = value_after(&rest[start..], "\"browser_download_url\":")?;
        if url.ends_with(".bin") {
            return Some((tag.to_string(), url.to_string()));
        }
        rest = &rest[start + 23..];
    }
}

/// True when `remote` (optionally "v"-prefixed) is a strictly newer x.y.z
/// than `local`. Unparsable versions are never "newer".
pub fn version_newer(remote: &str, local: &str) -> bool {
    fn parse(v: &str) -> Option<[u32; 3]> {
        let mut it = v.trim_start_matches('v').split('.');
        let out = [
            it.next()?.parse().ok()?,
            it.next()?.parse().ok()?,
            it.next()?.parse().ok()?,
        ];
        it.next().is_none().then_some(out)
    }
    match (parse(remote), parse(local)) {
        (Some(r), Some(l)) => r > l,
        _ => false,
    }
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

// --- schedule -----------------------------------------------------------------

pub const MAX_RULES: usize = 8;

/// One scheduler rule: at `minute` of day, on the `days` of week (bit 0 =
/// Monday .. bit 6 = Sunday), switch power to `on`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Rule {
    pub enabled: bool,
    pub days: u8,
    pub minute: u16,
    pub on: bool,
}

/// Compact NVS/wire form: "en|days|minute|on;..." e.g. "1|127|450|1".
pub fn schedule_to_string(rules: &[Rule]) -> String {
    rules
        .iter()
        .map(|r| {
            format!("{}|{}|{}|{}", r.enabled as u8, r.days, r.minute, r.on as u8)
        })
        .collect::<Vec<_>>()
        .join(";")
}

/// Lenient parse: malformed or out-of-range chunks are dropped.
pub fn schedule_from_string(s: &str) -> Vec<Rule> {
    s.split(';')
        .filter_map(|chunk| {
            let mut it = chunk.split('|');
            let enabled = it.next()?.parse::<u8>().ok()?;
            let days = it.next()?.parse::<u8>().ok()?;
            let minute = it.next()?.parse::<u16>().ok()?;
            let on = it.next()?.parse::<u8>().ok()?;
            if minute >= 1440 || days > 127 {
                return None;
            }
            Some(Rule { enabled: enabled != 0, days, minute, on: on != 0 })
        })
        .take(MAX_RULES)
        .collect()
}

pub fn schedule_to_json(rules: &[Rule], tz_min: i16) -> String {
    let items: Vec<String> = rules
        .iter()
        .map(|r| {
            format!(
                "{{\"en\":{},\"days\":{},\"min\":{},\"on\":{}}}",
                r.enabled, r.days, r.minute, r.on
            )
        })
        .collect();
    format!("{{\"tz\":{},\"rules\":[{}]}}", tz_min, items.join(","))
}

/// Minute of day and weekday (0 = Monday) for a UTC epoch under a fixed
/// UTC offset in minutes. (Fixed offset: DST shifts are picked up whenever
/// the browser re-saves the schedule.)
pub fn local_minute_weekday(epoch: i64, tz_min: i16) -> (u16, u8) {
    let local = epoch + tz_min as i64 * 60;
    let day = local.div_euclid(86400);
    let minute = (local.rem_euclid(86400) / 60) as u16;
    let weekday = ((day + 3).rem_euclid(7)) as u8; // 1970-01-01 was a Thursday
    (minute, weekday)
}

/// Scans the minute marks in (prev, now] and returns the power action of
/// the latest matching rule, if any. The scan is capped to the trailing
/// 3 hours so a huge clock jump can't stall the loop or replay stale rules.
pub fn schedule_due(rules: &[Rule], prev_epoch: i64, now_epoch: i64, tz_min: i16) -> Option<bool> {
    let end = now_epoch.div_euclid(60);
    let start = (prev_epoch.div_euclid(60) + 1).max(end - 179);
    let mut action = None;
    for m in start..=end {
        let (minute, weekday) = local_minute_weekday(m * 60, tz_min);
        for r in rules {
            if r.enabled && r.minute == minute && r.days >> weekday & 1 == 1 {
                action = Some(r.on);
            }
        }
    }
    action
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
    let temp = s.temp_whole();
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

// --- COOLIX (Midea 24-bit) frame --------------------------------------------
// Mirrors IRremoteESP8266 ir_Coolix: 24-bit code, each byte transmitted
// MSB-first and followed by its bitwise inverse, whole message sent twice.
// Power-off and swing are dedicated codes, not state bits.

const COOLIX_OFF: u32 = 0xB27BE0;
pub const COOLIX_SWING_TOGGLE: u32 = 0xB26BE0;

/// Builds the 24-bit Coolix state code (or the OFF code when power is down).
pub fn coolix_code(s: &AcState) -> u32 {
    if !s.power {
        return COOLIX_OFF;
    }
    // Temperature nibble per 17..30 C (kCoolixTempMap).
    const TEMP_MAP: [u32; 14] =
        [0b0000, 0b0001, 0b0011, 0b0010, 0b0110, 0b0111, 0b0101, 0b0100, 0b1100, 0b1101, 0b1001, 0b1000, 0b1010, 0b1011];
    // (mode bits, fan code used for Fan::Auto, forced temp nibble)
    let (mode, fan_auto, temp_override) = match s.mode {
        Mode::Cool => (0b00, 0b101, None),
        Mode::Dry => (0b01, 0b000, None),
        Mode::Auto => (0b10, 0b000, None),
        Mode::Heat => (0b11, 0b101, None),
        // Fan-only is Dry with the special temp code and a real fan speed.
        Mode::Fan => (0b01, 0b101, Some(0b1110)),
    };
    let temp = temp_override.unwrap_or(TEMP_MAP[(s.temp_whole().clamp(17, 30) - 17) as usize]);
    let fan = match s.fan {
        Fan::Auto => fan_auto,
        Fan::Low => 0b100,
        Fan::Medium => 0b010,
        Fan::High => 0b001,
    };
    // bits 23..16 fixed 0xB2; 15..13 fan; 12..8 sensor temp "ignore" (0x1F);
    // 7..4 temp; 3..2 mode; 1..0 unused.
    0xB2 << 16 | fan << 13 | 0x1F << 8 | temp << 4 | mode << 2
}

const COOLIX_HDR_MARK: u32 = 4692;
const COOLIX_HDR_SPACE: u32 = 4416;
const COOLIX_BIT_MARK: u32 = 552;
const COOLIX_ONE_SPACE: u32 = 1656;
const COOLIX_ZERO_SPACE: u32 = 552;
const COOLIX_GAP: u32 = 5244;

pub fn coolix_pulses(code: u32) -> Vec<u32> {
    let mut p = Vec::with_capacity(2 * (2 + 3 * 2 * 8 * 2 + 2));
    for _ in 0..2 {
        p.push(COOLIX_HDR_MARK);
        p.push(COOLIX_HDR_SPACE);
        for byte_idx in (0..3).rev() {
            let b = (code >> (byte_idx * 8)) as u8;
            for seg in [b, !b] {
                for bit in (0..8).rev() {
                    p.push(COOLIX_BIT_MARK);
                    p.push(if seg >> bit & 1 == 1 { COOLIX_ONE_SPACE } else { COOLIX_ZERO_SPACE });
                }
            }
        }
        p.push(COOLIX_BIT_MARK);
        p.push(COOLIX_GAP);
    }
    p
}

// --- GREE 8-byte frame --------------------------------------------------------
// Mirrors IRremoteESP8266 ir_Gree (YAW1F model): two 4-byte blocks LSB-first,
// separated by a 3-bit 0b010 footer, Kelvinator-style nibble checksum in the
// high nibble of the last byte.

/// Builds the 8-byte Gree state, checksum included.
pub fn gree_state(s: &AcState) -> [u8; 8] {
    let mode = match s.mode {
        Mode::Auto => 0,
        Mode::Cool => 1,
        Mode::Dry => 2,
        Mode::Fan => 3,
        Mode::Heat => 4,
    };
    let fan = match s.fan {
        Fan::Auto => 0u8,
        Fan::Low => 1,
        Fan::Medium => 2,
        Fan::High => 3,
    };
    let mut b = [0u8; 8];
    b[0] = mode | (s.power as u8) << 3 | fan << 4 | (s.swing as u8) << 6;
    b[1] = s.temp_whole().clamp(16, 30) - 16;
    // Light on; bit6 is the YAW1F model marker that tracks the power bit.
    b[2] = 0x20 | if s.power { 0x40 } else { 0 };
    b[3] = 0x50; // fixed 0b0101 pattern
    b[4] = if s.swing { 0b0001 } else { 0b0000 }; // SwingV auto / last position
    b[5] = 0x20; // fixed pattern
    let sum: u8 = 10
        + (b[0] & 0xF)
        + (b[1] & 0xF)
        + (b[2] & 0xF)
        + (b[3] & 0xF)
        + (b[4] >> 4)
        + (b[5] >> 4)
        + (b[6] >> 4);
    b[7] = (sum & 0xF) << 4;
    b
}

const GREE_HDR_MARK: u32 = 9000;
const GREE_HDR_SPACE: u32 = 4500;
const GREE_BIT_MARK: u32 = 620;
const GREE_ONE_SPACE: u32 = 1600;
const GREE_ZERO_SPACE: u32 = 540;
const GREE_MSG_GAP: u32 = 19980;

pub fn gree_pulses(state: &[u8; 8]) -> Vec<u32> {
    let mut p = Vec::with_capacity(2 + 64 * 2 + 3 * 2 + 4);
    let push_bit = |p: &mut Vec<u32>, one: bool| {
        p.push(GREE_BIT_MARK);
        p.push(if one { GREE_ONE_SPACE } else { GREE_ZERO_SPACE });
    };
    p.push(GREE_HDR_MARK);
    p.push(GREE_HDR_SPACE);
    for &byte in &state[..4] {
        for bit in 0..8 {
            push_bit(&mut p, byte >> bit & 1 == 1);
        }
    }
    // Block footer 0b010, LSB-first.
    for bit in [false, true, false] {
        push_bit(&mut p, bit);
    }
    p.push(GREE_BIT_MARK);
    p.push(GREE_MSG_GAP);
    for &byte in &state[4..] {
        for bit in 0..8 {
            push_bit(&mut p, byte >> bit & 1 == 1);
        }
    }
    p.push(GREE_BIT_MARK);
    p.push(GREE_MSG_GAP);
    p
}

// --- Web auth ------------------------------------------------------------------

/// Standard-alphabet base64; padded or unpadded. None on malformed input.
pub fn base64_decode(s: &str) -> Option<Vec<u8>> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3 + 2);
    let mut acc: u32 = 0;
    let mut bits: u8 = 0;
    let mut pad = 0usize;
    for &c in bytes {
        if c == b'=' {
            pad += 1;
            if pad > 2 {
                return None;
            }
            continue;
        }
        if pad > 0 {
            return None; // data after padding
        }
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        };
        acc = (acc << 6) | v as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    if bits >= 6 {
        return None; // a lone extra symbol can't carry a whole byte
    }
    Some(out)
}

/// "Basic <base64(user:pass)>" -> (user, pass). Scheme is case-insensitive.
pub fn parse_basic_auth(header: &str) -> Option<(String, String)> {
    let (scheme, value) = header.trim().split_once(' ')?;
    if !scheme.eq_ignore_ascii_case("basic") {
        return None;
    }
    let text = String::from_utf8(base64_decode(value.trim())?).ok()?;
    let (user, pass) = text.split_once(':')?;
    Some((user.to_string(), pass.to_string()))
}

/// Length leaks; contents don't.
pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |d, (x, y)| d | (x ^ y)) == 0
}

/// Gate for every web handler. Empty `stored` means auth is disabled.
/// The username in the header is ignored; only the password counts.
pub fn check_password(header: Option<&str>, stored: &str) -> bool {
    if stored.is_empty() {
        return true;
    }
    let Some(h) = header else { return false };
    let Some((_, pass)) = parse_basic_auth(h) else { return false };
    constant_time_eq(pass.as_bytes(), stored.as_bytes())
}

/// If-None-Match vs an unquoted entity tag: handles quoted/weak forms,
/// comma lists and `*`. Used for the 304 on the embedded web page.
pub fn if_none_match(header: Option<&str>, etag: &str) -> bool {
    let Some(h) = header else { return false };
    h.split(',').map(str::trim).any(|t| {
        t == "*" || t.strip_prefix("W/").unwrap_or(t) == format!("\"{etag}\"")
    })
}
