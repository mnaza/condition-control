// Embedded web UI: single-page control panel served from PROGMEM.
// Endpoints: GET /            — the page
//            GET /api/status  — state JSON (see webStatusJson)
//            GET /api/set     — apply query params (power/mode/temp/fan/swing)
//            GET /api/offvariant?v=0..3 — pick the OFF frame encoding (NVS)
//            POST /api/wifi   — save STA credentials to NVS and reboot
#pragma once
#include "ac_state.h"

// `state` must outlive the web layer; handlers mutate it and set the shared
// dirty flag (see webConsumeDirty).
void webInit(AcState& state);
void webLoop();

// True once if a web request changed the state since the last call.
bool webConsumeDirty();
