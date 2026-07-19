// OTA release signer. Shares the canonical string with the firmware's
// verifier through ac-core, so the two can never drift apart.
//
//   sign-manifest gen-key
//       secret key (hex) -> stdout, public key (hex) -> stderr
//   sign-manifest sign <firmware.bin> <version> <target>
//       reads OTA_SIGNING_KEY (hex secret key) from env,
//       manifest.json -> stdout
use std::process::ExitCode;

use ed25519_compact::{KeyPair, SecretKey};
use sha2::{Digest, Sha256};

fn hex(data: &[u8]) -> String {
    data.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len() / 2)
        .map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).ok())
        .collect()
}

fn b64(data: &[u8]) -> String {
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

pub fn manifest_for(bin: &[u8], version: &str, target: &str, sk: &SecretKey) -> String {
    let sha256 = hex(&Sha256::digest(bin));
    let size = bin.len();
    let msg = ac_core::ota_canonical(version, target, size, &sha256);
    let sig = sk.sign(msg.as_bytes(), None);
    format!(
        "{{\"version\":\"{version}\",\"target\":\"{target}\",\"size\":{size},\
         \"sha256\":\"{sha256}\",\"sig\":\"{}\"}}",
        b64(sig.as_ref())
    )
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("gen-key") => {
            let kp = KeyPair::generate();
            println!("{}", hex(kp.sk.as_ref()));
            eprintln!("public key: {}", hex(kp.pk.as_ref()));
            ExitCode::SUCCESS
        }
        Some("sign") if args.len() == 5 => {
            let Ok(key_hex) = std::env::var("OTA_SIGNING_KEY") else {
                eprintln!("OTA_SIGNING_KEY not set");
                return ExitCode::FAILURE;
            };
            let sk = match unhex(key_hex.trim()).and_then(|b| SecretKey::from_slice(&b).ok()) {
                Some(sk) => sk,
                None => {
                    eprintln!("OTA_SIGNING_KEY is not a valid hex Ed25519 secret key");
                    return ExitCode::FAILURE;
                }
            };
            let bin = match std::fs::read(&args[2]) {
                Ok(b) => b,
                Err(e) => {
                    eprintln!("cannot read {}: {e}", args[2]);
                    return ExitCode::FAILURE;
                }
            };
            println!("{}", manifest_for(&bin, &args[3], &args[4], &sk));
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("usage: sign-manifest gen-key | sign <bin> <version> <target>");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_compact::Seed;

    #[test]
    fn signer_output_verifies_with_ac_core() {
        let kp = KeyPair::from_seed(Seed::new([9u8; 32]));
        let mut pk = [0u8; 32];
        pk.copy_from_slice(kp.pk.as_ref());
        let bin = b"pretend firmware image".to_vec();

        let manifest = manifest_for(&bin, "1.2.3", "m5stickc-plus2", &kp.sk);
        let m = ac_core::verify_manifest(&manifest, &pk).unwrap();
        assert_eq!(m.size, bin.len());
        assert_eq!(m.version, "1.2.3");

        // A different binary must not verify against the same manifest —
        // and a tampered manifest must fail outright.
        let tampered = manifest.replace("1.2.3", "9.9.9");
        assert!(ac_core::verify_manifest(&tampered, &pk).is_err());
    }
}
