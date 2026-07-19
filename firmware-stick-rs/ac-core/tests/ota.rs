use ac_core::{gh_asset_url, ota_canonical, verify_manifest};
use ed25519_compact::{KeyPair, Seed};

fn test_keys() -> KeyPair {
    KeyPair::from_seed(Seed::new([7u8; 32]))
}

fn b64(data: &[u8]) -> String {
    // Tiny standard-alphabet encoder for tests only.
    const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in data.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(A[(n >> 18) as usize & 63] as char);
        out.push(A[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 { A[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if chunk.len() > 2 { A[n as usize & 63] as char } else { '=' });
    }
    out
}

fn signed_manifest(version: &str, target: &str, size: usize, sha256: &str) -> String {
    let kp = test_keys();
    let msg = ota_canonical(version, target, size, sha256);
    let sig = kp.sk.sign(msg.as_bytes(), None);
    format!(
        "{{\"version\":\"{version}\",\"target\":\"{target}\",\"size\":{size},\
         \"sha256\":\"{sha256}\",\"sig\":\"{}\"}}",
        b64(sig.as_ref())
    )
}

fn pubkey() -> [u8; 32] {
    let mut pk = [0u8; 32];
    pk.copy_from_slice(test_keys().pk.as_ref());
    pk
}

#[test]
fn canonical_string_is_stable() {
    assert_eq!(
        ota_canonical("0.3.11", "m5stickc-plus2", 1234567, "abc123"),
        "condition-control-ota-v1\n0.3.11\nm5stickc-plus2\n1234567\nabc123"
    );
}

#[test]
fn valid_manifest_verifies() {
    let sha = "d".repeat(64);
    let m = verify_manifest(&signed_manifest("0.3.11", "m5stickc-plus2", 999, &sha), &pubkey())
        .unwrap();
    assert_eq!(m.version, "0.3.11");
    assert_eq!(m.target, "m5stickc-plus2");
    assert_eq!(m.size, 999);
    assert_eq!(m.sha256, sha);
}

#[test]
fn tampered_fields_fail_signature() {
    let sha = "d".repeat(64);
    let good = signed_manifest("0.3.11", "m5stickc-plus2", 999, &sha);
    for (from, to) in [
        ("\"size\":999", "\"size\":998"),
        ("0.3.11", "0.3.12"),
        ("m5stickc-plus2", "m5stickc-plusX"),
    ] {
        let bad = good.replace(from, to);
        assert_eq!(verify_manifest(&bad, &pubkey()), Err("manifest: signature invalid"), "{from}");
    }
    // Tampered hash (keeps length): swap first char of the sha value
    let bad = good.replace(&sha, &format!("e{}", "d".repeat(63)));
    assert_eq!(verify_manifest(&bad, &pubkey()), Err("manifest: signature invalid"));
    // Wrong public key
    assert_eq!(
        verify_manifest(&good, &[9u8; 32]),
        Err("manifest: signature invalid")
    );
}

#[test]
fn malformed_manifests_rejected() {
    let sha = "d".repeat(64);
    let good = signed_manifest("0.3.11", "m5stickc-plus2", 999, &sha);
    // Junk base64 in sig
    let junk = good.replace(
        good.split("\"sig\":\"").nth(1).unwrap().split('"').next().unwrap(),
        "!!!",
    );
    assert_eq!(verify_manifest(&junk, &pubkey()), Err("manifest: bad sig encoding"));
    // Missing field
    let missing = good.replace("\"target\":\"m5stickc-plus2\",", "");
    assert_eq!(verify_manifest(&missing, &pubkey()), Err("manifest: bad json"));
    // Not JSON at all
    assert_eq!(verify_manifest("hello", &pubkey()), Err("manifest: bad json"));
}

#[test]
fn asset_url_lookup() {
    let json = r#"{"tag_name":"v0.3.11","assets":[
      {"name":"condition-control.bin","browser_download_url":"https://gh.example/dl/condition-control.bin"},
      {"name":"manifest.json","browser_download_url":"https://gh.example/dl/manifest.json"}]}"#;
    assert_eq!(
        gh_asset_url(json, ".bin").unwrap(),
        "https://gh.example/dl/condition-control.bin"
    );
    assert_eq!(
        gh_asset_url(json, "manifest.json").unwrap(),
        "https://gh.example/dl/manifest.json"
    );
    assert!(gh_asset_url(json, ".sig").is_none());
}

// The security argument rests on one invariant: the signature is verified
// over the RE-EXTRACTED fields, so any parser quirk is self-consistent —
// there is no "parsed one way, enforced another" gap. Pin that down.
#[test]
fn adversarial_manifests_stay_self_consistent() {
    let sha = "d".repeat(64);
    let good = signed_manifest("0.3.11", "m5stickc-plus2", 999, &sha);

    // Duplicate "size" smuggled in front: extraction sees the attacker's
    // value, the canonical string no longer matches the signature.
    let dup = good.replacen('{', "{\"size\":123,", 1);
    assert_eq!(verify_manifest(&dup, &pubkey()), Err("manifest: signature invalid"));

    // Newline injected into a signed field desyncs the canonical string.
    let nl = good.replace("0.3.11", "0.3.\n11");
    assert_eq!(verify_manifest(&nl, &pubkey()), Err("manifest: signature invalid"));

    // Trailing junk after the digits: parser stops at the junk, still
    // resolves to the signed value — lenient but self-consistent.
    let junk = good.replace("\"size\":999", "\"size\":999junk");
    assert_eq!(verify_manifest(&junk, &pubkey()).unwrap().size, 999);

    // Whitespace after the colon is tolerated (hand-written manifests).
    let spaced = good.replace("\"size\":999", "\"size\": 999");
    assert_eq!(verify_manifest(&spaced, &pubkey()).unwrap().size, 999);

    // Truncation anywhere fails closed.
    assert!(verify_manifest(&good[..good.len() / 2], &pubkey()).is_err());
}
