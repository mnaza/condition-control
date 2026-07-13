#include "electra_off.h"

void electraApplyOffVariant(uint8_t* state, int variant) {
  switch (variant) {
    case 1: state[9] |= 0x10; break;
    case 2: state[11] = 0x05; break;
    case 3: state[9] |= 0x10; state[11] = 0x05; break;
    default: break;
  }
}
