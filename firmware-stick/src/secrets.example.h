// Copy this file to secrets.h and fill in your credentials.
// secrets.h is gitignored. If secrets.h is absent the firmware builds with
// these placeholder values (useful for compile checks, not for running).
#pragma once

#define WIFI_SSID       "your-wifi-ssid"
#define WIFI_PASSWORD   "your-wifi-password"

#define MQTT_HOST       "192.168.1.10"   // Home Assistant / broker IP
#define MQTT_PORT       1883
#define MQTT_USER       "mqtt-user"      // leave "" if broker allows anonymous
#define MQTT_PASSWORD   "mqtt-password"

// Base identifier: used for MQTT topics, HA discovery and hostname.
#define DEVICE_ID       "stickc_ac_bridge"
#define DEVICE_NAME     "AC IR Bridge"
