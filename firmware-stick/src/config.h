// Board pins and credential loading for the M5StickC Plus2 IR bridge.
#pragma once

#if __has_include("secrets.h")
#include "secrets.h"
#else
#include "secrets.example.h"
#endif

// M5StickC Plus2 built-in IR transmitter LED.
constexpr int kIrLedPin = 19;

// Coalesce rapid button presses / MQTT bursts into one IR frame.
constexpr unsigned long kIrSendDebounceMs = 300;

// MQTT topic layout (DEVICE_ID from secrets):
//   <id>/mode/set    <id>/mode/state      "off|auto|cool|dry|fan_only|heat"
//   <id>/temp/set    <id>/temp/state      "16".."32"
//   <id>/fan/set     <id>/fan/state       "auto|low|medium|high"
//   <id>/swing/set   <id>/swing/state     "on|off"
//   <id>/availability                     "online|offline" (LWT)
