// Pure logic behind the web UI endpoints (no Arduino deps, host-testable).
#pragma once
#include <stddef.h>

#include "ac_state.h"

// Applies one query/form parameter to the state.
// Keys: power=on|off|toggle, mode=off|auto|cool|dry|fan_only|heat,
// temp=<int>, fan=auto|low|medium|high, swing=on|off.
// Returns false (state untouched) on unknown key or bad value.
bool webApplyParam(const char* key, const char* value, AcState& s);

// Renders the /api/status JSON body. snprintf semantics: returns the
// would-be length, output is truncated to len.
int webStatusJson(const AcState& s, bool wifiUp, bool mqttUp, int offVariant,
                  char* buf, size_t len);

// Strict TCP-port parse: returns 1..65535, or 0 for anything else
// (empty, non-numeric, out of range).
int webParsePort(const char* str);
