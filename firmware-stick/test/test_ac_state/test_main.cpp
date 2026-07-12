// Host-side unit tests for AcState (pio test -e native).
#include <unity.h>
#include <string.h>

#include "ac_state.h"

void setUp() {}
void tearDown() {}

// --- defaults & temperature clamping ---

static void test_defaults() {
  AcState s;
  TEST_ASSERT_FALSE(s.power);
  TEST_ASSERT_EQUAL(static_cast<int>(AcMode::Cool), static_cast<int>(s.mode));
  TEST_ASSERT_EQUAL_UINT8(24, s.temp);
  TEST_ASSERT_EQUAL(static_cast<int>(AcFan::Auto), static_cast<int>(s.fan));
  TEST_ASSERT_FALSE(s.swing);
}

static void test_set_temp_clamps_low() {
  AcState s;
  s.setTemp(5);
  TEST_ASSERT_EQUAL_UINT8(kAcMinTemp, s.temp);
}

static void test_set_temp_clamps_high() {
  AcState s;
  s.setTemp(40);
  TEST_ASSERT_EQUAL_UINT8(kAcMaxTemp, s.temp);
}

static void test_set_temp_in_range() {
  AcState s;
  s.setTemp(21);
  TEST_ASSERT_EQUAL_UINT8(21, s.temp);
}

static void test_cycle_temp_increments_and_wraps() {
  AcState s;
  s.setTemp(kAcMaxTemp - 1);
  s.cycleTemp();
  TEST_ASSERT_EQUAL_UINT8(kAcMaxTemp, s.temp);
  s.cycleTemp();
  TEST_ASSERT_EQUAL_UINT8(kAcMinTemp, s.temp);
}

// --- HA climate mode strings ("off" folds power into mode) ---

static void test_mode_to_string_off_when_power_off() {
  AcState s;
  s.power = false;
  s.mode = AcMode::Heat;
  TEST_ASSERT_EQUAL_STRING("off", acModeToString(s));
}

static void test_mode_to_string_named_modes() {
  AcState s;
  s.power = true;
  s.mode = AcMode::Cool;
  TEST_ASSERT_EQUAL_STRING("cool", acModeToString(s));
  s.mode = AcMode::Fan;
  TEST_ASSERT_EQUAL_STRING("fan_only", acModeToString(s));
  s.mode = AcMode::Dry;
  TEST_ASSERT_EQUAL_STRING("dry", acModeToString(s));
  s.mode = AcMode::Heat;
  TEST_ASSERT_EQUAL_STRING("heat", acModeToString(s));
  s.mode = AcMode::Auto;
  TEST_ASSERT_EQUAL_STRING("auto", acModeToString(s));
}

static void test_mode_from_string_off_clears_power() {
  AcState s;
  s.power = true;
  TEST_ASSERT_TRUE(acModeFromString("off", s));
  TEST_ASSERT_FALSE(s.power);
}

static void test_mode_from_string_sets_power_and_mode() {
  AcState s;
  TEST_ASSERT_TRUE(acModeFromString("heat", s));
  TEST_ASSERT_TRUE(s.power);
  TEST_ASSERT_EQUAL(static_cast<int>(AcMode::Heat), static_cast<int>(s.mode));
  TEST_ASSERT_TRUE(acModeFromString("fan_only", s));
  TEST_ASSERT_EQUAL(static_cast<int>(AcMode::Fan), static_cast<int>(s.mode));
}

static void test_mode_from_string_rejects_unknown() {
  AcState s;
  AcState before = s;
  TEST_ASSERT_FALSE(acModeFromString("banana", s));
  TEST_ASSERT_TRUE(before == s);
}

// --- fan strings ---

static void test_fan_round_trip() {
  const char* names[] = {"auto", "low", "medium", "high"};
  for (const char* n : names) {
    AcFan f;
    TEST_ASSERT_TRUE(acFanFromString(n, f));
    TEST_ASSERT_EQUAL_STRING(n, acFanToString(f));
  }
  AcFan f;
  TEST_ASSERT_FALSE(acFanFromString("turbo", f));
}

// --- temperature command payload ("24.0" from HA) ---

static void test_temp_from_payload() {
  AcState s;
  TEST_ASSERT_TRUE(acTempFromPayload("21.0", s));
  TEST_ASSERT_EQUAL_UINT8(21, s.temp);
  TEST_ASSERT_TRUE(acTempFromPayload("26.6", s));   // rounds
  TEST_ASSERT_EQUAL_UINT8(27, s.temp);
  TEST_ASSERT_TRUE(acTempFromPayload("99", s));     // clamps
  TEST_ASSERT_EQUAL_UINT8(kAcMaxTemp, s.temp);
  TEST_ASSERT_FALSE(acTempFromPayload("abc", s));
}

// --- equality (used by main loop to detect dirty state) ---

static void test_equality() {
  AcState a, b;
  TEST_ASSERT_TRUE(a == b);
  b.setTemp(25);
  TEST_ASSERT_FALSE(a == b);
}

int main(int, char**) {
  UNITY_BEGIN();
  RUN_TEST(test_defaults);
  RUN_TEST(test_set_temp_clamps_low);
  RUN_TEST(test_set_temp_clamps_high);
  RUN_TEST(test_set_temp_in_range);
  RUN_TEST(test_cycle_temp_increments_and_wraps);
  RUN_TEST(test_mode_to_string_off_when_power_off);
  RUN_TEST(test_mode_to_string_named_modes);
  RUN_TEST(test_mode_from_string_off_clears_power);
  RUN_TEST(test_mode_from_string_sets_power_and_mode);
  RUN_TEST(test_mode_from_string_rejects_unknown);
  RUN_TEST(test_fan_round_trip);
  RUN_TEST(test_temp_from_payload);
  RUN_TEST(test_equality);
  return UNITY_END();
}
