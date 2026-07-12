// Single source of truth for the desired AC state.
// Pure C++ (no Arduino deps) so it can be unit-tested on the host.
// String forms follow Home Assistant MQTT climate payloads:
// modes "off|auto|cool|dry|fan_only|heat", fan "auto|low|medium|high".
#pragma once
#include <stdint.h>

enum class AcMode : uint8_t { Auto, Cool, Dry, Fan, Heat };
enum class AcFan : uint8_t { Auto, Low, Medium, High };

constexpr uint8_t kAcMinTemp = 16;
constexpr uint8_t kAcMaxTemp = 32;

struct AcState {
  bool power = false;
  AcMode mode = AcMode::Cool;
  uint8_t temp = 24;
  AcFan fan = AcFan::Auto;
  bool swing = false;

  void setTemp(int t);   // clamps to [kAcMinTemp, kAcMaxTemp]
  void cycleTemp();      // +1 degree, wraps max -> min (BtnB fallback)

  bool operator==(const AcState& o) const;
  bool operator!=(const AcState& o) const { return !(*this == o); }
};

// "off" when power is false, otherwise the HA name of the mode.
const char* acModeToString(const AcState& s);
// Accepts "off" (clears power, keeps mode) or a mode name (sets power).
// Returns false and leaves state untouched on unknown input.
bool acModeFromString(const char* str, AcState& s);

const char* acFanToString(AcFan f);
bool acFanFromString(const char* str, AcFan& f);

// Parses an HA temperature command payload like "24.0"; rounds and clamps.
bool acTempFromPayload(const char* str, AcState& s);
