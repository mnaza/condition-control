// Embedded web UI — same page and endpoints as the Arduino firmware.
// POST bodies are application/x-www-form-urlencoded (the page uses
// URLSearchParams), parsed by ac-core's form_pairs.
use std::sync::atomic::Ordering;
use std::sync::Arc;

use std::sync::Mutex;

use ac_core::{form_pairs, json_escape, status_json, OFF_VARIANT_COUNT};
use anyhow::Result;
use esp_idf_svc::http::server::{Configuration, EspHttpServer, Request};
use esp_idf_svc::http::Method;
use esp_idf_svc::io::Write;
use esp_idf_svc::wifi::{AuthMethod, EspWifi};

use crate::net::Store;
use crate::Shared;

const INDEX_HTML: &str = include_str!("index.html");

fn read_body(req: &mut Request<&mut esp_idf_svc::http::server::EspHttpConnection>) -> String {
    let mut buf = [0u8; 512];
    let mut body = Vec::new();
    while let Ok(n) = req.read(&mut buf) {
        if n == 0 {
            break;
        }
        body.extend_from_slice(&buf[..n]);
        if body.len() > 4096 {
            break;
        }
    }
    String::from_utf8_lossy(&body).into_owned()
}

fn send_json(
    req: Request<&mut esp_idf_svc::http::server::EspHttpConnection>,
    json: &str,
) -> Result<()> {
    let mut resp = req.into_response(200, Some("OK"), &[("Content-Type", "application/json")])?;
    resp.write_all(json.as_bytes())?;
    Ok(())
}

fn status(shared: &Shared) -> String {
    let ac = *shared.ac.lock().unwrap();
    status_json(
        &ac,
        shared.wifi_up.load(Ordering::Relaxed),
        shared.mqtt_up.load(Ordering::Relaxed),
        shared.off_variant.load(Ordering::Relaxed),
    )
}

fn reboot_after_ok(req: Request<&mut esp_idf_svc::http::server::EspHttpConnection>) -> Result<()> {
    let mut resp = req.into_response(200, Some("OK"), &[("Content-Type", "text/plain")])?;
    resp.write_all(b"rebooting")?;
    resp.flush()?;
    drop(resp);
    std::thread::sleep(std::time::Duration::from_millis(500));
    esp_idf_svc::hal::reset::restart();
}

/// Registers all routes. The returned server must stay alive.
pub fn start(
    shared: Arc<Shared>,
    store: Arc<Store>,
    wifi: Arc<Mutex<EspWifi<'static>>>,
) -> Result<EspHttpServer<'static>> {
    let mut server = EspHttpServer::new(&Configuration::default())?;

    server.fn_handler("/api/scan", Method::Get, move |req| -> Result<()> {
        // Blocking survey (~2-3 s); the main loop keeps serving cached WiFi
        // status meanwhile (net::Wifi::poll only try_locks).
        let aps = wifi.lock().unwrap().scan()?;
        // Strongest signal wins per SSID.
        let mut nets: Vec<(String, i8, bool)> = Vec::new();
        for ap in &aps {
            if ap.ssid.is_empty() {
                continue;
            }
            let secured = ap.auth_method.map(|m| m != AuthMethod::None).unwrap_or(false);
            match nets.iter_mut().find(|(ssid, ..)| ssid == ap.ssid.as_str()) {
                Some(n) => {
                    if ap.signal_strength > n.1 {
                        (n.1, n.2) = (ap.signal_strength, secured);
                    }
                }
                None => nets.push((ap.ssid.to_string(), ap.signal_strength, secured)),
            }
        }
        nets.sort_by_key(|n| -(n.1 as i16));
        nets.truncate(20);
        let items: Vec<String> = nets
            .iter()
            .map(|(ssid, rssi, sec)| {
                format!("{{\"ssid\":\"{}\",\"rssi\":{},\"sec\":{}}}", json_escape(ssid), rssi, sec)
            })
            .collect();
        send_json(req, &format!("[{}]", items.join(",")))
    })?;

    server.fn_handler("/", Method::Get, |req| -> Result<()> {
        let mut resp =
            req.into_response(200, Some("OK"), &[("Content-Type", "text/html; charset=utf-8")])?;
        resp.write_all(INDEX_HTML.as_bytes())?;
        Ok(())
    })?;

    let sh = shared.clone();
    server.fn_handler("/api/status", Method::Get, move |req| -> Result<()> {
        send_json(req, &status(&sh))
    })?;

    let sh = shared.clone();
    server.fn_handler("/api/set", Method::Get, move |req| -> Result<()> {
        let query = req.uri().split_once('?').map(|(_, q)| q.to_string()).unwrap_or_default();
        {
            let mut ac = sh.ac.lock().unwrap();
            let before = *ac;
            let mut applied = false;
            for (k, v) in form_pairs(&query) {
                applied |= ac.apply(&k, &v);
            }
            if applied && before != *ac {
                sh.dirty.store(true, Ordering::Relaxed);
            }
        }
        send_json(req, &status(&sh))
    })?;

    let sh = shared.clone();
    let st = store.clone();
    server.fn_handler("/api/offvariant", Method::Get, move |req| -> Result<()> {
        let query = req.uri().split_once('?').map(|(_, q)| q.to_string()).unwrap_or_default();
        if let Some((_, v)) = form_pairs(&query).into_iter().find(|(k, _)| k == "v") {
            if let Ok(v) = v.parse::<u8>() {
                if v < OFF_VARIANT_COUNT {
                    sh.off_variant.store(v, Ordering::Relaxed);
                    let _ = st.save_off_variant(v);
                }
            }
        }
        send_json(req, &status(&sh))
    })?;

    let st = store.clone();
    server.fn_handler("/api/wifi", Method::Post, move |mut req| -> Result<()> {
        let body = read_body(&mut req);
        let pairs = form_pairs(&body);
        let get = |key: &str| {
            pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone()).unwrap_or_default()
        };
        let ssid = get("ssid");
        if ssid.is_empty() {
            req.into_status_response(400)?.write_all(b"ssid required")?;
            return Ok(());
        }
        st.save_wifi(&ssid, &get("pass"))?;
        reboot_after_ok(req)
    })?;

    let st = store.clone();
    server.fn_handler("/api/mqtt", Method::Get, move |req| -> Result<()> {
        let s = st.load();
        let json = format!(
            "{{\"host\":\"{}\",\"port\":{},\"user\":\"{}\"}}",
            s.mqtt_host, s.mqtt_port, s.mqtt_user
        );
        send_json(req, &json)
    })?;

    let st = store;
    server.fn_handler("/api/mqtt", Method::Post, move |mut req| -> Result<()> {
        let body = read_body(&mut req);
        let pairs = form_pairs(&body);
        let get = |key: &str| {
            pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone()).unwrap_or_default()
        };
        let port_arg = get("port");
        let port = if port_arg.is_empty() {
            1883
        } else {
            match port_arg.parse::<u16>() {
                Ok(p) if p > 0 => p,
                _ => {
                    req.into_status_response(400)?.write_all(b"bad port")?;
                    return Ok(());
                }
            }
        };
        st.save_mqtt(&get("host"), port, &get("user"), &get("pass"))?;
        reboot_after_ok(req)
    })?;

    Ok(server)
}
