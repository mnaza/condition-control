// Smart AC IR Remote — M5StickC Plus2 "IR bridge".
//
// Desired state lives in one AcState; every change (button or MQTT) re-sends
// the FULL ELECTRA_AC frame after a short debounce that coalesces bursts
// (e.g. HA dragging the temperature slider) into a single IR transmission.
#include <M5Unified.h>

#include "ac_state.h"
#include "config.h"
#include "ir_send.h"
#include "net.h"
#include "ui.h"

static AcState acState;
static AcState lastSent;
static bool pendingSend = false;
static unsigned long lastChangeMs = 0;

void setup() {
  auto cfg = M5.config();
  M5.begin(cfg);
  Serial.begin(115200);

  uiInit();
  irSendInit();
  netInit(acState);
  uiUpdate(acState, netWifiUp(), netMqttUp());
}

void loop() {
  M5.update();

  bool changed = uiHandleButtons(acState);
  netLoop();
  changed |= netConsumeDirty();

  if (changed) {
    pendingSend = true;
    lastChangeMs = millis();
    netPublishState(acState);  // immediate feedback in HA
  }

  if (pendingSend && millis() - lastChangeMs >= kIrSendDebounceMs) {
    pendingSend = false;
    if (acState != lastSent) {
      irSendState(acState);
      lastSent = acState;
      Serial.printf("IR sent: %s %dC fan=%s swing=%s\n", acModeToString(acState),
                    acState.temp, acFanToString(acState.fan),
                    acState.swing ? "on" : "off");
    }
  }

  uiUpdate(acState, netWifiUp(), netMqttUp());
}
