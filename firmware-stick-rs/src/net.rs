// Wi-Fi (STA with AP fallback), NVS-backed settings and MQTT with Home
// Assistant discovery. NVS namespaces/keys match the Arduino firmware, so
// settings saved by either firmware carry over to the other.
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use ac_core::OFF_VARIANT_DEFAULT;
use anyhow::{anyhow, Result};
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::modem::Modem;
use esp_idf_svc::mqtt::client::{
    EspMqttClient, EventPayload, LwtConfiguration, MqttClientConfiguration, QoS,
};
use esp_idf_svc::nvs::{EspDefaultNvsPartition, EspNvs, NvsDefault};
use esp_idf_svc::wifi::{
    AccessPointConfiguration, AuthMethod, ClientConfiguration, Configuration, EspWifi,
};

use crate::Shared;

pub const DEVICE_ID: &str = "stickc_ac_bridge";
pub const DEVICE_NAME: &str = "AC IR Bridge";
const AP_SSID: &str = "AC-Remote";
const AP_PASSWORD: &str = "12345678";
const STA_CONNECT_TIMEOUT: Duration = Duration::from_secs(20);

// --- settings ----------------------------------------------------------------

#[derive(Clone, Default)]
pub struct Settings {
    pub ssid: String,
    pub pass: String,
    pub mqtt_host: String,
    pub mqtt_port: u16,
    pub mqtt_user: String,
    pub mqtt_pass: String,
    pub off_variant: u8,
}

/// NVS access shared between boot-time load and the web save handlers.
pub struct Store {
    net: Mutex<EspNvs<NvsDefault>>,
    web: Mutex<EspNvs<NvsDefault>>,
}

fn get_string(nvs: &EspNvs<NvsDefault>, key: &str, default: &str) -> String {
    let mut buf = [0u8; 128];
    match nvs.get_str(key, &mut buf) {
        Ok(Some(v)) => v.to_string(),
        _ => default.to_string(),
    }
}

impl Store {
    pub fn new(part: EspDefaultNvsPartition) -> Result<Self> {
        Ok(Self {
            net: Mutex::new(EspNvs::new(part.clone(), "net", true)?),
            web: Mutex::new(EspNvs::new(part, "web", true)?),
        })
    }

    pub fn load(&self) -> Settings {
        let net = self.net.lock().unwrap();
        let web = self.web.lock().unwrap();
        Settings {
            ssid: get_string(&net, "ssid", ""),
            pass: get_string(&net, "pass", ""),
            mqtt_host: get_string(&net, "mhost", ""),
            // The Arduino firmware stores these via Preferences putUInt/putInt.
            mqtt_port: net.get_u32("mport").ok().flatten().unwrap_or(1883) as u16,
            mqtt_user: get_string(&net, "muser", ""),
            mqtt_pass: get_string(&net, "mpass", ""),
            off_variant: web
                .get_i32("offv")
                .ok()
                .flatten()
                .unwrap_or(OFF_VARIANT_DEFAULT as i32) as u8,
        }
    }

    pub fn save_wifi(&self, ssid: &str, pass: &str) -> Result<()> {
        let mut net = self.net.lock().unwrap();
        net.set_str("ssid", ssid)?;
        net.set_str("pass", pass)?;
        Ok(())
    }

    pub fn save_mqtt(&self, host: &str, port: u16, user: &str, pass: &str) -> Result<()> {
        let mut net = self.net.lock().unwrap();
        net.set_str("mhost", host)?;
        net.set_u32("mport", port as u32)?;
        net.set_str("muser", user)?;
        net.set_str("mpass", pass)?;
        Ok(())
    }

    pub fn save_off_variant(&self, v: u8) -> Result<()> {
        self.web.lock().unwrap().set_i32("offv", v as i32)?;
        Ok(())
    }
}

// --- Wi-Fi ---------------------------------------------------------------------

pub struct Wifi {
    wifi: EspWifi<'static>,
    pub ap_mode: bool,
    last_reconnect: Instant,
}

impl Wifi {
    /// Brings the interface up: STA when credentials exist and connect within
    /// the timeout, otherwise the AC-Remote fallback AP.
    pub fn start(
        modem: Modem,
        sysloop: EspSystemEventLoop,
        nvs: EspDefaultNvsPartition,
        settings: &Settings,
    ) -> Result<Self> {
        let mut wifi = EspWifi::new(modem, sysloop, Some(nvs))?;
        let mut ap_mode = true;
        if !settings.ssid.is_empty() {
            let client = ClientConfiguration {
                ssid: settings.ssid.as_str().try_into().map_err(|_| anyhow!("ssid too long"))?,
                password: settings
                    .pass
                    .as_str()
                    .try_into()
                    .map_err(|_| anyhow!("password too long"))?,
                auth_method: if settings.pass.is_empty() {
                    AuthMethod::None
                } else {
                    AuthMethod::WPA2Personal
                },
                ..Default::default()
            };
            wifi.set_configuration(&Configuration::Client(client))?;
            wifi.start()?;
            wifi.connect()?;
            let deadline = Instant::now() + STA_CONNECT_TIMEOUT;
            while Instant::now() < deadline {
                if wifi.is_connected()? && !wifi.sta_netif().get_ip_info()?.ip.is_unspecified() {
                    ap_mode = false;
                    break;
                }
                std::thread::sleep(Duration::from_millis(200));
            }
            if ap_mode {
                log::warn!("STA connect to '{}' timed out, falling back to AP", settings.ssid);
                wifi.stop()?;
            }
        }
        if ap_mode {
            let ap = AccessPointConfiguration {
                ssid: AP_SSID.try_into().unwrap(),
                password: AP_PASSWORD.try_into().unwrap(),
                auth_method: AuthMethod::WPA2Personal,
                ..Default::default()
            };
            wifi.set_configuration(&Configuration::AccessPoint(ap))?;
            wifi.start()?;
            // Note: esp-idf-svc's default AP address is 192.168.71.1 (not
            // the 192.168.4.1 that ESP-IDF/Arduino uses).
            log::info!("WiFi: AP fallback '{AP_SSID}' pass '{AP_PASSWORD}'");
        } else {
            // Modem sleep between DTIM beacons — the single biggest battery
            // win. Not applicable in AP mode (an AP must beacon constantly).
            unsafe {
                esp_idf_svc::sys::esp_wifi_set_ps(
                    esp_idf_svc::sys::wifi_ps_type_t_WIFI_PS_MAX_MODEM,
                );
            }
        }
        Ok(Self { wifi, ap_mode, last_reconnect: Instant::now() })
    }

    pub fn sta_up(&self) -> bool {
        !self.ap_mode && self.wifi.is_connected().unwrap_or(false)
    }

    pub fn ip(&self) -> String {
        let info = if self.ap_mode {
            self.wifi.ap_netif().get_ip_info()
        } else {
            self.wifi.sta_netif().get_ip_info()
        };
        match info {
            Ok(i) if !i.ip.is_unspecified() => i.ip.to_string(),
            _ => String::new(),
        }
    }

    /// Non-blocking STA reconnect attempts on a 10 s timer (AP is static).
    pub fn poll(&mut self) {
        if self.ap_mode || self.sta_up() {
            return;
        }
        if self.last_reconnect.elapsed() >= Duration::from_secs(10) {
            self.last_reconnect = Instant::now();
            let _ = self.wifi.connect();
        }
    }
}

// --- MQTT ----------------------------------------------------------------------

pub struct Mqtt {
    client: Arc<Mutex<EspMqttClient<'static>>>,
}

fn topic(suffix: &str) -> String {
    format!("{DEVICE_ID}/{suffix}")
}

fn discovery_json() -> String {
    format!(
        concat!(
            "{{\"name\":\"{name}\",\"unique_id\":\"{id}\",",
            "\"min_temp\":{min},\"max_temp\":{max},\"temp_step\":1,",
            "\"modes\":[\"off\",\"auto\",\"cool\",\"dry\",\"fan_only\",\"heat\"],",
            "\"fan_modes\":[\"auto\",\"low\",\"medium\",\"high\"],",
            "\"swing_modes\":[\"on\",\"off\"],",
            "\"mode_command_topic\":\"{id}/mode/set\",",
            "\"mode_state_topic\":\"{id}/mode/state\",",
            "\"temperature_command_topic\":\"{id}/temp/set\",",
            "\"temperature_state_topic\":\"{id}/temp/state\",",
            "\"fan_mode_command_topic\":\"{id}/fan/set\",",
            "\"fan_mode_state_topic\":\"{id}/fan/state\",",
            "\"swing_mode_command_topic\":\"{id}/swing/set\",",
            "\"swing_mode_state_topic\":\"{id}/swing/state\",",
            "\"availability_topic\":\"{id}/availability\",",
            "\"device\":{{\"identifiers\":[\"{id}\"],\"name\":\"{name}\",",
            "\"manufacturer\":\"M5Stack\",\"model\":\"StickC Plus2 (Rust)\"}}}}"
        ),
        name = DEVICE_NAME,
        id = DEVICE_ID,
        min = ac_core::MIN_TEMP,
        max = ac_core::MAX_TEMP,
    )
}

impl Mqtt {
    /// Connects and spawns the event thread. Returns None when no broker is
    /// configured. The esp-mqtt client reconnects by itself; we re-subscribe
    /// and re-announce on every Connected event.
    pub fn start(settings: &Settings, shared: Arc<Shared>) -> Option<Self> {
        if settings.mqtt_host.is_empty() {
            log::info!("MQTT disabled (no broker configured)");
            return None;
        }
        let url = format!("mqtt://{}:{}", settings.mqtt_host, settings.mqtt_port);
        let avail = topic("availability");
        let conf = MqttClientConfiguration {
            client_id: Some(DEVICE_ID),
            username: (!settings.mqtt_user.is_empty()).then_some(settings.mqtt_user.as_str()),
            password: (!settings.mqtt_user.is_empty()).then_some(settings.mqtt_pass.as_str()),
            lwt: Some(LwtConfiguration {
                topic: &avail,
                payload: b"offline",
                qos: QoS::AtMostOnce,
                retain: true,
            }),
            ..Default::default()
        };
        let (client, mut connection) = match EspMqttClient::new(&url, &conf) {
            Ok(pair) => pair,
            Err(e) => {
                log::warn!("MQTT client init failed: {e}");
                return None;
            }
        };
        let client = Arc::new(Mutex::new(client));
        let mqtt = Self { client: client.clone() };

        std::thread::Builder::new()
            .name("mqtt-events".into())
            .stack_size(8 * 1024)
            .spawn(move || {
                while let Ok(event) = connection.next() {
                    match event.payload() {
                        EventPayload::Connected(_) => {
                            shared.mqtt_up.store(true, Ordering::Relaxed);
                            let mut c = client.lock().unwrap();
                            for s in ["mode/set", "temp/set", "fan/set", "swing/set"] {
                                let _ = c.subscribe(&topic(s), QoS::AtMostOnce);
                            }
                            let _ = c.enqueue(
                                &topic("availability"),
                                QoS::AtMostOnce,
                                true,
                                b"online",
                            );
                            let _ = c.enqueue(
                                &format!("homeassistant/climate/{DEVICE_ID}/config"),
                                QoS::AtMostOnce,
                                true,
                                discovery_json().as_bytes(),
                            );
                            drop(c);
                            shared.publish.store(true, Ordering::Relaxed);
                        }
                        EventPayload::Disconnected => {
                            shared.mqtt_up.store(false, Ordering::Relaxed);
                        }
                        EventPayload::Received { topic: Some(t), data, .. } => {
                            if let Ok(value) = std::str::from_utf8(data) {
                                let key = match t.strip_prefix(DEVICE_ID) {
                                    Some("/mode/set") => "mode",
                                    Some("/temp/set") => "temp",
                                    Some("/fan/set") => "fan",
                                    Some("/swing/set") => "swing",
                                    _ => continue,
                                };
                                let mut ac = shared.ac.lock().unwrap();
                                let before = *ac;
                                let applied = ac.apply(key, value);
                                let changed = applied && before != *ac;
                                drop(ac);
                                if changed {
                                    shared.dirty.store(true, Ordering::Relaxed);
                                }
                                // Settle HA's optimistic UI even on no-ops.
                                if applied {
                                    shared.publish.store(true, Ordering::Relaxed);
                                }
                            }
                        }
                        _ => (),
                    }
                }
                log::warn!("MQTT event loop ended");
            })
            .ok()?;
        Some(mqtt)
    }

    /// Publishes the retained state topics (call after every applied change).
    pub fn publish_state(&self, s: &ac_core::AcState) {
        let mut c = self.client.lock().unwrap();
        let items = [
            ("mode/state", s.mode_str().to_string()),
            ("temp/state", s.temp.to_string()),
            ("fan/state", s.fan_str().to_string()),
            ("swing/state", if s.swing { "on" } else { "off" }.to_string()),
        ];
        for (suffix, payload) in items {
            let _ = c.enqueue(&topic(suffix), QoS::AtMostOnce, true, payload.as_bytes());
        }
    }
}
