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

use crate::Shared;

const RELEASES_URL: &str =
    "https://api.github.com/repos/mnaza/condition-control/releases/latest";
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
        set_state(shared, "up-to-date");
        return Ok(());
    }

    set_state(shared, &format!("downloading {tag}"));
    http_get(&mut conn, &url)?;
    let mut ota = esp_idf_svc::ota::EspOta::new()?;
    let mut update = ota.initiate_update()?;
    let mut total = 0usize;
    let mut chunk = vec![0u8; 4096];
    let copied = (|| -> Result<usize> {
        loop {
            let n = conn.read(&mut chunk)?;
            if n == 0 {
                break;
            }
            update.write_all(&chunk[..n])?;
            total += n;
            if total % (128 * 1024) < 4096 {
                set_state(shared, &format!("downloading {tag}: {} KB", total / 1024));
            }
        }
        Ok(total)
    })();

    match copied {
        Ok(n) if n > 0 => {
            update.complete()?;
            set_state(shared, "rebooting");
            log::info!("update: {n} bytes flashed, rebooting into {tag}");
            std::thread::sleep(std::time::Duration::from_millis(700));
            esp_idf_svc::hal::reset::restart();
        }
        res => {
            let _ = update.abort();
            bail!("download failed: {:?}", res.err());
        }
    }
}
