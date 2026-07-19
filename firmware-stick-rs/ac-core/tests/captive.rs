use ac_core::dns_captive_response;

fn query(qtype: u8) -> Vec<u8> {
    let mut q = vec![
        0x12, 0x34, // id
        0x01, 0x00, // standard query, RD
        0x00, 0x01, // 1 question
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];
    q.extend_from_slice(b"\x03abc\x07example\x00"); // abc.example
    q.extend_from_slice(&[0x00, qtype, 0x00, 0x01]); // type, class IN
    q
}

#[test]
fn a_query_gets_the_ap_ip() {
    let q = query(1);
    let r = dns_captive_response(&q, [192, 168, 71, 1]).unwrap();
    assert_eq!(&r[0..2], &[0x12, 0x34]); // id echoed
    assert_eq!(&r[2..4], &[0x81, 0x80]); // response, RA, NOERROR
    assert_eq!(&r[4..12], &[0, 1, 0, 1, 0, 0, 0, 0]); // 1 question, 1 answer
    assert_eq!(&r[12..12 + 17], &q[12..]); // question echoed verbatim
    let ans = &r[12 + 17..];
    assert_eq!(&ans[..12], &[0xC0, 0x0C, 0, 1, 0, 1, 0, 0, 0, 60, 0, 4]);
    assert_eq!(&ans[12..], &[192, 168, 71, 1]);
}

#[test]
fn non_a_query_gets_empty_noerror() {
    let r = dns_captive_response(&query(0x1c), [192, 168, 71, 1]).unwrap(); // AAAA
    assert_eq!(&r[4..12], &[0, 1, 0, 0, 0, 0, 0, 0]); // no answers
    assert_eq!(r.len(), 12 + 17); // header + question only
}

#[test]
fn junk_is_dropped() {
    assert!(dns_captive_response(&[0x12, 0x34, 0x01], [1, 1, 1, 1]).is_none()); // truncated
    let mut two_q = query(1);
    two_q[5] = 2; // qdcount 2
    assert!(dns_captive_response(&two_q, [1, 1, 1, 1]).is_none());
    let mut cut = query(1);
    cut.truncate(15); // name runs past the packet
    assert!(dns_captive_response(&cut, [1, 1, 1, 1]).is_none());
}
