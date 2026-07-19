// Self-update from GitHub releases: fetch the latest release, compare its
// tag with our version, stream the .bin asset into the inactive OTA slot.
// Runs in its own thread (TLS wants real stack); progress lands in
// Shared.update_state for the web UI to poll.
use std::sync::atomic::Ordering;
use std::sync::Arc;

use anyhow::{anyhow, bail, Result};
use esp_idf_svc::http::client::{Configuration, EspHttpConnection};
use esp_idf_svc::http::Method;
use esp_idf_svc::io::Write;
use sha2::{Digest, Sha256};

use crate::Shared;

const RELEASES_URL: &str =
    "https://api.github.com/repos/mnaza/condition-control/releases/latest";
/// Ed25519 public key verifying release manifests (tools/sign-manifest;
/// private half lives only in the OTA_SIGNING_KEY GitHub Actions secret).
/// hex: 8648e69c6374cb598562a2effb647dd69a715bcfde810b1ea18ab70c1c4aca1d
const OTA_PUBKEY: [u8; 32] = [
    0x86, 0x48, 0xe6, 0x9c, 0x63, 0x74, 0xcb, 0x59, 0x85, 0x62, 0xa2, 0xef, 0xfb, 0x64, 0x7d,
    0xd6, 0x9a, 0x71, 0x5b, 0xcf, 0xde, 0x81, 0x0b, 0x1e, 0xa1, 0x8a, 0xb7, 0x0c, 0x1c, 0x4a,
    0xca, 0x1d,
];
const OTA_TARGET: &str = "m5stickc-plus2";
const OTA_SLOT_SIZE: usize = 0x30_0000;
const UA: &[(&str, &str)] = &[
    ("User-Agent", "condition-control-stick"),
    ("Accept", "application/vnd.github+json"),
];

fn set_state(shared: &Shared, s: &str) {
    log::info!("update: {s}");
    *shared.update_state.lock().unwrap() = s.to_string();
}

/// Spawns the updater unless one is already running.
pub fn spawn(shared: Arc<Shared>) {
    if shared.updating.swap(true, Ordering::SeqCst) {
        return;
    }
    let _ = std::thread::Builder::new()
        .name("gh-update".into())
        .stack_size(16 * 1024)
        .spawn(move || {
            let res = run(&shared);
            if let Err(e) = res {
                set_state(&shared, &format!("error: {e}"));
            }
            shared.updating.store(false, Ordering::SeqCst);
        });
}

fn http_get(conn: &mut EspHttpConnection, url: &str) -> Result<()> {
    conn.initiate_request(Method::Get, url, UA)?;
    conn.initiate_response()?;
    let status = conn.status();
    if status != 200 {
        bail!("HTTP {status}");
    }
    Ok(())
}

fn run(shared: &Shared) -> Result<()> {
    set_state(shared, "checking");
    let mut conn = EspHttpConnection::new(&Configuration {
        crt_bundle_attach: Some(esp_idf_svc::sys::esp_crt_bundle_attach),
        buffer_size: Some(4096),
        // Big enough for the redirect to the signed (very long URL)
        // release-asset location — the default 1 KB tx buffer overflows.
        buffer_size_tx: Some(4096),
        timeout: Some(std::time::Duration::from_secs(30)),
        ..Default::default()
    })?;

    http_get(&mut conn, RELEASES_URL)?;
    let mut body = Vec::new();
    let mut buf = [0u8; 1024];
    while body.len() < 32 * 1024 {
        let n = conn.read(&mut buf)?;
        if n == 0 {
            break;
        }
        body.extend_from_slice(&buf[..n]);
    }
    let body = String::from_utf8_lossy(&body);
    let (tag, url) =
        ac_core::gh_release_parse(&body).ok_or_else(|| anyhow!("no release / no .bin asset"))?;

    if !ac_core::version_newer(&tag, env!("CARGO_PKG_VERSION")) {
        // Naming the compared release makes the "pressed the button before
        // CI published" race visible at a glance.
        set_state(shared, &format!("up-to-date ({tag} is latest)"));
        return Ok(());
    }

    // Fail closed: releases without a valid signed manifest never install.
    let manifest_url = ac_core::gh_asset_url(&body, "manifest.json")
        .ok_or_else(|| anyhow!("unsigned release (no manifest)"))?;
    set_state(shared, "verifying manifest");
    http_get(&mut conn, &manifest_url)?;
    let mut mtext = Vec::new();
    while mtext.len() < 4096 {
        let n = conn.read(&mut buf)?;
        if n == 0 {
            break;
        }
        mtext.extend_from_slice(&buf[..n]);
    }
    let manifest = ac_core::verify_manifest(&String::from_utf8_lossy(&mtext), &OTA_PUBKEY)
        .map_err(|e| anyhow!(e))?;
    if manifest.target != OTA_TARGET {
        bail!("manifest: wrong target '{}'", manifest.target);
    }
    if manifest.version.trim_start_matches('v') != tag.trim_start_matches('v') {
        bail!("manifest: version '{}' != release {tag}", manifest.version);
    }
    if manifest.size == 0 || manifest.size > OTA_SLOT_SIZE {
        bail!("manifest: bad size {}", manifest.size);
    }
    set_state(shared, &format!("verified {tag}, downloading"));
    http_get(&mut conn, &url)?;
    let mut ota = esp_idf_svc::ota::EspOta::new()?;
    let mut update = ota.initiate_update()?;
    let mut total = 0usize;
    let mut chunk = vec![0u8; 4096];
    let mut hasher = Sha256::new();
    let copied = (|| -> Result<usize> {
        loop {
            let n = conn.read(&mut chunk)?;
            if n == 0 {
                break;
            }
            if total + n > manifest.size {
                bail!("image larger than manifest size");
            }
            hasher.update(&chunk[..n]);
            update.write_all(&chunk[..n])?;
            total += n;
            if total % (128 * 1024) < 4096 {
                set_state(shared, &format!("downloading {tag}: {} KB", total / 1024));
            }
        }
        if total != manifest.size {
            bail!("size mismatch: got {total}, manifest says {}", manifest.size);
        }
        let digest = format!("{:x}", hasher.finalize_reset());
        if digest != manifest.sha256 {
            bail!("sha256 mismatch");
        }
        Ok(total)
    })();

    match copied {
        Ok(n) if n > 0 => {
            update.complete()?;
            set_state(shared, "rebooting");
            log::info!("update: {n} bytes flashed and verified, rebooting into {tag}");
            std::thread::sleep(std::time::Duration::from_millis(700));
            esp_idf_svc::hal::reset::restart();
        }
        Err(e) => {
            let _ = update.abort();
            bail!("update rejected: {e}");
        }
        Ok(_) => {
            let _ = update.abort();
            bail!("update rejected: empty image");
        }
    }
}
