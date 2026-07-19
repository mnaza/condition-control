# AP Provisioning QR — Design

**Date:** 2026-07-19 · **Beads:** cndition-control-v9r · Approach approved in chat.

While the fallback AP is active, the display switches to a dedicated
provisioning screen instead of the normal status view:

- Right: a `WIFI:T:WPA;S:AC-Remote;P:<pass>;;` QR (scan → phone joins the
  AP). Dark modules on a white square, module size computed from the
  matrix (V3 ≈ 29×29 → 4 px on the 135-px screen), reduced 2-module
  quiet zone (practical on a lit LCD).
- Left: `AC-Remote`, the password (manual fallback), `192.168.71.1`.
- Normal screen returns once the device is back on Wi-Fi; buttons keep
  working throughout.

Implementation: `qrcodegen` crate (Nayuki, dependency-free) added to
`ac-core`; `ac_core::wifi_qr(ssid, pass) -> Vec<Vec<bool>>` wraps
encode + module extraction and is host-tested (square matrix, plausible
version size, finder-pattern corners dark, deterministic). Inputs need
no QR-WiFi escaping: the SSID is a constant and the password alphabet
has no `;,:"\`. `ui.rs` draws the matrix with filled rectangles;
`net::AP_SSID` becomes `pub`. No web/API changes. Version 0.3.16.

Testing: host tests for the matrix; live e2e via the 📡 AP-mode button —
scan the QR with a phone, expect it to join `AC-Remote` directly.
