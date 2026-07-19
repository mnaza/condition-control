use ac_core::base64_decode;

#[test]
fn base64_decodes_padded() {
    assert_eq!(base64_decode("dXNlcjpwYXNz").unwrap(), b"user:pass");
    assert_eq!(base64_decode("YQ==").unwrap(), b"a");
    assert_eq!(base64_decode("YWI=").unwrap(), b"ab");
    assert_eq!(base64_decode("").unwrap(), b"");
}

#[test]
fn base64_decodes_unpadded() {
    assert_eq!(base64_decode("YQ").unwrap(), b"a");
    assert_eq!(base64_decode("YWI").unwrap(), b"ab");
}

#[test]
fn base64_rejects_garbage() {
    assert!(base64_decode("Y Q==").is_none()); // space
    assert!(base64_decode("YQ==YQ").is_none()); // data after padding
    assert!(base64_decode("YQ===").is_none()); // 3 pads
    assert!(base64_decode("Y").is_none()); // dangling 6 bits
    assert!(base64_decode("абв").is_none()); // non-ascii
}

use ac_core::{check_password, constant_time_eq, parse_basic_auth};

#[test]
fn parses_basic_auth_header() {
    // "user:pass"
    assert_eq!(
        parse_basic_auth("Basic dXNlcjpwYXNz").unwrap(),
        ("user".to_string(), "pass".to_string())
    );
    // scheme is case-insensitive, surrounding whitespace tolerated
    assert_eq!(parse_basic_auth("  basic dXNlcjpwYXNz ").unwrap().1, "pass");
    // password may itself contain ':' — split at the FIRST colon ("u:pa:ss")
    assert_eq!(parse_basic_auth("Basic dTpwYTpzcw==").unwrap().1, "pa:ss");
    // empty username is fine (curl http://:pw@host) — ":pw"
    assert_eq!(parse_basic_auth("Basic OnB3").unwrap(), ("".into(), "pw".into()));
}

#[test]
fn rejects_bad_auth_headers() {
    assert!(parse_basic_auth("Bearer abcdef").is_none());
    assert!(parse_basic_auth("Basic !!!not-base64!!!").is_none());
    assert!(parse_basic_auth("Basic dXNlcnBhc3M=").is_none()); // no colon
    assert!(parse_basic_auth("Basic").is_none()); // no value at all
    // valid base64 but not UTF-8
    assert!(parse_basic_auth("Basic /w==").is_none());
}

#[test]
fn constant_time_eq_basics() {
    assert!(constant_time_eq(b"abc", b"abc"));
    assert!(!constant_time_eq(b"abc", b"abd"));
    assert!(!constant_time_eq(b"abc", b"abcd"));
    assert!(constant_time_eq(b"", b""));
}

#[test]
fn check_password_rules() {
    // no stored password => everything allowed
    assert!(check_password(None, ""));
    assert!(check_password(Some("Basic dXNlcjpwYXNz"), ""));
    // stored password => header required and must match (username ignored)
    assert!(!check_password(None, "pass"));
    assert!(check_password(Some("Basic dXNlcjpwYXNz"), "pass"));
    assert!(check_password(Some("Basic eDpwYXNz"), "pass")); // "x:pass"
    assert!(!check_password(Some("Basic dXNlcjp3cm9uZw=="), "pass")); // "user:wrong"
    assert!(!check_password(Some("garbage"), "pass"));
    // unicode password round-trips ("u:пароль")
    assert!(check_password(Some("Basic dTrQv9Cw0YDQvtC70Yw="), "пароль"));
}

use ac_core::if_none_match;

#[test]
fn if_none_match_rules() {
    // exact quoted match, weak validator, and wildcard all hit
    assert!(if_none_match(Some("\"0.3.9\""), "0.3.9"));
    assert!(if_none_match(Some("W/\"0.3.9\""), "0.3.9"));
    assert!(if_none_match(Some("*"), "0.3.9"));
    // comma-separated list
    assert!(if_none_match(Some("\"0.3.7\", \"0.3.9\""), "0.3.9"));
    // misses
    assert!(!if_none_match(None, "0.3.9"));
    assert!(!if_none_match(Some("\"0.3.8\""), "0.3.9"));
    assert!(!if_none_match(Some(""), "0.3.9"));
    // unquoted junk doesn't match
    assert!(!if_none_match(Some("0.3.9"), "0.3.9"));
}

use ac_core::same_origin;

#[test]
fn origin_gate_rules() {
    // Non-browser clients send no Origin.
    assert!(same_origin(None, "192.168.200.140"));
    // Same-origin browser requests.
    assert!(same_origin(Some("http://192.168.200.140"), "192.168.200.140"));
    assert!(same_origin(Some("http://AC.local"), "ac.local")); // case-insensitive
    assert!(same_origin(Some("http://x:80"), "x")); // default port either side
    assert!(same_origin(Some("http://x"), "x:80"));
    assert!(same_origin(Some("http://x:8080"), "x:8080"));
    // Cross-origin / malformed → rejected.
    assert!(!same_origin(Some("http://evil.example"), "192.168.200.140"));
    assert!(!same_origin(Some("http://x:8080"), "x"));
    assert!(!same_origin(Some("https://x"), "x")); // device is plain http
    assert!(!same_origin(Some("null"), "x")); // sandboxed iframe
    assert!(!same_origin(Some(""), "x"));
    assert!(!same_origin(Some("http://x"), "")); // no Host to compare against
}

#[test]
fn origin_gate_edge_cases() {
    assert!(!same_origin(Some("http://x:80:80"), "x")); // single :80 strip only
    assert!(same_origin(Some("http://[::1]"), "[::1]:80")); // IPv6 literal
    assert!(!same_origin(Some("http://[::1]"), "[::1]:8080"));
    assert!(same_origin(Some(" http://x "), "x")); // padded header tolerated
}

use ac_core::ap_password;

#[test]
fn ap_password_generation() {
    let pw = ap_password(&[0u8; 10]);
    assert_eq!(pw.len(), 10);
    // Unambiguous alphabet only — never i/l/o/0/1.
    let ok = |c: char| ('a'..='z').contains(&c) && !"ilo".contains(c) || ('2'..='9').contains(&c);
    assert!(pw.chars().all(ok), "{pw}");
    // Deterministic for equal input, distinct for different input.
    assert_eq!(pw, ap_password(&[0u8; 10]));
    assert_ne!(pw, ap_password(&[7u8; 10]));
    // Every byte value maps into the alphabet.
    let all = ap_password(&[255u8; 10]);
    assert!(all.chars().all(ok), "{all}");
}

use ac_core::wifi_qr;

#[test]
fn wifi_qr_matrix() {
    let m = wifi_qr("AC-Remote", "abcd2345ef");
    // Square, a plausible QR version (21 + 4k modules per side).
    let s = m.len();
    assert!(m.iter().all(|row| row.len() == s));
    assert!(s >= 21 && (s - 21) % 4 == 0, "size {s}");
    // Finder patterns: the three corners start with a dark module.
    assert!(m[0][0] && m[0][s - 1] && m[s - 1][0]);
    // Deterministic; different password -> different matrix.
    assert_eq!(m, wifi_qr("AC-Remote", "abcd2345ef"));
    assert_ne!(m, wifi_qr("AC-Remote", "zzzz9999zz"));
}
