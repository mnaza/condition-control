// IR transmission over the RMT peripheral (IR LED on GPIO 19).
// The pulse train comes from the pure ac-core encoders; here we only add the
// 38 kHz carrier and clock the marks/spaces out.
use anyhow::Result;
use esp_idf_svc::hal::gpio::Gpio19;
use esp_idf_svc::hal::rmt::config::{CarrierConfig, DutyPercent, TransmitConfig};
use esp_idf_svc::hal::rmt::{
    PinState, Pulse, PulseTicks, TxRmtDriver, VariableLengthSignal, CHANNEL0,
};
use esp_idf_svc::hal::units::Hertz;

pub struct IrSender {
    tx: TxRmtDriver<'static>,
    /// Pins APB to 80 MHz while a frame is going out — see `new`.
    pm_lock: esp_idf_svc::sys::esp_pm_lock_handle_t,
    /// CPU MHz sampled just before taking the lock and again with it held.
    /// The pair is the evidence that the lock really does hold the clock up:
    /// on an idle device the first should read 40 and the second 80 or 160.
    cpu_before: u32,
    cpu_during: u32,
}

impl IrSender {
    pub fn new(channel: CHANNEL0, pin: Gpio19) -> Result<Self> {
        let carrier =
            CarrierConfig::new().frequency(Hertz(38_000)).duty_percent(DutyPercent::new(50)?);
        // APB 80 MHz / 80 -> 1 µs per RMT tick.
        let cfg = TransmitConfig::new().clock_divider(80).carrier(Some(carrier));
        let tx = TxRmtDriver::new(channel, pin, &cfg)?;
        // Max pad drive strength (~40 mA vs the ~20 mA default) — squeezes
        // some extra current through the on-board IR LED for better range.
        unsafe {
            esp_idf_svc::sys::gpio_set_drive_capability(
                19,
                esp_idf_svc::sys::gpio_drive_cap_t_GPIO_DRIVE_CAP_3,
            );
        }
        // The channel is clocked from APB, and the *legacy* RMT driver — unlike
        // ESP-IDF's newer one — creates no power-management lock at all. With
        // DFS enabled (esp_pm_configure 160/40 MHz) an idle CPU drops APB to
        // 40 MHz, which stretches every mark and space to 2 us and drags the
        // carrier down to 19 kHz. The LED still lights, but the AC's 38 kHz
        // band-pass receiver discards the frame — so commands only landed when
        // the CPU happened to be busy. Hold APB at maximum for the duration of
        // each transmission, exactly as the newer driver does for APB clocking.
        let mut pm_lock: esp_idf_svc::sys::esp_pm_lock_handle_t = core::ptr::null_mut();
        esp_idf_svc::sys::esp!(unsafe {
            esp_idf_svc::sys::esp_pm_lock_create(
                esp_idf_svc::sys::esp_pm_lock_type_t_ESP_PM_APB_FREQ_MAX,
                0,
                c"ir".as_ptr(),
                &mut pm_lock,
            )
        })?;
        Ok(Self { tx, pm_lock, cpu_before: 0, cpu_during: 0 })
    }

    /// (CPU MHz just before the APB lock, CPU MHz while transmitting) from the
    /// most recent send. Exposed through /api/health as cpuBefore/cpuDuring.
    pub fn last_cpu_mhz(&self) -> (u32, u32) {
        (self.cpu_before, self.cpu_during)
    }

    /// Clocks out one mark/space train (even indices = mark). All supported
    /// protocols use the same 38 kHz carrier.
    pub fn send(&mut self, pulses: &[u32]) -> Result<()> {
        let mut signal = VariableLengthSignal::new();
        for (i, &us) in pulses.iter().enumerate() {
            let level = if i % 2 == 0 { PinState::High } else { PinState::Low };
            let pulse = Pulse::new(level, PulseTicks::new(us as u16)?);
            signal.push(std::iter::once(&pulse))?;
        }
        // Released on both paths — a stuck lock would keep APB (and the
        // battery drain) pinned high forever.
        self.cpu_before = unsafe { esp_idf_svc::sys::esp_rom_get_cpu_ticks_per_us() };
        unsafe { esp_idf_svc::sys::esp_pm_lock_acquire(self.pm_lock) };
        self.cpu_during = unsafe { esp_idf_svc::sys::esp_rom_get_cpu_ticks_per_us() };
        let sent = self.tx.start_blocking(&signal);
        unsafe { esp_idf_svc::sys::esp_pm_lock_release(self.pm_lock) };
        sent?;
        Ok(())
    }
}
