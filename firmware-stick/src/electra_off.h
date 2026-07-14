// Power-OFF encodings for the YKR-L/201E, which ignores the stock
// IRElectraAc OFF frame. Byte hints come from the AUX YKR-P/002E
// (arduino-heatpumpir#71). Live test on the real unit (2026-07) confirmed
// variant 2: only byte11=0x05 makes the AC accept power-off. Pure logic,
// host-testable; the checksum is recomputed later by IRElectraAc::setRaw().
#pragma once
#include <stdint.h>

constexpr int kElectraOffVariantCount = 4;
// Confirmed working on the YKR-L/201E (web UI still allows overriding).
constexpr int kElectraOffVariantDefault = 2;

// Patches a 13-byte ELECTRA_AC state buffer for variant 0..3:
//   0: stock library OFF (no change)
//   1: byte9 |= 0x10 (keep bit4 set alongside cleared power bit)
//   2: byte11 = 0x05 (AUX-style light byte) — confirmed on YKR-L/201E
//   3: both
// Out-of-range variants leave the buffer untouched.
void electraApplyOffVariant(uint8_t* state, int variant);
