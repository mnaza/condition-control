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

static void printRaw(const char* tag) {
  Serial.printf("%s state:", tag);
  const uint8_t* raw = ac.getRaw();
  for (uint8_t i = 0; i < kElectraAcStateLength; i++)
    Serial.printf(" %02X", raw[i]);
  Serial.println();
}

// The plain library OFF (power bit 0x20 cleared in byte 9) is ignored by the
// YKR-L/201E, so BtnB cycles candidate OFF encodings. Bytes 9/11 hints come
// from the AUX YKR-P/002E (arduino-heatpumpir#71): ON=0x30/0x05 there, so
// OFF may need byte9 bit4 (0x10) kept set and/or byte11=0x05.
static int offVariant = 0;

static void sendOffVariant(int v) {
  ac.setPower(false);
  ac.setMode(kElectraAcCool);
  ac.setTemp(24);
  ac.setFan(kElectraAcFanAuto);
  uint8_t st[kElectraAcStateLength];
  memcpy(st, ac.getRaw(), kElectraAcStateLength);
  switch (v) {
    case 0: break;                            // baseline library OFF
    case 1: st[9] |= 0x10; break;             // keep bit4 set: byte9 = 0x10
    case 2: st[11] = 0x05; break;             // AUX-style byte11
    case 3: st[9] |= 0x10; st[11] = 0x05; break;  // both
  }
  ac.setRaw(st);
  ac.send();
  char msg[24];
  snprintf(msg, sizeof(msg), "OFF v%d sent", v + 1);
  show(msg, TFT_ORANGE);
  printRaw(msg);
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
    printRaw("ON");
  }
  if (M5.BtnB.wasClicked()) {
    sendOffVariant(offVariant);
    offVariant = (offVariant + 1) % 4;
  }
}
