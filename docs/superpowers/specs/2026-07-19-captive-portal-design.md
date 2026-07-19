# Captive Portal (AP mode) — Design

**Date:** 2026-07-19 · **Beads:** cndition-control-2xq · Approach approved in chat.

Goal: after joining the `AC-Remote` AP (QR or manual), the phone's own
captive-portal detection pops the device UI — no typing `192.168.71.1`.

## Mechanism

1. **Wildcard DNS** (AP mode only): a UDP :53 thread answers every A
   query with `192.168.71.1`. Packet logic is pure and host-tested:
   `ac_core::dns_captive_response(query, ip) -> Option<Vec<u8>>` —
   echoes id+question, flags NOERROR+RA, one A record (TTL 60); non-A
   queries get an empty NOERROR answer; malformed/multi-question → None
   (dropped).
2. **DHCP advertises us as DNS**: on the AP netif — `dhcps_stop`, set
   DNS info to the AP IP, enable the `OFFER_DNS` (=2) dhcps option,
   `dhcps_start` (raw esp-idf-sys calls, the standard IDF captive-portal
   recipe).
3. **Probe-path redirects** in the web server, registered **only in AP
   mode** (`web::start` gains `captive: bool`): `/generate_204`,
   `/gen_204` (Android), `/hotspot-detect.html` (iOS/macOS),
   `/connecttest.txt`, `/ncsi.txt` (Windows), `/canonical.html`
   (Firefox) → `302 Location: http://192.168.71.1/`, **no auth** —
   deliberate: a fixed redirect carries no state, and the captive sheet
   must be able to reach it; `/` itself still asks for Basic Auth if a
   web password is set.

## Accepted caveats

Some captive sheets handle Basic Auth poorly — fallback is opening a
real browser at `192.168.71.1` (the screen shows it). DNS thread runs
until reboot; AP mode is already reboot-bounded. HTTPS probes can't be
intercepted (no TLS) — phones use the HTTP probes above for the
"sign-in" popup, which is all we need.

## Testing

- Host: DNS responder — A query, AAAA query, truncated packet, qd≠1,
  response byte layout (flags/counts/pointer/A record).
- Live e2e via the 📡 button: join AP (QR), expect the "sign in to
  network" sheet with the UI; Android + whatever else is at hand.
