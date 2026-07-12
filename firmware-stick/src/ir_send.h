// Thin wrapper around IRremoteESP8266's IRElectraAc (YKR-* remote family).
#pragma once
#include "ac_state.h"

void irSendInit();
// Transmits the FULL state as one ELECTRA_AC frame (AC remotes are stateless
// on the receiving end — never send incremental button presses).
void irSendState(const AcState& s);
