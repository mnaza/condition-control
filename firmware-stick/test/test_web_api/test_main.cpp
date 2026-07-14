// Host-side tests for the pure web-API logic: query-param application,
// status JSON rendering and the Electra OFF-variant byte patch.
#include <string.h>

#include <unity.h>

#include "../../src/ac_state.h"
#include "../../src/electra_off.h"
#include "../../src/web_api.h"

void setUp() {}
void tearDown() {}

// --- webApplyParam ---------------------------------------------------------

static void test_param_mode_sets_power_and_mode() {
  AcState s;
  TEST_ASSERT_TRUE(webApplyParam("mode", "heat", s));
  TEST_ASSERT_TRUE(s.power);
  TEST_ASSERT_EQUAL(static_cast<int>(AcMode::Heat), static_cast<int>(s.mode));
}

static void test_param_mode_off_clears_power() {
  AcState s;
  s.power = true;
  TEST_ASSERT_TRUE(webApplyParam("mode", "off", s));
  TEST_ASSERT_FALSE(s.power);
}

static void test_param_power_on_off_toggle() {
  AcState s;
  TEST_ASSERT_TRUE(webApplyParam("power", "on", s));
  TEST_ASSERT_TRUE(s.power);
  TEST_ASSERT_TRUE(webApplyParam("power", "off", s));
  TEST_ASSERT_FALSE(s.power);
  TEST_ASSERT_TRUE(webApplyParam("power", "toggle", s));
  TEST_ASSERT_TRUE(s.power);
}

static void test_param_temp_clamped() {
  AcState s;
  TEST_ASSERT_TRUE(webApplyParam("temp", "25", s));
  TEST_ASSERT_EQUAL(25, s.temp);
  TEST_ASSERT_TRUE(webApplyParam("temp", "99", s));
  TEST_ASSERT_EQUAL(kAcMaxTemp, s.temp);
}

static void test_param_fan_and_swing() {
  AcState s;
  TEST_ASSERT_TRUE(webApplyParam("fan", "high", s));
  TEST_ASSERT_EQUAL(static_cast<int>(AcFan::High), static_cast<int>(s.fan));
  TEST_ASSERT_TRUE(webApplyParam("swing", "on", s));
  TEST_ASSERT_TRUE(s.swing);
  TEST_ASSERT_TRUE(webApplyParam("swing", "off", s));
  TEST_ASSERT_FALSE(s.swing);
}

static void test_param_unknown_rejected_state_untouched() {
  AcState s;
  AcState before = s;
  TEST_ASSERT_FALSE(webApplyParam("bogus", "1", s));
  TEST_ASSERT_FALSE(webApplyParam("mode", "bogus", s));
  TEST_ASSERT_FALSE(webApplyParam("fan", "bogus", s));
  TEST_ASSERT_TRUE(s == before);
}

// --- webStatusJson ---------------------------------------------------------

static void test_status_json_exact() {
  AcState s;
  s.power = true;
  s.mode = AcMode::Cool;
  s.temp = 24;
  s.fan = AcFan::Auto;
  s.swing = false;
  char buf[192];
  int n = webStatusJson(s, true, false, 2, buf, sizeof(buf));
  TEST_ASSERT_GREATER_THAN(0, n);
  TEST_ASSERT_EQUAL_STRING(
      "{\"power\":true,\"mode\":\"cool\",\"temp\":24,\"fan\":\"auto\","
      "\"swing\":false,\"wifi\":true,\"mqtt\":false,\"offVariant\":2}",
      buf);
}

static void test_status_json_off_mode_string() {
  AcState s;  // power=false by default
  char buf[192];
  webStatusJson(s, false, false, 0, buf, sizeof(buf));
  TEST_ASSERT_NOT_NULL(strstr(buf, "\"power\":false"));
  TEST_ASSERT_NOT_NULL(strstr(buf, "\"mode\":\"off\""));
}

// --- electraApplyOffVariant -------------------------------------------------

static void test_off_variant_patches() {
  uint8_t st[13];
  memset(st, 0, sizeof(st));
  electraApplyOffVariant(st, 0);  // baseline: no change
  for (int i = 0; i < 13; i++) TEST_ASSERT_EQUAL_HEX8(0x00, st[i]);

  memset(st, 0, sizeof(st));
  electraApplyOffVariant(st, 1);  // byte9 |= 0x10
  TEST_ASSERT_EQUAL_HEX8(0x10, st[9]);
  TEST_ASSERT_EQUAL_HEX8(0x00, st[11]);

  memset(st, 0, sizeof(st));
  electraApplyOffVariant(st, 2);  // byte11 = 0x05
  TEST_ASSERT_EQUAL_HEX8(0x00, st[9]);
  TEST_ASSERT_EQUAL_HEX8(0x05, st[11]);

  memset(st, 0, sizeof(st));
  electraApplyOffVariant(st, 3);  // both
  TEST_ASSERT_EQUAL_HEX8(0x10, st[9]);
  TEST_ASSERT_EQUAL_HEX8(0x05, st[11]);
}

static void test_off_variant_default_is_confirmed_v3() {
  // Live-confirmed on the YKR-L/201E (2026-07): only byte11=0x05 makes the
  // AC accept the power-off frame.
  TEST_ASSERT_EQUAL(2, kElectraOffVariantDefault);
  uint8_t st[13] = {0};
  electraApplyOffVariant(st, kElectraOffVariantDefault);
  TEST_ASSERT_EQUAL_HEX8(0x05, st[11]);
  TEST_ASSERT_EQUAL_HEX8(0x00, st[9]);
}

static void test_off_variant_out_of_range_ignored() {
  uint8_t st[13];
  memset(st, 0xAA, sizeof(st));
  electraApplyOffVariant(st, -1);
  electraApplyOffVariant(st, 4);
  for (int i = 0; i < 13; i++) TEST_ASSERT_EQUAL_HEX8(0xAA, st[i]);
}

int main() {
  UNITY_BEGIN();
  RUN_TEST(test_param_mode_sets_power_and_mode);
  RUN_TEST(test_param_mode_off_clears_power);
  RUN_TEST(test_param_power_on_off_toggle);
  RUN_TEST(test_param_temp_clamped);
  RUN_TEST(test_param_fan_and_swing);
  RUN_TEST(test_param_unknown_rejected_state_untouched);
  RUN_TEST(test_status_json_exact);
  RUN_TEST(test_status_json_off_mode_string);
  RUN_TEST(test_off_variant_patches);
  RUN_TEST(test_off_variant_default_is_confirmed_v3);
  RUN_TEST(test_off_variant_out_of_range_ignored);
  return UNITY_END();
}
