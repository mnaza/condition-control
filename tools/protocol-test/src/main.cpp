// Step-0 protocol verification for the Baxi (YKR-L/201E) AC.
//
// Point the StickC Plus2 IR LED at the AC from 1-2 m:
//   BtnA (front)  -> send POWER ON, cool, 24C, fan auto  (ELECTRA_AC)
//   BtnB (side)   -> send POWER OFF
// If the AC beeps and reacts, the protocol is confirmed — proceed with the
// main firmware. If nothing happens, sniff the original remote with
// tools/sniffer instead.
#include <M5Unified.h>
#include <IRremoteESP8266.h>
#include <ir_Electra.h>

constexpr int kIrLedPin = 19;
static IRElectraAc ac(kIrLedPin);

static void show(const char* msg, uint16_t color) {
  M5.Display.fillScreen(TFT_BLACK);
  M5.Display.setTextDatum(middle_center);
  M5.Display.setTextFont(4);
  M5.Display.setTextColor(color, TFT_BLACK);
  M5.Display.drawString(msg, M5.Display.width() / 2, M5.Display.height() / 2);
  Serial.println(msg);
}

void setup() {
  auto cfg = M5.config();
  M5.begin(cfg);
  Serial.begin(115200);
  M5.Display.setRotation(1);
  ac.begin();
  show("A: ON  B: OFF", TFT_WHITE);
}

void loop() {
  M5.update();
  if (M5.BtnA.wasClicked()) {
    ac.setPower(true);
    ac.setMode(kElectraAcCool);
    ac.setTemp(24);
    ac.setFan(kElectraAcFanAuto);
    ac.send();
    show("SENT: ON 24C", TFT_GREEN);
  }
  if (M5.BtnB.wasClicked()) {
    ac.setPower(false);
    ac.send();
    show("SENT: OFF", TFT_ORANGE);
  }
}
