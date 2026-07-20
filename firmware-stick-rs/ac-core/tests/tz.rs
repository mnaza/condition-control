use ac_core::tz_offset_min;

const KYIV: &str = "EET-2EEST,M3.5.0/3,M10.5.0/4";

#[test]
fn ukraine_across_2026_transitions() {
    assert_eq!(tz_offset_min(KYIV, 1774745940), Some(120)); // 2026-03-29 00:59Z, still EET
    assert_eq!(tz_offset_min(KYIV, 1774746000), Some(180)); // 01:00Z = 03:00 local, EEST
    assert_eq!(tz_offset_min(KYIV, 1792889940), Some(180)); // 2026-10-25 00:59Z, still EEST
    assert_eq!(tz_offset_min(KYIV, 1792890000), Some(120)); // 04:00 local, back to EET
}

#[test]
fn cet_with_default_transition_time() {
    let cet = "CET-1CEST,M3.5.0,M10.5.0/3";
    assert_eq!(tz_offset_min(cet, 1768046400), Some(60)); // 2026-01-10, CET
    assert_eq!(tz_offset_min(cet, 1782907200), Some(120)); // 2026-07-01, CEST
}

#[test]
fn fixed_offsets() {
    assert_eq!(tz_offset_min("<+02>-2", 1768046400), Some(120));
    assert_eq!(tz_offset_min("UTC0", 1768046400), Some(0));
    assert_eq!(tz_offset_min("<-03>3", 1768046400), Some(-180));
    assert_eq!(tz_offset_min("<+0530>-5:30", 1768046400), Some(330)); // India
}

#[test]
fn southern_hemisphere() {
    let syd = "AEST-10AEDT,M10.1.0,M4.1.0/3";
    assert_eq!(tz_offset_min(syd, 1768435200), Some(660)); // January = summer = AEDT
    assert_eq!(tz_offset_min(syd, 1784073600), Some(600)); // July = winter = AEST
}

#[test]
fn malformed_rules_are_none() {
    for bad in ["", "EET", "EET-2EEST", "EET-25", "EET-2EEST,J60,M10.5.0", "M3.5.0", "<+02-2"] {
        assert_eq!(tz_offset_min(bad, 1768046400), None, "{bad}");
    }
}

use ac_core::{schedule_due, Rule};

#[test]
fn schedule_follows_dst_rule() {
    // 04:30 local rule on the spring-forward day in Kyiv: 04:30 EEST = 01:30 UTC.
    let rules = [Rule { enabled: true, days: 0x7f, minute: 270, on: true }];
    let fire = 1774746000 + 30 * 60; // 2026-03-29 01:30 UTC
    assert_eq!(schedule_due(&rules, fire - 60, fire + 60, 0, KYIV), Some(true));
    // With only the stale fixed offset (+120 = pre-DST), the same window misses.
    assert_eq!(schedule_due(&rules, fire - 60, fire + 60, 120, ""), None);
}

#[test]
fn browser_generator_output_parses() {
    // Exact string the web UI's tzRule() emits for Europe/Kyiv.
    let gen = "<+02>-2<+03>-3,M3.5.0/3,M10.5.0/4";
    assert_eq!(tz_offset_min(gen, 1768046400), Some(120)); // January
    assert_eq!(tz_offset_min(gen, 1782907200), Some(180)); // July
    // And for a no-DST zone (India).
    assert_eq!(tz_offset_min("<+0530>-5:30", 1782907200), Some(330));
}
