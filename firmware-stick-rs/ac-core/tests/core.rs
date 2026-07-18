// Host tests for the pure AC core: state, HA payloads, ELECTRA_AC frames.
// Expected bytes mirror IRremoteESP8266's ir_Electra bit layout and the
// live-confirmed YKR-L/201E OFF fix (byte 11 = 0x05).
use ac_core::*;

fn on_cool_24() -> AcState {
    AcState { power: true, mode: Mode::Cool, temp2: 48, fan: Fan::Auto, swing: false }
}

// --- state / HA payloads ----------------------------------------------------

#[test]
fn apply_mode_sets_power() {
    let mut s = AcState::default();
    assert!(s.apply("mode", "heat"));
    assert!(s.power);
    assert_eq!(s.mode, Mode::Heat);
    assert!(s.apply("mode", "off"));
    assert!(!s.power);
    assert_eq!(s.mode, Mode::Heat); // mode kept, only power cleared
}

#[test]
fn apply_power_temp_fan_swing() {
    let mut s = AcState::default();
    assert!(s.apply("power", "on"));
    assert!(s.power);
    assert!(s.apply("power", "toggle"));
    assert!(!s.power);
    assert!(s.apply("temp", "25"));
    assert_eq!(s.temp2, 50);
    assert!(s.apply("temp", "99"));
    assert_eq!(s.temp2, MAX_TEMP * 2);
    assert!(s.apply("temp", "1"));
    assert_eq!(s.temp2, MIN_TEMP * 2);
    // Half-degree resolution: kept in state/JSON, rounded up for IR.
    assert!(s.apply("temp", "24.5"));
    assert_eq!(s.temp2, 49);
    assert_eq!(s.temp_str(), "24.5");
    assert_eq!(s.temp_whole(), 25);
    assert!(s.apply("temp", "24.3")); // snaps to the nearest half
    assert_eq!(s.temp_str(), "24.5");
    assert!(s.apply("temp", "26"));
    assert_eq!(s.temp_str(), "26");
    assert_eq!(s.temp_whole(), 26);
    assert!(s.apply("fan", "high"));
    assert_eq!(s.fan, Fan::High);
    assert!(s.apply("swing", "on"));
    assert!(s.swing);
}

#[test]
fn apply_rejects_unknown_untouched() {
    let mut s = AcState::default();
    let before = s;
    assert!(!s.apply("bogus", "1"));
    assert!(!s.apply("mode", "bogus"));
    assert!(!s.apply("fan", "bogus"));
    assert!(!s.apply("temp", "abc"));
    assert_eq!(s, before);
}

#[test]
fn ha_strings() {
    let mut s = on_cool_24();
    assert_eq!(s.mode_str(), "cool");
    assert_eq!(s.fan_str(), "auto");
    s.power = false;
    assert_eq!(s.mode_str(), "off");
    s.power = true;
    s.mode = Mode::Fan;
    assert_eq!(s.mode_str(), "fan_only");
}

#[test]
fn status_json_exact() {
    let s = on_cool_24();
    assert_eq!(
        status_json(&s, true, false, 2, Protocol::Electra, 3800, false),
        "{\"power\":true,\"mode\":\"cool\",\"temp\":24,\"fan\":\"auto\",\
         \"swing\":false,\"wifi\":true,\"mqtt\":false,\"offVariant\":2,\
         \"proto\":\"electra\",\
         \"battMv\":3800,\"battPct\":60,\"battMin\":288,\"battChg\":false}"
    );
    let j = status_json(&s, true, false, 2, Protocol::Coolix, 4254, true);
    assert!(j.contains("\"battChg\":true"));
    assert!(j.contains("\"proto\":\"coolix\""));
}

#[test]
fn charge_detector_boot_guess() {
    assert!(ChargeDetector::new(4250).charging()); // docked at boot
    assert!(!ChargeDetector::new(4100).charging());
    assert!(!ChargeDetector::new(3800).charging());
}

#[test]
fn charge_detector_unplug_is_a_drop() {
    let mut d = ChargeDetector::new(4250);
    assert!(d.update(4252)); // still docked, tiny drift
    assert!(d.update(4248));
    assert!(!d.update(4190)); // -58 mV step: cable pulled
    assert!(!d.update(4185)); // stays on battery through slow sag
    assert!(!d.update(4180));
}

#[test]
fn charge_detector_plug_is_a_jump() {
    let mut d = ChargeDetector::new(4180);
    assert!(!d.charging());
    assert!(d.update(4251)); // +71 mV step into charger territory
}

#[test]
fn charge_detector_load_transients_ignored_when_low() {
    // IR-send sag/rebound on a mid battery must not read as a charger.
    let mut d = ChargeDetector::new(3900);
    assert!(!d.update(3870)); // sag under load
    assert!(!d.update(3910)); // rebound +40 but far below charger range
    // Deep-discharge safety: never "charging" below 4 V.
    let mut d2 = ChargeDetector::new(4250);
    assert!(!d2.update(3990));
}

// --- battery ------------------------------------------------------------------

#[test]
fn battery_percent_curve() {
    assert_eq!(battery_percent(4200), 100);
    assert_eq!(battery_percent(5000), 100); // charging / clamped
    assert_eq!(battery_percent(3800), 60);
    assert_eq!(battery_percent(3400), 5); // midway 3300..3500
    assert_eq!(battery_percent(3300), 0);
    assert_eq!(battery_percent(3000), 0);
    assert_eq!(battery_percent(0), 0); // unknown reading
}

#[test]
fn battery_runtime_estimate() {
    assert_eq!(battery_runtime_min(100), 480); // 200 mAh / 25 mA = 8 h
    assert_eq!(battery_runtime_min(50), 240);
    assert_eq!(battery_runtime_min(0), 0);
}

// --- form / query parsing ----------------------------------------------------

#[test]
fn form_pairs_decodes() {
    let pairs = form_pairs("ssid=My+Net%21&pass=p%40ss&empty=");
    assert_eq!(
        pairs,
        vec![
            ("ssid".to_string(), "My Net!".to_string()),
            ("pass".to_string(), "p@ss".to_string()),
            ("empty".to_string(), "".to_string()),
        ]
    );
}

#[test]
fn form_pairs_tolerates_junk() {
    assert_eq!(form_pairs(""), vec![]);
    let pairs = form_pairs("a=%GG&b=1&noeq");
    assert_eq!(pairs[0], ("a".to_string(), "%GG".to_string())); // bad escape kept
    assert_eq!(pairs[1], ("b".to_string(), "1".to_string()));
    assert_eq!(pairs[2], ("noeq".to_string(), "".to_string()));
}

// --- json escaping -----------------------------------------------------------

#[test]
fn json_escape_specials() {
    assert_eq!(json_escape("plain"), "plain");
    assert_eq!(json_escape("a\"b"), "a\\\"b");
    assert_eq!(json_escape("a\\b"), "a\\\\b");
    assert_eq!(json_escape("tab\there"), "tab\\there");
    assert_eq!(json_escape("nl\nhere"), "nl\\nhere");
    assert_eq!(json_escape("ctl\u{1}"), "ctl\\u0001");
    assert_eq!(json_escape("кириллица ok"), "кириллица ok");
}

// --- schedule -----------------------------------------------------------------

// 2026-07-15 00:00 UTC; that day is a Wednesday (weekday 2, Monday = 0).
const WED_MIDNIGHT: i64 = 1784073600;

#[test]
fn schedule_string_roundtrip() {
    let rules = vec![
        Rule { enabled: true, days: 0b1111111, minute: 450, on: true },
        Rule { enabled: false, days: 0b0000011, minute: 1320, on: false },
    ];
    let s = schedule_to_string(&rules);
    assert_eq!(s, "1|127|450|1;0|3|1320|0");
    assert_eq!(schedule_from_string(&s), rules);
}

#[test]
fn schedule_parse_lenient() {
    assert_eq!(schedule_from_string(""), vec![]);
    // Bad chunks skipped, minute clamped rule dropped.
    let rules = schedule_from_string("garbage;1|127|450|1;1|127|9999|1");
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0].minute, 450);
}

#[test]
fn schedule_json_shape() {
    let rules = vec![Rule { enabled: true, days: 127, minute: 450, on: true }];
    assert_eq!(
        schedule_to_json(&rules, 120),
        "{\"tz\":120,\"rules\":[{\"en\":true,\"days\":127,\"min\":450,\"on\":true}]}"
    );
}

#[test]
fn local_minute_weekday_math() {
    assert_eq!(local_minute_weekday(WED_MIDNIGHT, 0), (0, 2));
    assert_eq!(local_minute_weekday(WED_MIDNIGHT, 120), (120, 2)); // 02:00 CEST
    // 23:00 UTC + 2h -> 01:00 Thursday.
    assert_eq!(local_minute_weekday(WED_MIDNIGHT + 23 * 3600, 120), (60, 3));
    // Negative offset crossing back to Tuesday.
    assert_eq!(local_minute_weekday(WED_MIDNIGHT, -60), (23 * 60, 1));
}

#[test]
fn schedule_due_fires_once_in_window() {
    // 07:30 every day, turn on (tz 0).
    let rules = vec![Rule { enabled: true, days: 127, minute: 450, on: true }];
    let fire = WED_MIDNIGHT + 450 * 60;
    assert_eq!(schedule_due(&rules, fire - 30, fire + 30, 0), Some(true));
    assert_eq!(schedule_due(&rules, fire + 30, fire + 90, 0), None); // already past
    assert_eq!(schedule_due(&rules, fire - 90, fire - 30, 0), None); // not yet
}

#[test]
fn schedule_due_respects_days_and_enabled() {
    let fire = WED_MIDNIGHT + 450 * 60;
    let tue_only = vec![Rule { enabled: true, days: 0b0000010, minute: 450, on: true }];
    assert_eq!(schedule_due(&tue_only, fire - 30, fire + 30, 0), None);
    let wed = vec![Rule { enabled: true, days: 0b0000100, minute: 450, on: false }];
    assert_eq!(schedule_due(&wed, fire - 30, fire + 30, 0), Some(false));
    let off = vec![Rule { enabled: false, days: 127, minute: 450, on: true }];
    assert_eq!(schedule_due(&off, fire - 30, fire + 30, 0), None);
}

#[test]
fn schedule_due_latest_wins_and_gap_capped() {
    // Device slept through both 07:00 off and 07:30 on -> latest (on) wins.
    let rules = vec![
        Rule { enabled: true, days: 127, minute: 420, on: false },
        Rule { enabled: true, days: 127, minute: 450, on: true },
    ];
    let base = WED_MIDNIGHT;
    assert_eq!(schedule_due(&rules, base + 400 * 60, base + 460 * 60, 0), Some(true));
    // Absurdly long gap doesn't scan unbounded (cap ~3h): a rule 10h ago is missed.
    assert_eq!(schedule_due(&rules, base, base + 24 * 3600 - 60, 0), None);
}

// --- ELECTRA_AC frame -------------------------------------------------------

#[test]
fn frame_on_cool_24() {
    let f = electra_frame(&on_cool_24(), OFF_VARIANT_DEFAULT);
    assert_eq!(f[0], 0xC3);
    assert_eq!(f[1], 0x87); // temp 24 -> (24-8)<<3, swingV off = 0b111
    assert_eq!(f[2], 0xE0); // swingH off
    assert_eq!(f[4], 0xA0); // fan auto = 0b101 << 5
    assert_eq!(f[6], 0x20); // cool = 0b001 << 5
    assert_eq!(f[9], 0x20); // power on
    assert_eq!(f[11], 0x08); // library light byte on ON frames
    assert_eq!(f[12], 0x12); // checksum
}

#[test]
fn frame_off_uses_confirmed_v3() {
    let mut s = on_cool_24();
    s.power = false;
    let f = electra_frame(&s, OFF_VARIANT_DEFAULT);
    assert_eq!(f[9], 0x00);
    assert_eq!(f[11], 0x05); // the YKR-L/201E OFF fix
    assert_eq!(f[12], 0xEF);
}

#[test]
fn frame_off_variants() {
    let mut s = on_cool_24();
    s.power = false;
    assert_eq!(electra_frame(&s, 0)[11], 0x08); // stock library OFF
    assert_eq!(electra_frame(&s, 1)[9], 0x10);
    let f3 = electra_frame(&s, 3);
    assert_eq!((f3[9], f3[11]), (0x10, 0x05));
    // Variants never leak into ON frames.
    s.power = true;
    let fon = electra_frame(&s, 3);
    assert_eq!((fon[9], fon[11]), (0x20, 0x08));
}

#[test]
fn frame_checksum_is_byte_sum() {
    for temp in MIN_TEMP..=MAX_TEMP {
        let mut s = on_cool_24();
        s.temp2 = temp * 2;
        let f = electra_frame(&s, 2);
        let sum: u32 = f[..12].iter().map(|&b| b as u32).sum();
        assert_eq!(f[12], (sum & 0xFF) as u8);
    }
}

#[test]
fn frame_modes_and_fans() {
    let mut s = on_cool_24();
    s.mode = Mode::Heat;
    assert_eq!(electra_frame(&s, 2)[6], 0x80); // heat = 0b100 << 5
    s.mode = Mode::Auto;
    assert_eq!(electra_frame(&s, 2)[6], 0x00);
    s.mode = Mode::Fan;
    assert_eq!(electra_frame(&s, 2)[6], 0xC0); // fan = 0b110 << 5
    s.fan = Fan::High;
    assert_eq!(electra_frame(&s, 2)[4], 0x20); // high = 0b001 << 5
    s.swing = true;
    assert_eq!(electra_frame(&s, 2)[1] & 0x07, 0x00); // swingV on = 0b000
}

// --- pulse train ------------------------------------------------------------

#[test]
fn pulses_shape() {
    let f = electra_frame(&on_cool_24(), 2);
    let p = electra_pulses(&f);
    // header mark+space, 104 bits x (mark+space), footer mark
    assert_eq!(p.len(), 2 + 13 * 8 * 2 + 1);
    assert_eq!((p[0], p[1]), (9166, 4470));
    assert_eq!(*p.last().unwrap(), 646);
    // 0xC3 sent LSB-first: bits 1,1,0,0,0,0,1,1
    assert_eq!((p[2], p[3]), (646, 1647)); // 1
    assert_eq!((p[4], p[5]), (646, 1647)); // 1
    assert_eq!((p[6], p[7]), (646, 547)); // 0
}

// --- protocol selection -------------------------------------------------------

#[test]
fn protocol_roundtrip() {
    for p in [Protocol::Electra, Protocol::Coolix, Protocol::Gree] {
        assert_eq!(Protocol::parse(p.as_str()), Some(p));
        assert_eq!(Protocol::from_u8(p.as_u8()), p);
    }
    assert_eq!(Protocol::parse("nope"), None);
    assert_eq!(Protocol::from_u8(99), Protocol::Electra); // safe default
    assert_eq!(Protocol::parse("electra"), Some(Protocol::Electra));
}

// --- COOLIX (Midea 24-bit) ----------------------------------------------------
// Reference codes from IRremoteESP8266 ir_Coolix.h.

#[test]
fn coolix_reference_codes() {
    // kCoolixDefaultState: Auto mode, 25C, fan Auto0.
    let s = AcState { power: true, mode: Mode::Auto, temp2: 50, fan: Fan::Auto, swing: false };
    assert_eq!(coolix_code(&s), 0xB21FC8);
    // kCoolixCmdFan: fan-only is a special case of Dry with temp code 0b1110.
    let s = AcState { power: true, mode: Mode::Fan, temp2: 50, fan: Fan::Auto, swing: false };
    assert_eq!(coolix_code(&s), 0xB2BFE4);
    // kCoolixOff: power off is a dedicated code regardless of settings.
    let s = AcState { power: false, mode: Mode::Cool, temp2: 44, fan: Fan::High, swing: true };
    assert_eq!(coolix_code(&s), 0xB27BE0);
    assert_eq!(COOLIX_SWING_TOGGLE, 0xB26BE0);
}

#[test]
fn coolix_mode_temp_fan_bits() {
    // Cool 24C fan max: temp map 24C=0b0100, mode cool=0b00, fan max=0b001.
    let s = AcState { power: true, mode: Mode::Cool, temp2: 48, fan: Fan::High, swing: false };
    assert_eq!(coolix_code(&s), 0xB23F40);
    // Dry 20C: fan forced to Auto0 in dry/auto modes when fan=auto.
    let s = AcState { power: true, mode: Mode::Dry, temp2: 40, fan: Fan::Auto, swing: false };
    assert_eq!(coolix_code(&s), 0xB21F24);
    // Cool clamps 16 -> 17C (map 0b0000) and 32 -> 30C (map 0b1011).
    let s = AcState { power: true, mode: Mode::Cool, temp2: 32, fan: Fan::Low, swing: false };
    assert_eq!(coolix_code(&s) >> 4 & 0xF, 0b0000);
    let s = AcState { power: true, mode: Mode::Cool, temp2: 64, fan: Fan::Low, swing: false };
    assert_eq!(coolix_code(&s) >> 4 & 0xF, 0b1011);
}

#[test]
fn coolix_pulses_shape() {
    let p = coolix_pulses(0xB21FC8);
    // Sent twice (kCoolixDefaultRepeat = 1). Per copy: header pair,
    // 3 bytes x (byte + inverted byte) x 8 bits x (mark+space), footer pair.
    let per_copy = 2 + 3 * 2 * 8 * 2 + 2;
    assert_eq!(p.len(), 2 * per_copy);
    assert_eq!((p[0], p[1]), (4692, 4416));
    // First byte 0xB2 goes MSB-first: bits 1,0,1,1,0,0,1,0.
    assert_eq!((p[2], p[3]), (552, 1656)); // 1
    assert_eq!((p[4], p[5]), (552, 552)); // 0
    assert_eq!((p[6], p[7]), (552, 1656)); // 1
    // Inverted byte 0x4D follows: MSB bit is 0.
    assert_eq!(p[2 + 16 + 1], 552);
    // Copy #1 footer, then copy #2 header.
    assert_eq!((p[per_copy - 2], p[per_copy - 1]), (552, 5244));
    assert_eq!((p[per_copy], p[per_copy + 1]), (4692, 4416));
    assert_eq!(&p[..per_copy], &p[per_copy..]);
}

// --- GREE (8-byte, two blocks) --------------------------------------------------

#[test]
fn gree_reference_reset_state() {
    // IRGreeAC::stateReset(): Power Off, Mode Auto, 25C, Fan Auto ->
    // bytes {0x00, 0x09, 0x20, 0x50, 0x00, 0x20, 0x00} + checksum.
    let s = AcState { power: false, mode: Mode::Auto, temp2: 50, fan: Fan::Auto, swing: false };
    let b = gree_state(&s);
    assert_eq!(&b[..7], &[0x00, 0x09, 0x20, 0x50, 0x00, 0x20, 0x00]);
    // Kelvinator block checksum: 10 + low nibbles b0..b3 + high nibbles b4..b6.
    assert_eq!(b[7], ((10u8 + 9 + 2) & 0xF) << 4);
}

#[test]
fn gree_on_cool_fan_swing() {
    let s = AcState { power: true, mode: Mode::Cool, temp2: 48, fan: Fan::Low, swing: true };
    let b = gree_state(&s);
    // mode 1 | power<<3 | fan 1<<4 | swing auto<<6
    assert_eq!(b[0], 0x59);
    assert_eq!(b[1], 24 - 16);
    assert_eq!(b[2], 0x60); // light + YAW1F power-on model bit
    assert_eq!(b[4], 0x01); // SwingV auto
    assert_eq!(b[7], 0xD0); // (10+9+8+0+0 + 0+2+0) & 0xF = 13
    // Temp clamps to the 16..30 range.
    let s = AcState { power: true, mode: Mode::Cool, temp2: 64, fan: Fan::Low, swing: false };
    assert_eq!(gree_state(&s)[1], 30 - 16);
}

#[test]
fn gree_pulses_shape() {
    let s = AcState { power: false, mode: Mode::Auto, temp2: 50, fan: Fan::Auto, swing: false };
    let b = gree_state(&s);
    let p = gree_pulses(&b);
    // hdr + 32 bits + 3 footer bits + gap pair + 32 bits + gap pair, sent once.
    assert_eq!(p.len(), 2 + 32 * 2 + 3 * 2 + 2 + 32 * 2 + 2);
    assert_eq!((p[0], p[1]), (9000, 4500));
    // Block footer 0b010 LSB-first (0,1,0) right after the first 4 bytes.
    let f = 2 + 32 * 2;
    assert_eq!(&p[f..f + 6], &[620, 540, 620, 1600, 620, 540]);
    assert_eq!((p[f + 6], p[f + 7]), (620, 19980));
    assert_eq!(*p.last().unwrap(), 19980);
}

// --- dispatcher -----------------------------------------------------------------

#[test]
fn ir_pulses_dispatch() {
    let s = on_cool_24();
    assert_eq!(ir_pulses(Protocol::Electra, &s, 2), electra_pulses(&electra_frame(&s, 2)));
    assert_eq!(ir_pulses(Protocol::Coolix, &s, 2), coolix_pulses(coolix_code(&s)));
    assert_eq!(ir_pulses(Protocol::Gree, &s, 2), gree_pulses(&gree_state(&s)));
}

#[test]
fn swing_toggle_only_for_coolix() {
    assert_eq!(swing_toggle_pulses(Protocol::Coolix), Some(coolix_pulses(0xB26BE0)));
    assert_eq!(swing_toggle_pulses(Protocol::Electra), None);
    assert_eq!(swing_toggle_pulses(Protocol::Gree), None);
}

// --- GitHub release update ------------------------------------------------------

#[test]
fn gh_release_parse_extracts_tag_and_bin_url() {
    let json = r#"{"url":"https://api.github.com/...","tag_name":"v0.3.1","name":"v0.3.1",
      "assets":[
        {"name":"notes.txt","browser_download_url":"https://github.com/x/y/releases/download/v0.3.1/notes.txt"},
        {"name":"condition-control.bin","browser_download_url":"https://github.com/x/y/releases/download/v0.3.1/condition-control.bin"}
      ],"body":"notes"}"#;
    let (tag, url) = gh_release_parse(json).unwrap();
    assert_eq!(tag, "v0.3.1");
    assert_eq!(url, "https://github.com/x/y/releases/download/v0.3.1/condition-control.bin");
}

#[test]
fn gh_release_parse_rejects_junk() {
    assert_eq!(gh_release_parse("{}"), None);
    assert_eq!(gh_release_parse(r#"{"tag_name":"v1.0.0","assets":[]}"#), None); // no .bin
    assert_eq!(
        gh_release_parse(r#"{"message":"Not Found","documentation_url":"..."}"#),
        None
    );
}

#[test]
fn version_newer_semver() {
    assert!(version_newer("v0.3.1", "0.3.0"));
    assert!(version_newer("0.3.1", "0.3.0"));
    assert!(version_newer("v1.0.0", "0.9.9"));
    assert!(version_newer("v0.3.10", "0.3.9")); // numeric, not lexicographic
    assert!(!version_newer("v0.3.0", "0.3.0"));
    assert!(!version_newer("v0.2.9", "0.3.0"));
    assert!(!version_newer("garbage", "0.3.0")); // unparsable = not newer
}
