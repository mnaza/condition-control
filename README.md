# Smart AC IR Remote — M5StickC Plus2

Wi-Fi bridge that controls a **Baxi** air conditioner (remote YKR-L/201E,
**ELECTRA_AC** protocol) over MQTT / Home Assistant, using the M5StickC Plus2's
built-in IR LED. Works standalone too: BtnA toggles power, BtnB cycles the
set temperature.

## Layout

| Path | What |
|------|------|
| `firmware-stick/` | Main firmware (PlatformIO): IR + display + buttons + Wi-Fi/MQTT/HA |
| `tools/protocol-test/` | **Step 0** — minimal sketch to confirm the AC speaks ELECTRA_AC |
| `tools/sniffer/` | IRrecvDumpV2-based decoder (needs an IR receiver on Grove G33) |

## Step 0 — confirm the protocol (do this first)

```bash
cd tools/protocol-test
pio run -t upload && pio device monitor -b 115200
```

Point the Stick's top (IR LED) at the AC from 1–2 m. Press **BtnA** (big front
button): the AC should beep and turn on cooling at 24 °C. **BtnB** turns it
off. If nothing happens, flash `tools/sniffer/`, wire a TSOP38238 / M5 IR Unit
to the Grove port (data → G33), press buttons on the original remote and read
the decoded protocol from the serial monitor.

## Main firmware

```bash
cd firmware-stick
cp src/secrets.example.h src/secrets.h   # then edit credentials
pio run -e stickc_plus2 -t upload
pio device monitor -b 115200
```

### Web UI

The device serves a control page on port 80 (power, mode, temperature, fan,
swing, plus settings). Credentials are optional: with no `secrets.h` and no
saved network the device opens the **`AC-Remote`** access point (password
`12345678`) — join it, open <http://192.168.4.1>, control the AC directly or
enter your Wi-Fi in *Настройки* (saved to NVS, then reboots). On your network
it's reachable at the IP shown on the device display or at
<http://ac-remote.local> (mDNS).

The MQTT broker (host/port/user/password) is configurable from the same
settings section — NVS values override `secrets.h`, an empty host disables
MQTT. So the whole setup works without ever creating `secrets.h`.

The settings section also selects the power-OFF frame encoding (v1–v4). The
YKR-L/201E ignores the stock IRremoteESP8266 OFF frame; a live test confirmed
it needs byte 11 = 0x05 (v3, now the default — see `src/electra_off.h`). The
choice persists across reboots.

### Home Assistant

The device announces itself via MQTT discovery (`homeassistant/climate/…`) and
appears as a **climate** entity with modes off/auto/cool/dry/fan_only/heat,
16–32 °C, fan auto/low/medium/high and vertical swing. No YAML needed — just a
running MQTT broker integrated with HA. Availability is tracked through an LWT
topic.

MQTT topics (also usable without HA): `<DEVICE_ID>/mode/set|state`,
`temp/set|state`, `fan/set|state`, `swing/set|state`, `availability`.

### Design notes

- `AcState` is the single source of truth; every change re-sends the **full**
  IR frame (AC remotes are stateless receivers — no button replay).
- Changes are debounced 300 ms so slider drags in HA become one IR burst.
- All networking is non-blocking; the device keeps working as a local remote
  when Wi-Fi/MQTT are down, and reconnects on timers.

## Rust edition (`firmware-stick-rs/`)

A functionally equivalent rewrite on `esp-idf-svc` (std): same web UI and
endpoints, same NVS keys (settings saved by either firmware carry over), same
MQTT/HA discovery, and a from-scratch ELECTRA_AC encoder on the RMT peripheral
with the confirmed byte11=0x05 OFF fix. Differences: no mDNS (IP is on the
display), and the fallback AP lives at **192.168.71.1** (esp-idf-svc default)
instead of 192.168.4.1.

The Rust build goes further than the Arduino one (all user-verified on the
device):

- **Web UI 2.0** — bilingual RU/EN (auto from the browser, toggle persists),
  icon buttons, temperature dial, WiFi scanner with fan-shaped signal icons
  (`GET /api/scan`).
- **Battery** — percent (LiPo curve) + runtime estimate on the page, the
  display and `/api/status`; charging detected from voltage *steps* (this
  unit's ADC reads ~50 mV high, absolute thresholds don't work).
- **Diagnostics** — `GET /api/health`: uptime, reset reason (brownout = died
  from battery), heap free/min, RSSI, device time, IR frame counter.
- **Scheduler** — up to 8 rules (time + weekday mask → power on/off), edited
  on the page, stored in NVS, applied without reboot (`/api/schedule`).
  Clock via SNTP; the timezone offset is captured from the browser on every
  save (re-save after a DST switch). Missed rules are replayed for at most
  the trailing 3 h after a power gap.

Power saving (Rust build only): WiFi max modem sleep, automatic light sleep
with frequency scaling (`CONFIG_PM_ENABLE`, 160→40 MHz), 30 s backlight
timeout (a press while dark only wakes the screen), battery voltage on the
display (ADC GPIO38, 1:2 divider). In AP fallback mode consumption stays high —
an AP must beacon; join it to a real network for battery use.

```bash
# toolchain (once): espup install; plus espflash and ldproxy on PATH
cd firmware-stick-rs
source ~/export-esp.sh
cargo build --release
espflash flash --monitor target/xtensa-esp32-espidf/release/firmware-stick-rs

# host tests for the pure core (state, frames, parsing):
cd ac-core && cargo +stable test
```

## Tests

Pure-logic state handling is unit-tested on the host:

```bash
cd firmware-stick
pio test -e native
```

## Hardware notes

- IR LED: GPIO 19, weak (~1–3 m, aim it at the AC).
- No `m5stick-c-plus2` board def exists in platform-espressif32 6.x, so builds
  use `m5stick-c` with `flash_size = 8MB` overrides.
- The optional M5 S3 wearable remote (ESP-NOW) is not implemented yet —
  pending confirmation of the exact S3 model.
