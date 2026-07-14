// ELECTRA_AC transmission over the RMT peripheral (IR LED on GPIO 19).
// The pulse train comes from the pure ac-core encoder; here we only add the
// 38 kHz carrier and clock the marks/spaces out.
use ac_core::{electra_frame, electra_pulses, AcState};
use anyhow::Result;
use esp_idf_svc::hal::gpio::Gpio19;
use esp_idf_svc::hal::rmt::config::{CarrierConfig, DutyPercent, TransmitConfig};
use esp_idf_svc::hal::rmt::{
    PinState, Pulse, PulseTicks, TxRmtDriver, VariableLengthSignal, CHANNEL0,
};
use esp_idf_svc::hal::units::Hertz;

pub struct IrSender {
    tx: TxRmtDriver<'static>,
}

impl IrSender {
    pub fn new(channel: CHANNEL0, pin: Gpio19) -> Result<Self> {
        let carrier = CarrierConfig::new()
            .frequency(Hertz(38_000))
            .duty_percent(DutyPercent::new(50)?);
        // APB 80 MHz / 80 -> 1 µs per RMT tick.
        let cfg = TransmitConfig::new().clock_divider(80).carrier(Some(carrier));
        Ok(Self { tx: TxRmtDriver::new(channel, pin, &cfg)? })
    }

    /// Sends the FULL state as one frame (AC remotes are stateless receivers).
    pub fn send(&mut self, s: &AcState, off_variant: u8) -> Result<()> {
        let pulses = electra_pulses(&electra_frame(s, off_variant));
        let mut signal = VariableLengthSignal::new();
        for (i, &us) in pulses.iter().enumerate() {
            let level = if i % 2 == 0 { PinState::High } else { PinState::Low };
            let pulse = Pulse::new(level, PulseTicks::new(us as u16)?);
            signal.push(std::iter::once(&pulse))?;
        }
        self.tx.start_blocking(&signal)?;
        Ok(())
    }
}
