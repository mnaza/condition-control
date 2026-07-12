#include "ac_state.h"

#include <stdlib.h>
#include <string.h>

void AcState::setTemp(int t) {
  if (t < kAcMinTemp) t = kAcMinTemp;
  if (t > kAcMaxTemp) t = kAcMaxTemp;
  temp = static_cast<uint8_t>(t);
}

void AcState::cycleTemp() {
  temp = (temp >= kAcMaxTemp) ? kAcMinTemp : static_cast<uint8_t>(temp + 1);
}

bool AcState::operator==(const AcState& o) const {
  return power == o.power && mode == o.mode && temp == o.temp &&
         fan == o.fan && swing == o.swing;
}

const char* acModeToString(const AcState& s) {
  if (!s.power) return "off";
  switch (s.mode) {
    case AcMode::Auto: return "auto";
    case AcMode::Cool: return "cool";
    case AcMode::Dry:  return "dry";
    case AcMode::Fan:  return "fan_only";
    case AcMode::Heat: return "heat";
  }
  return "off";
}

bool acModeFromString(const char* str, AcState& s) {
  if (strcmp(str, "off") == 0) { s.power = false; return true; }
  AcMode m;
  if      (strcmp(str, "auto") == 0)     m = AcMode::Auto;
  else if (strcmp(str, "cool") == 0)     m = AcMode::Cool;
  else if (strcmp(str, "dry") == 0)      m = AcMode::Dry;
  else if (strcmp(str, "fan_only") == 0) m = AcMode::Fan;
  else if (strcmp(str, "heat") == 0)     m = AcMode::Heat;
  else return false;
  s.power = true;
  s.mode = m;
  return true;
}

const char* acFanToString(AcFan f) {
  switch (f) {
    case AcFan::Auto:   return "auto";
    case AcFan::Low:    return "low";
    case AcFan::Medium: return "medium";
    case AcFan::High:   return "high";
  }
  return "auto";
}

bool acFanFromString(const char* str, AcFan& f) {
  if      (strcmp(str, "auto") == 0)   f = AcFan::Auto;
  else if (strcmp(str, "low") == 0)    f = AcFan::Low;
  else if (strcmp(str, "medium") == 0) f = AcFan::Medium;
  else if (strcmp(str, "high") == 0)   f = AcFan::High;
  else return false;
  return true;
}

bool acTempFromPayload(const char* str, AcState& s) {
  char* end = nullptr;
  double v = strtod(str, &end);
  if (end == str) return false;
  s.setTemp(static_cast<int>(v + 0.5));
  return true;
}
