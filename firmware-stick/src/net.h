// Wi-Fi + MQTT with Home Assistant discovery. Fully non-blocking:
// reconnects are attempted on millis() timers, never spin-waited, so the
// device keeps working as a local IR remote while offline.
//
// Credentials: NVS ("net" namespace, saved from the web UI) override
// secrets.h. If STA can't connect within kStaFallbackMs (or the SSID is
// still the placeholder), the device opens the kApSsid/kApPassword access
// point so the web UI stays reachable at 192.168.4.1.
#pragma once
#include "ac_state.h"

// `state` must outlive the network layer; MQTT commands mutate it and set
// the shared dirty flag (see netConsumeDirty).
void netInit(AcState& state);
void netLoop();

bool netWifiUp();
bool netMqttUp();
bool netApUp();
// Dotted-quad of the STA interface, or of the AP (192.168.4.1) in fallback
// mode; "" while disconnected. Buffer owned by net.cpp, valid until next call.
const char* netIp();

// Persist new STA credentials to NVS (used on next boot; web layer reboots).
void netSaveCredentials(const char* ssid, const char* pass);

// True once if an MQTT command changed the state since the last call.
bool netConsumeDirty();

// Publish retained state topics (call after every applied change).
void netPublishState(const AcState& s);
