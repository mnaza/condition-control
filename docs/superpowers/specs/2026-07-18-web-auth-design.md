# Web UI Authentication — Design

**Date:** 2026-07-18 · **Beads epic:** cndition-control-7b0

## Goal

Protect the StickC Plus2 web server (UI page and every `/api/*` endpoint)
from unauthenticated LAN access. Today anyone on the network can control
the AC, flash firmware over `/api/ota`, and read/change Wi-Fi and MQTT
credentials.

## Mechanism: HTTP Basic Auth

- Every handler in `web.rs` — including `/` — first runs an auth check.
- The check reads `Authorization: Basic <base64>`, decodes `user:password`,
  **ignores the username**, and compares the password against the stored
  one using a constant-time comparison.
- Failure → `401` with `WWW-Authenticate: Basic realm="AC Remote"`.
  The browser shows its native prompt and caches the credentials, so the
  existing `fetch()` polling keeps working with no JS changes.
- **No stored password ⇒ auth disabled** (device behaves exactly as today).
  The device can never lock the owner out before they opt in.
- The same rule applies in AP fallback mode.
- Unaffected: MQTT/Home Assistant, the outbound GitHub release update
  checker, ESP-NOW (future). `/api/ota` being protected is a deliberate
  bonus: no unauthenticated LAN flashing.
- No TLS: credentials travel in cleartext on the LAN. Accepted for the
  threat model (personal LAN; any scheme without TLS has this property).

## Pure logic in `ac-core` (host-tested)

New module `auth` in `firmware-stick-rs/ac-core`:

- `base64_decode(&str) -> Option<Vec<u8>>` — ~20-line standard-alphabet
  decoder, no new dependency, rejects invalid input.
- `parse_basic_auth(header: &str) -> Option<(String, String)>` — accepts
  `"Basic <b64>"` (case-insensitive scheme), splits decoded bytes at the
  first `:`.
- `constant_time_eq(a: &[u8], b: &[u8]) -> bool`.
- `check_password(header: Option<&str>, stored: &str) -> bool` — the one
  entry point `web.rs` calls; returns `true` when `stored` is empty.

`web.rs` only wires the header in and the 401 out.

## Storage & settings API

- NVS: key `webpw` in the existing `web` namespace. Empty/absent = auth
  off. Loaded at boot into the shared settings; changes take effect
  immediately (no reboot).
- `POST /api/webauth`, form-encoded body `password=...` (matching the
  page's existing `post()` helper and `form_pairs`) — sets the password;
  empty value clears it. The request itself is subject to the same auth
  check, so no separate old-password field is needed. Takes effect
  immediately, no reboot.
- `pwset: bool` added to the `GET /api/mqtt` response (the payload the
  settings modal already fetches) so the UI knows whether a password is
  configured.

## UI (`index.html`)

Settings modal gains a "Web password" block: password input + Save
button, and a hint line — "no password set — UI is open to the LAN"
when `pwset` is false. No login screen; the browser handles prompting.

## Recovery

Holding **BtnB while powering on** clears `webpw` (checked once at boot,
before the web server starts; a warning is logged — the display is not
yet drawn at that point). Physical access equals ownership; a forgotten
password never bricks remote access.

## Testing

- `ac-core` host tests (TDD): base64 valid/invalid/padding, missing
  colon, empty password, wrong password, empty stored password ⇒ allow,
  scheme case-insensitivity, unicode passwords.
- On-device verification: `curl` with no/wrong/correct credentials
  (401/401/200), set + clear round-trip via `/api/webauth`, phone
  browser prompt, recovery-button clear.
