# Provisioning Hardening — Design

**Date:** 2026-07-19 · **Beads:** cndition-control-d64 · Scope approved:
unique AP password + display (no auto-heal, no button gating).

## Problem

The fallback AP (`AC-Remote`) uses the fixed, README-documented password
`12345678`. It appears not only on first setup but during any Wi-Fi
outage, letting anyone nearby join and reconfigure the device.

## Design

- `ac_core::ap_password(rand: &[u8; 10]) -> String` — maps 10 RNG bytes
  to a 10-char password over an unambiguous alphabet (no i/l/o/0/1;
  ~49 bits). Host-tested (TDD).
- `Store::load` generates the password on first read via
  `esp_fill_random`, persists it as `appw` in the `web` NVS namespace,
  and returns it in the new `Settings::ap_pass` field. Persist failure
  degrades to a fresh password next boot (display always shows the
  current one). It is never logged.
- The AP config uses `settings.ap_pass`; the `AP_PASSWORD` const dies.
- While the AP is active, the display shows the password as a small
  yellow line under the IP (`pass xxxxxxxxxx`). Reading the display
  requires physical presence — that is the ownership proof.
- README First-time-setup section updated: password is per-device, on
  the screen.

## Out of scope (recorded)

Auto-heal from AP back to STA, button-gated AP, provisioning timeout —
owner chose the minimal variant; the unique password removes the actual
vulnerability while keeping outage-time reachability.

## Testing

- ac-core: length, alphabet membership, determinism, distinct inputs →
  distinct outputs.
- On-device: next time the device enters AP fallback (or after a
  temporary wrong-Wi-Fi test), the AP requires the display-shown
  password and `12345678` no longer works. No forced e2e at ship time —
  verified opportunistically.
