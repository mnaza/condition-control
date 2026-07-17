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

    /// Clocks out one mark/space train (even indices = mark). All supported
    /// protocols use the same 38 kHz carrier.
    pub fn send(&mut self, pulses: &[u32]) -> Result<()> {
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
