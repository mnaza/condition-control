// Display + buttons (M5Unified). BtnA toggles power, BtnB cycles temp —
// local fallback so the device works with no Wi-Fi at all.
#pragma once
#include "ac_state.h"

void uiInit();
// Polls buttons; mutates s and returns true if the user changed the state.
bool uiHandleButtons(AcState& s);
// Redraws only when something visible changed.
void uiUpdate(const AcState& s, bool wifiUp, bool mqttUp);
