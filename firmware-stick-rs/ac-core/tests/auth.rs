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
