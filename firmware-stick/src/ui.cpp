#include "ui.h"

#include <M5Unified.h>

// Landscape layout on the 135x240 ST7789V2.
void uiInit() {
  M5.Display.setRotation(1);
  M5.Display.setBrightness(80);
  M5.Display.fillScreen(TFT_BLACK);
}

bool uiHandleButtons(AcState& s) {
  bool changed = false;
  if (M5.BtnA.wasClicked()) {
    s.power = !s.power;
    changed = true;
  }
  if (M5.BtnB.wasClicked()) {
    s.cycleTemp();
    changed = true;
  }
  return changed;
}

struct DrawnState {
  AcState ac;
  bool wifi = false;
  bool mqtt = false;
  String ip;
  bool valid = false;
};
static DrawnState last;

void uiUpdate(const AcState& s, bool wifiUp, bool mqttUp, const char* ip) {
  if (last.valid && last.ac == s && last.wifi == wifiUp &&
      last.mqtt == mqttUp && last.ip == ip)
    return;
  last = {s, wifiUp, mqttUp, String(ip), true};

  auto& d = M5.Display;
  d.startWrite();
  d.fillScreen(TFT_BLACK);

  // Header: power state + link status.
  d.setTextDatum(top_left);
  d.setTextFont(4);
  d.setTextColor(s.power ? TFT_GREEN : TFT_DARKGREY, TFT_BLACK);
  d.drawString(s.power ? "ON" : "OFF", 8, 6);

  d.setTextDatum(top_right);
  d.setTextFont(2);
  d.setTextColor(wifiUp ? TFT_GREEN : TFT_RED, TFT_BLACK);
  d.drawString(wifiUp ? "WiFi" : "WiFi x", 232, 4);
  d.setTextColor(mqttUp ? TFT_GREEN : TFT_RED, TFT_BLACK);
  d.drawString(mqttUp ? "MQTT" : "MQTT x", 232, 20);

  // Web UI address (STA IP, or the AP's 192.168.4.1 in fallback mode).
  if (ip[0] != '\0') {
    d.setTextDatum(top_center);
    d.setTextColor(TFT_LIGHTGREY, TFT_BLACK);
    d.drawString(ip, 120, 4);
  }

  // Big set-temperature in the middle.
  char buf[8];
  snprintf(buf, sizeof(buf), "%dC", s.temp);
  d.setTextDatum(middle_center);
  d.setTextFont(7);
  d.setTextColor(s.power ? TFT_WHITE : TFT_DARKGREY, TFT_BLACK);
  d.drawString(buf, 120, 70);

  // Footer: mode / fan / swing.
  d.setTextDatum(bottom_left);
  d.setTextFont(2);
  d.setTextColor(TFT_CYAN, TFT_BLACK);
  snprintf(buf, sizeof(buf), "%s", acModeToString(s));
  d.drawString(buf, 8, 131);
  d.setTextDatum(bottom_center);
  d.setTextColor(TFT_YELLOW, TFT_BLACK);
  d.drawString(acFanToString(s.fan), 120, 131);
  d.setTextDatum(bottom_right);
  d.setTextColor(TFT_MAGENTA, TFT_BLACK);
  d.drawString(s.swing ? "swing" : "fixed", 232, 131);

  d.endWrite();
}
