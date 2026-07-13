// Experimental power-OFF encodings for the YKR-L/201E (bug: the AC ignores
// the stock IRElectraAc OFF frame). Byte hints come from the AUX YKR-P/002E
// (arduino-heatpumpir#71). Pure logic, host-testable; the checksum is
// recomputed later by IRElectraAc::setRaw()/send().
#pragma once
#include <stdint.h>

constexpr int kElectraOffVariantCount = 4;

// Patches a 13-byte ELECTRA_AC state buffer for variant 0..3:
//   0: stock library OFF (no change)
//   1: byte9 |= 0x10 (keep bit4 set alongside cleared power bit)
//   2: byte11 = 0x05 (AUX-style light byte)
//   3: both
// Out-of-range variants leave the buffer untouched.
void electraApplyOffVariant(uint8_t* state, int variant);
