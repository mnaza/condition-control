# DST-proof Schedule Timezones — Design

**Date:** 2026-07-20 · **Beads:** cndition-control-54u

## Problem

The schedule stores a fixed UTC offset captured from the browser; every
DST switch silently shifts all rules by an hour until someone re-saves.

## Design

- `ac_core::tz_offset_min(rule: &str, epoch: i64) -> Option<i16>` — a
  POSIX TZ rule evaluator (the `TZ=` format: `EET-2EEST,M3.5.0/3,M10.5.0/4`).
  Supported: quoted `<...>` and alphabetic zone names, `±h[:mm]` offsets
  (POSIX west-positive, returned east-positive), optional DST offset
  (default std+1h), `Mm.w.d[/time]` transition dates only (week 5 =
  last), default transition time 02:00, northern and southern hemisphere
  ordering. Anything else (J-day forms, DST without rules, junk) → None.
- `schedule_due(rules, prev, now, tz_min, tz_rule)` — per check the
  effective offset is `tz_offset_min(rule, now).unwrap_or(tz_min)`; the
  fixed offset stays as the fallback and for old saves.
- Date math helpers (days_from_civil / civil_from_days, Hinnant
  algorithms) live next to it, host-tested.
- **Storage:** NVS `tzr` string in the `web` namespace, saved with the
  schedule; `Shared.tz_rule: Mutex<String>`.
- **API:** `/api/schedule` POST gains a `tzrule` field, GET returns it.
- **UI:** on save the page auto-generates the rule from the browser — it
  probes `Date` offsets across the year; constant offset → quoted fixed
  rule (`<+02>-2`), DST → binary-search both transition instants and
  emit `Mm.w.d/h` terms. No user input; the schedule hint mentions
  DST is now automatic.

## Testing

Host tests with exact epochs (python-verified): Ukraine EET/EEST across
both 2026 transitions incl. exact boundary minutes, CET default-time
rule, fixed offsets (`<+02>-2`, `UTC0`, `<-03>3`), southern-hemisphere
AEST/AEDT (Jan=DST, Jul=std), malformed inputs → None. JS generator is
exercised live (save the schedule, GET shows the rule).

## Out of scope

Sub-minute offsets, J-day rules, the 167-hour extended time field,
historic zone changes (rule reflects the browser's current zone rules —
re-save only if the government changes the rules themselves).
