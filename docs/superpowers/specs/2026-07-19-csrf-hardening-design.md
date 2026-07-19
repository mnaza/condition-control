# CSRF / Origin Hardening — Design

**Date:** 2026-07-19 · **Beads:** cndition-control-68h · Approach approved by owner (POST-only migration).

## Problem

Browsers attach cached Basic-Auth credentials to any request aimed at the
device, so a malicious page can drive the AC (`<img
src=http://device/api/set?power=on>`) or trigger updates cross-site.
Mutating GETs cannot be defended with header checks: `<img>` requests
carry no Origin and Referer is attacker-suppressible.

## Design

1. **Mutating endpoints are POST-only.** `/api/set`, `/api/offvariant`,
   `/api/protocol`, `/api/update` migrate from GET to POST; parameters
   move from the query string to a form-encoded body with the same keys
   (`power/mode/temp/fan/swing`, `v`, `p`). The old GETs get esp-idf's
   automatic 405. Read-only GETs (`/`, status, schedule GET, health,
   update/status, scan, mqtt GET) are unchanged.
2. **Origin-vs-Host gate on every mutating POST** (including the existing
   ones: schedule, wifi, mqtt, webauth, ota, and the four migrants).
   Rule, implemented in ac-core as `same_origin(origin: Option<&str>,
   host: &str) -> bool`:
   - No `Origin` header → allow (curl, HA, scripts).
   - `Origin` present → must be exactly `http://<Host>` (case-insensitive
     host comparison, a `:80` suffix on either side is ignored). Anything
     else — different host, https, `null`, garbage — → **403**.
   Browsers always send Origin on cross-site POSTs, so this closes CSRF;
   non-browser clients are unaffected.
3. **Order of checks:** auth (401) → origin (403) → body handling. Both
   run before `read_body`.
4. **UI**: `index.html` switches the four call sites to POST with
   form-encoded bodies.
5. **README**: API table gets a Method column update, the Origin rule,
   and a `curl -X POST -d` example. Breaking change called out.

## Out of scope

`/api/scan` stays GET (read-only, though slow); MQTT/HA channel
untouched; no CSRF tokens (Origin gate is sufficient for this threat
model and keeps non-browser clients trivial).

## Testing

- ac-core TDD for `same_origin`: absent, exact match, case difference,
  `:80` on either side, different host/port, https scheme, `null`,
  garbage, empty host.
- On-device matrix: GET /api/set → 405; POST without Origin + auth → 200;
  POST with matching Origin → 200; POST with `Origin: http://evil.example`
  → 403; UI drives dials/protocol/update normally from the phone.
