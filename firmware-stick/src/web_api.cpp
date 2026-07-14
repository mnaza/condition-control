#include "web_api.h"

#include <stdio.h>
#include <string.h>

bool webApplyParam(const char* key, const char* value, AcState& s) {
  if (strcmp(key, "power") == 0) {
    if (strcmp(value, "on") == 0) s.power = true;
    else if (strcmp(value, "off") == 0) s.power = false;
    else if (strcmp(value, "toggle") == 0) s.power = !s.power;
    else return false;
    return true;
  }
  if (strcmp(key, "mode") == 0) return acModeFromString(value, s);
  if (strcmp(key, "temp") == 0) return acTempFromPayload(value, s);
  if (strcmp(key, "fan") == 0) return acFanFromString(value, s.fan);
  if (strcmp(key, "swing") == 0) {
    if (strcmp(value, "on") == 0) s.swing = true;
    else if (strcmp(value, "off") == 0) s.swing = false;
    else return false;
    return true;
  }
  return false;
}

int webParsePort(const char* str) {
  if (*str == '\0') return 0;
  long v = 0;
  for (const char* p = str; *p; p++) {
    if (*p < '0' || *p > '9') return 0;
    v = v * 10 + (*p - '0');
    if (v > 65535) return 0;
  }
  return static_cast<int>(v);
}

int webStatusJson(const AcState& s, bool wifiUp, bool mqttUp, int offVariant,
                  char* buf, size_t len) {
  return snprintf(buf, len,
                  "{\"power\":%s,\"mode\":\"%s\",\"temp\":%d,\"fan\":\"%s\","
                  "\"swing\":%s,\"wifi\":%s,\"mqtt\":%s,\"offVariant\":%d}",
                  s.power ? "true" : "false", acModeToString(s), s.temp,
                  acFanToString(s.fan), s.swing ? "true" : "false",
                  wifiUp ? "true" : "false", mqttUp ? "true" : "false",
                  offVariant);
}
