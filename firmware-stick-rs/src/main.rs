// Smart AC IR Remote — M5StickC Plus2 "IR bridge", Rust edition.
//
// Same design as the Arduino firmware: one AcState is the single source of
// truth; every change (button, web, MQTT) re-sends the FULL ELECTRA_AC frame
// after a 300 ms debounce that coalesces bursts into one IR transmission.
mod ir;
mod net;
mod ui;
mod web;

use std::sync::atomic::{AtomicBool, AtomicI16, AtomicU16, AtomicU32, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use ac_core::AcState;
use anyhow::Result;
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::adc::attenuation::DB_11;
use esp_idf_svc::hal::adc::oneshot::config::{AdcChannelConfig, Calibration};
use esp_idf_svc::hal::adc::oneshot::{AdcChannelDriver, AdcDriver};
use esp_idf_svc::hal::adc::ADC1;
use esp_idf_svc::hal::gpio::PinDriver;
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::nvs::EspDefaultNvsPartition;

const IR_SEND_DEBOUNCE: Duration = Duration::from_millis(300);
const BATTERY_POLL: Duration = Duration::from_secs(10);

/// State shared with the web server and the MQTT event thread.
pub struct Shared {
    pub ac: Mutex<AcState>,
    /// Set by web/MQTT when they changed `ac`; consumed by the main loop.
    pub dirty: AtomicBool,
    /// Request to (re)publish MQTT state topics.
    pub publish: AtomicBool,
    pub off_variant: AtomicU8,
    /// Selected IR protocol (ac_core::Protocol as_u8 form).
    pub protocol: AtomicU8,
    /// Swing changed since the last IR send — Coolix needs a dedicated
    /// toggle code for it instead of a state bit.
    pub swing_flip: AtomicBool,
    pub wifi_up: AtomicBool,
    pub mqtt_up: AtomicBool,
    /// Battery voltage in mV (0 until the first ADC reading).
    pub batt_mv: AtomicU16,
    /// USB power attached (see ac_core::ChargeDetector).
    pub batt_chg: AtomicBool,
    /// Successfully transmitted IR frames since boot.
    pub ir_sends: AtomicU32,
    /// Scheduler rules + the UTC offset (minutes) they are written in.
    pub schedule: Mutex<Vec<ac_core::Rule>>,
    pub tz_min: AtomicI16,
}

fn main() -> Result<()> {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    let p = Peripherals::take()?;
    let sysloop = EspSystemEventLoop::take()?;
    let nvs = EspDefaultNvsPartition::take()?;

    // HOLD pin: keep the regulator on when running from battery.
    let mut hold = PinDriver::output(p.pins.gpio4)?;
    hold.set_high()?;

    // Auto light-sleep + frequency scaling whenever no driver holds a PM
    // lock (RMT takes one during IR transmit, so timings are unaffected).
    let pm = esp_idf_svc::sys::esp_pm_config_t {
        max_freq_mhz: 160,
        min_freq_mhz: 40,
        light_sleep_enable: true,
    };
    esp_idf_svc::sys::esp!(unsafe {
        esp_idf_svc::sys::esp_pm_configure(&pm as *const _ as *const core::ffi::c_void)
    })?;

    // Battery voltage: GPIO38 sits behind a 1:2 divider on the Plus2.
    let adc: &'static AdcDriver<'static, ADC1> = Box::leak(Box::new(AdcDriver::new(p.adc1)?));
    let batt_cfg = AdcChannelConfig {
        attenuation: DB_11,
        calibration: Calibration::Line,
        ..Default::default()
    };
    let mut batt_ch = AdcChannelDriver::new(adc, p.pins.gpio38, &batt_cfg)?;
    let mut batt_mv: u16 = 0;
    let mut charge_det: Option<ac_core::ChargeDetector> = None;
    let mut last_batt_poll: Option<Instant> = None;

    let store = Arc::new(net::Store::new(nvs.clone())?);
    let settings = store.load();
    let (sched_rules, tz_min) = store.load_schedule();

    let shared = Arc::new(Shared {
        ac: Mutex::new(AcState::default()),
        dirty: AtomicBool::new(false),
        publish: AtomicBool::new(false),
        off_variant: AtomicU8::new(settings.off_variant),
        protocol: AtomicU8::new(settings.protocol.as_u8()),
        swing_flip: AtomicBool::new(false),
        wifi_up: AtomicBool::new(false),
        mqtt_up: AtomicBool::new(false),
        batt_mv: AtomicU16::new(0),
        batt_chg: AtomicBool::new(false),
        ir_sends: AtomicU32::new(0),
        schedule: Mutex::new(sched_rules),
        tz_min: AtomicI16::new(tz_min),
    });

    let mut ir = ir::IrSender::new(p.rmt.channel0, p.pins.gpio19)?;
    let mut ui = ui::Ui::new(ui::Pins {
        spi: p.spi2,
        sclk: p.pins.gpio13,
        mosi: p.pins.gpio15,
        cs: p.pins.gpio5,
        dc: p.pins.gpio14,
        rst: p.pins.gpio12,
        backlight: p.pins.gpio27,
        btn_a: p.pins.gpio37,
        btn_b: p.pins.gpio39,
    })?;

    let mut wifi = net::Wifi::start(p.modem, sysloop, nvs, &settings)?;
    // Wall clock for the scheduler; syncs in the background once STA is up.
    let _sntp = if wifi.ap_mode { None } else { Some(esp_idf_svc::sntp::EspSntp::new_default()?) };
    let mqtt = if wifi.ap_mode { None } else { net::Mqtt::start(&settings, shared.clone()) };
    let _server = web::start(shared.clone(), store, wifi.handle())?;
    log::info!("web UI up at http://{}/", wifi.ip());

    let mut last_sent = *shared.ac.lock().unwrap();
    let mut pending_since: Option<Instant> = None;
    let mut sched_prev: Option<i64> = None;

    loop {
        let mut changed = {
            let mut ac = shared.ac.lock().unwrap();
            ui.handle_buttons(&mut ac)
        };
        changed |= shared.dirty.swap(false, Ordering::Relaxed);

        // Scheduler: check rules at every minute boundary once SNTP gave us
        // a real wall clock (pre-sync the clock sits in 1970).
        let epoch = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        if epoch > 1_600_000_000 {
            match sched_prev {
                None => sched_prev = Some(epoch),
                Some(prev) if epoch / 60 != prev / 60 => {
                    sched_prev = Some(epoch);
                    let due = ac_core::schedule_due(
                        &shared.schedule.lock().unwrap(),
                        prev,
                        epoch,
                        shared.tz_min.load(Ordering::Relaxed),
                    );
                    if let Some(on) = due {
                        let mut ac = shared.ac.lock().unwrap();
                        if ac.power != on {
                            ac.power = on;
                            drop(ac);
                            changed = true;
                            log::info!("schedule: power {}", if on { "on" } else { "off" });
                        }
                    }
                }
                _ => (),
            }
        }

        if changed {
            pending_since = Some(Instant::now());
            shared.publish.store(true, Ordering::Relaxed); // instant HA feedback
        }

        if let Some(t0) = pending_since {
            if t0.elapsed() >= IR_SEND_DEBOUNCE {
                pending_since = None;
                let ac = *shared.ac.lock().unwrap();
                let proto = ac_core::Protocol::from_u8(shared.protocol.load(Ordering::Relaxed));
                let swing_flip = shared.swing_flip.swap(false, Ordering::Relaxed);

                // Protocols like Coolix carry no swing bit in the state
                // frame — swing is its own toggle code. Send that first,
                // and skip the state frame when swing was the only change.
                let mut frames: Vec<Vec<u32>> = Vec::new();
                let toggle = ac_core::swing_toggle_pulses(proto);
                let uses_toggle = toggle.is_some();
                if swing_flip {
                    if let Some(p) = toggle {
                        frames.push(p);
                    }
                }
                let all_but_swing_same = {
                    let mut a = ac;
                    a.swing = last_sent.swing;
                    a == last_sent
                };
                if ac != last_sent && !(uses_toggle && all_but_swing_same) {
                    let variant = shared.off_variant.load(Ordering::Relaxed);
                    frames.push(ac_core::ir_pulses(proto, &ac, variant));
                }

                if !frames.is_empty() {
                    let mut ok = true;
                    for f in &frames {
                        match ir.send(f) {
                            Ok(()) => {
                                shared.ir_sends.fetch_add(1, Ordering::Relaxed);
                            }
                            Err(e) => {
                                ok = false;
                                log::error!("IR send failed: {e}");
                            }
                        }
                    }
                    if ok {
                        last_sent = ac;
                        log::info!(
                            "IR sent ({}): {} {}C fan={} swing={}",
                            proto.as_str(),
                            ac.mode_str(),
                            ac.temp,
                            ac.fan_str(),
                            ac.swing
                        );
                    }
                }
            }
        }

        if shared.publish.swap(false, Ordering::Relaxed) {
            if let Some(m) = &mqtt {
                m.publish_state(&shared.ac.lock().unwrap().clone());
            }
        }

        wifi.poll();
        shared.wifi_up.store(wifi.sta_up(), Ordering::Relaxed);

        if last_batt_poll.is_none_or(|t| t.elapsed() >= BATTERY_POLL) {
            last_batt_poll = Some(Instant::now());
            if let Ok(mv) = adc.read(&mut batt_ch) {
                batt_mv = mv.saturating_mul(2);
                let chg = match charge_det.as_mut() {
                    Some(d) => d.update(batt_mv),
                    None => charge_det.insert(ac_core::ChargeDetector::new(batt_mv)).charging(),
                };
                shared.batt_mv.store(batt_mv, Ordering::Relaxed);
                shared.batt_chg.store(chg, Ordering::Relaxed);
            }
        }

        let ac = *shared.ac.lock().unwrap();
        ui.update(
            &ac,
            shared.wifi_up.load(Ordering::Relaxed),
            shared.mqtt_up.load(Ordering::Relaxed),
            &wifi.ip(),
            batt_mv,
            shared.batt_chg.load(Ordering::Relaxed),
        );

        std::thread::sleep(Duration::from_millis(20));
    }
}
