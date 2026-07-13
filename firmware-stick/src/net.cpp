#include "net.h"

#include <ArduinoJson.h>
#include <ESPmDNS.h>
#include <Preferences.h>
#include <PubSubClient.h>
#include <WiFi.h>

#include "config.h"

static AcState* g_state = nullptr;
static bool g_dirty = false;

static WiFiClient wifiClient;
static PubSubClient mqtt(wifiClient);

static unsigned long lastWifiAttempt = 0;
static unsigned long lastMqttAttempt = 0;
constexpr unsigned long kWifiRetryMs = 10000;
constexpr unsigned long kMqttRetryMs = 5000;

// STA credentials: NVS (set from the web UI) overrides secrets.h.
static String g_ssid;
static String g_pass;
static bool g_apMode = false;
static bool g_everConnected = false;
static bool g_mdnsUp = false;
static unsigned long bootMs = 0;
constexpr unsigned long kStaFallbackMs = 30000;
constexpr const char* kApSsid = "AC-Remote";
constexpr const char* kApPassword = "12345678";
constexpr const char* kMdnsName = "ac-remote";  // http://ac-remote.local

static void startAp() {
  g_apMode = true;
  WiFi.disconnect(true);
  WiFi.mode(WIFI_AP);
  WiFi.softAP(kApSsid, kApPassword);
  Serial.printf("WiFi: AP fallback '%s' pass '%s' at %s\n", kApSsid,
                kApPassword, WiFi.softAPIP().toString().c_str());
}

static String topic(const char* suffix) {
  return String(DEVICE_ID) + "/" + suffix;
}

static void publishDiscovery() {
  // HA MQTT climate discovery — one retained config message.
  JsonDocument doc;
  doc["name"] = DEVICE_NAME;
  doc["unique_id"] = DEVICE_ID;
  doc["min_temp"] = kAcMinTemp;
  doc["max_temp"] = kAcMaxTemp;
  doc["temp_step"] = 1;
  JsonArray modes = doc["modes"].to<JsonArray>();
  for (const char* m : {"off", "auto", "cool", "dry", "fan_only", "heat"})
    modes.add(m);
  JsonArray fans = doc["fan_modes"].to<JsonArray>();
  for (const char* f : {"auto", "low", "medium", "high"}) fans.add(f);
  JsonArray swings = doc["swing_modes"].to<JsonArray>();
  for (const char* s : {"on", "off"}) swings.add(s);
  doc["mode_command_topic"] = topic("mode/set");
  doc["mode_state_topic"] = topic("mode/state");
  doc["temperature_command_topic"] = topic("temp/set");
  doc["temperature_state_topic"] = topic("temp/state");
  doc["fan_mode_command_topic"] = topic("fan/set");
  doc["fan_mode_state_topic"] = topic("fan/state");
  doc["swing_mode_command_topic"] = topic("swing/set");
  doc["swing_mode_state_topic"] = topic("swing/state");
  doc["availability_topic"] = topic("availability");
  JsonObject dev = doc["device"].to<JsonObject>();
  dev["identifiers"][0] = DEVICE_ID;
  dev["name"] = DEVICE_NAME;
  dev["manufacturer"] = "M5Stack";
  dev["model"] = "StickC Plus2";

  String payload;
  serializeJson(doc, payload);
  String cfgTopic = String("homeassistant/climate/") + DEVICE_ID + "/config";
  mqtt.publish(cfgTopic.c_str(), payload.c_str(), true);
}

static void onMqttMessage(char* t, uint8_t* payload, unsigned int len) {
  if (!g_state) return;
  char msg[32];
  if (len >= sizeof(msg)) return;
  memcpy(msg, payload, len);
  msg[len] = '\0';

  String tp(t);
  bool changed = false;
  AcState before = *g_state;
  if (tp == topic("mode/set")) {
    changed = acModeFromString(msg, *g_state);
  } else if (tp == topic("temp/set")) {
    changed = acTempFromPayload(msg, *g_state);
  } else if (tp == topic("fan/set")) {
    changed = acFanFromString(msg, g_state->fan);
  } else if (tp == topic("swing/set")) {
    g_state->swing = (strcmp(msg, "on") == 0);
    changed = true;
  }
  if (changed && before != *g_state) g_dirty = true;
  // Re-publish even on no-op commands so HA's optimistic UI settles.
  if (changed) netPublishState(*g_state);
}

static void mqttConnect() {
  String avail = topic("availability");
  bool ok;
  if (strlen(MQTT_USER) > 0) {
    ok = mqtt.connect(DEVICE_ID, MQTT_USER, MQTT_PASSWORD, avail.c_str(), 0,
                      true, "offline");
  } else {
    ok = mqtt.connect(DEVICE_ID, avail.c_str(), 0, true, "offline");
  }
  if (!ok) return;
  mqtt.publish(avail.c_str(), "online", true);
  for (const char* s : {"mode/set", "temp/set", "fan/set", "swing/set"})
    mqtt.subscribe(topic(s).c_str());
  publishDiscovery();
  if (g_state) netPublishState(*g_state);
}

void netInit(AcState& state) {
  g_state = &state;
  bootMs = millis();

  Preferences prefs;
  prefs.begin("net", true);
  g_ssid = prefs.getString("ssid", WIFI_SSID);
  g_pass = prefs.getString("pass", WIFI_PASSWORD);
  prefs.end();

  if (g_ssid.length() == 0 || g_ssid == "your-wifi-ssid") {
    // No real credentials anywhere: AP right away, configure via web UI.
    startAp();
  } else {
    WiFi.mode(WIFI_STA);
    WiFi.setHostname(DEVICE_ID);
    WiFi.begin(g_ssid.c_str(), g_pass.c_str());
    lastWifiAttempt = millis();
  }
  // Discovery config (~700B) exceeds PubSubClient's 256B default.
  mqtt.setBufferSize(1024);
  mqtt.setServer(MQTT_HOST, MQTT_PORT);
  mqtt.setCallback(onMqttMessage);
}

void netLoop() {
  unsigned long now = millis();
  if (g_apMode) return;  // AP is static; leave it via web WiFi form + reboot.
  if (WiFi.status() != WL_CONNECTED) {
    g_mdnsUp = false;
    // Never connected with these creds -> they may be wrong; open the AP so
    // the web UI stays reachable. After a drop, keep retrying STA instead.
    if (!g_everConnected && now - bootMs >= kStaFallbackMs) {
      startAp();
      return;
    }
    if (now - lastWifiAttempt >= kWifiRetryMs) {
      lastWifiAttempt = now;
      WiFi.disconnect();
      WiFi.begin(g_ssid.c_str(), g_pass.c_str());
    }
    return;
  }
  g_everConnected = true;
  if (!g_mdnsUp && MDNS.begin(kMdnsName)) {
    MDNS.addService("http", "tcp", 80);
    g_mdnsUp = true;
    Serial.printf("mDNS: http://%s.local\n", kMdnsName);
  }
  if (!mqtt.connected()) {
    if (now - lastMqttAttempt >= kMqttRetryMs) {
      lastMqttAttempt = now;
      mqttConnect();
    }
    return;
  }
  mqtt.loop();
}

bool netWifiUp() { return WiFi.status() == WL_CONNECTED && !g_apMode; }
bool netMqttUp() { return mqtt.connected(); }
bool netApUp() { return g_apMode; }

const char* netIp() {
  static String ip;
  if (g_apMode) ip = WiFi.softAPIP().toString();
  else if (WiFi.status() == WL_CONNECTED) ip = WiFi.localIP().toString();
  else ip = "";
  return ip.c_str();
}

void netSaveCredentials(const char* ssid, const char* pass) {
  Preferences prefs;
  prefs.begin("net", false);
  prefs.putString("ssid", ssid);
  prefs.putString("pass", pass);
  prefs.end();
}

bool netConsumeDirty() {
  bool d = g_dirty;
  g_dirty = false;
  return d;
}

void netPublishState(const AcState& s) {
  if (!mqtt.connected()) return;
  char temp[4];
  snprintf(temp, sizeof(temp), "%d", s.temp);
  mqtt.publish(topic("mode/state").c_str(), acModeToString(s), true);
  mqtt.publish(topic("temp/state").c_str(), temp, true);
  mqtt.publish(topic("fan/state").c_str(), acFanToString(s.fan), true);
  mqtt.publish(topic("swing/state").c_str(), s.swing ? "on" : "off", true);
}
