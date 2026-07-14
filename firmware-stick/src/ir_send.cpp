#include "ir_send.h"

#include <string.h>

#include <IRremoteESP8266.h>
#include <ir_Electra.h>

#include "config.h"
#include "electra_off.h"

static IRElectraAc ac(kIrLedPin);
static int g_offVariant = kElectraOffVariantDefault;

static uint8_t toElectraMode(AcMode m) {
  switch (m) {
    case AcMode::Auto: return kElectraAcAuto;
    case AcMode::Cool: return kElectraAcCool;
    case AcMode::Dry:  return kElectraAcDry;
    case AcMode::Fan:  return kElectraAcFan;
    case AcMode::Heat: return kElectraAcHeat;
  }
  return kElectraAcAuto;
}

static uint8_t toElectraFan(AcFan f) {
  switch (f) {
    case AcFan::Auto:   return kElectraAcFanAuto;
    case AcFan::Low:    return kElectraAcFanLow;
    case AcFan::Medium: return kElectraAcFanMed;
    case AcFan::High:   return kElectraAcFanHigh;
  }
  return kElectraAcFanAuto;
}

void irSendInit() {
  ac.begin();
}

void irSetOffVariant(int v) {
  if (v >= 0 && v < kElectraOffVariantCount) g_offVariant = v;
}

void irSendState(const AcState& s) {
  ac.setPower(s.power);
  ac.setMode(toElectraMode(s.mode));
  ac.setTemp(s.temp);
  ac.setFan(toElectraFan(s.fan));
  ac.setSwingV(s.swing);
  ac.setSwingH(false);
  // Normalize the bytes the OFF-variant experiment touches (they are not
  // managed by the setters above, so a previous patch would linger), then
  // re-apply the selected patch for power-off frames. setRaw()/send()
  // recompute the checksum.
  uint8_t st[kElectraAcStateLength];
  memcpy(st, ac.getRaw(), kElectraAcStateLength);
  st[9] &= ~0x10;
  st[11] = 0x08;  // library stateReset default (light-toggle off)
  if (!s.power) electraApplyOffVariant(st, g_offVariant);
  ac.setRaw(st);
  ac.send();
}
