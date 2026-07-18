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
