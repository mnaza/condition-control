// Wi-Fi + MQTT with Home Assistant discovery. Fully non-blocking:
// reconnects are attempted on millis() timers, never spin-waited, so the
// device keeps working as a local IR remote while offline.
#pragma once
#include "ac_state.h"

// `state` must outlive the network layer; MQTT commands mutate it and set
// the shared dirty flag (see netConsumeDirty).
void netInit(AcState& state);
void netLoop();

bool netWifiUp();
bool netMqttUp();

// True once if an MQTT command changed the state since the last call.
bool netConsumeDirty();

// Publish retained state topics (call after every applied change).
void netPublishState(const AcState& s);
