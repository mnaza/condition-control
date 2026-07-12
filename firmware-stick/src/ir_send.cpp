#include "ir_send.h"

#include <IRremoteESP8266.h>
#include <ir_Electra.h>

#include "config.h"

static IRElectraAc ac(kIrLedPin);

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

void irSendState(const AcState& s) {
  ac.setPower(s.power);
  ac.setMode(toElectraMode(s.mode));
  ac.setTemp(s.temp);
  ac.setFan(toElectraFan(s.fan));
  ac.setSwingV(s.swing);
  ac.setSwingH(false);
  ac.send();
}
