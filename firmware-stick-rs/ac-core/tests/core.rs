// Host tests for the pure AC core: state, HA payloads, ELECTRA_AC frames.
// Expected bytes mirror IRremoteESP8266's ir_Electra bit layout and the
// live-confirmed YKR-L/201E OFF fix (byte 11 = 0x05).
use ac_core::*;

fn on_cool_24() -> AcState {
    AcState { power: true, mode: Mode::Cool, temp: 24, fan: Fan::Auto, swing: false }
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
    assert_eq!(s.temp, 25);
    assert!(s.apply("temp", "99"));
    assert_eq!(s.temp, MAX_TEMP);
    assert!(s.apply("temp", "1"));
    assert_eq!(s.temp, MIN_TEMP);
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
        status_json(&s, true, false, 2, 3800, false),
        "{\"power\":true,\"mode\":\"cool\",\"temp\":24,\"fan\":\"auto\",\
         \"swing\":false,\"wifi\":true,\"mqtt\":false,\"offVariant\":2,\
         \"battMv\":3800,\"battPct\":60,\"battMin\":288,\"battChg\":false}"
    );
    assert!(status_json(&s, true, false, 2, 4254, true).contains("\"battChg\":true"));
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
        s.temp = temp;
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
