// Smart AC IR Remote — M5StickC Plus2 "IR bridge", Rust edition.
//
// Same design as the Arduino firmware: one AcState is the single source of
// truth; every change (button, web, MQTT) re-sends the FULL ELECTRA_AC frame
// after a 300 ms debounce that coalesces bursts into one IR transmission.
mod ir;
mod net;
mod ui;
mod web;

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use ac_core::AcState;
use anyhow::Result;
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::gpio::PinDriver;
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::nvs::EspDefaultNvsPartition;

const IR_SEND_DEBOUNCE: Duration = Duration::from_millis(300);

/// State shared with the web server and the MQTT event thread.
pub struct Shared {
    pub ac: Mutex<AcState>,
    /// Set by web/MQTT when they changed `ac`; consumed by the main loop.
    pub dirty: AtomicBool,
    /// Request to (re)publish MQTT state topics.
    pub publish: AtomicBool,
    pub off_variant: AtomicU8,
    pub wifi_up: AtomicBool,
    pub mqtt_up: AtomicBool,
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

    let store = Arc::new(net::Store::new(nvs.clone())?);
    let settings = store.load();

    let shared = Arc::new(Shared {
        ac: Mutex::new(AcState::default()),
        dirty: AtomicBool::new(false),
        publish: AtomicBool::new(false),
        off_variant: AtomicU8::new(settings.off_variant),
        wifi_up: AtomicBool::new(false),
        mqtt_up: AtomicBool::new(false),
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
    let mqtt = if wifi.ap_mode { None } else { net::Mqtt::start(&settings, shared.clone()) };
    let _server = web::start(shared.clone(), store)?;
    log::info!("web UI up at http://{}/", wifi.ip());

    let mut last_sent = *shared.ac.lock().unwrap();
    let mut pending_since: Option<Instant> = None;

    loop {
        let mut changed = {
            let mut ac = shared.ac.lock().unwrap();
            ui.handle_buttons(&mut ac)
        };
        changed |= shared.dirty.swap(false, Ordering::Relaxed);

        if changed {
            pending_since = Some(Instant::now());
            shared.publish.store(true, Ordering::Relaxed); // instant HA feedback
        }

        if let Some(t0) = pending_since {
            if t0.elapsed() >= IR_SEND_DEBOUNCE {
                pending_since = None;
                let ac = *shared.ac.lock().unwrap();
                if ac != last_sent {
                    let variant = shared.off_variant.load(Ordering::Relaxed);
                    match ir.send(&ac, variant) {
                        Ok(()) => {
                            last_sent = ac;
                            log::info!(
                                "IR sent: {} {}C fan={} swing={}",
                                ac.mode_str(),
                                ac.temp,
                                ac.fan_str(),
                                ac.swing
                            );
                        }
                        Err(e) => log::error!("IR send failed: {e}"),
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

        let ac = *shared.ac.lock().unwrap();
        ui.update(
            &ac,
            shared.wifi_up.load(Ordering::Relaxed),
            shared.mqtt_up.load(Ordering::Relaxed),
            &wifi.ip(),
        );

        std::thread::sleep(Duration::from_millis(20));
    }
}
