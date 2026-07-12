// IR sniffer based on IRrecvDumpV2: decodes frames from the original remote
// and prints protocol, state bytes and human-readable AC settings over serial.
// Hardware: IR receiver (TSOP38238 or M5 IR Unit) on the Grove port, data G33.
#include <M5Unified.h>
#include <IRremoteESP8266.h>
#include <IRrecv.h>
#include <IRac.h>
#include <IRutils.h>

constexpr uint16_t kRecvPin = 33;       // Grove G33 (white wire on IR Unit)
constexpr uint16_t kCaptureBufferSize = 1024;
constexpr uint8_t kFrameTimeoutMs = 50;  // long enough for slow AC protocols

static IRrecv irrecv(kRecvPin, kCaptureBufferSize, kFrameTimeoutMs, true);
static decode_results results;

void setup() {
  auto cfg = M5.config();
  M5.begin(cfg);
  Serial.begin(115200);
  M5.Display.setRotation(1);
  M5.Display.setTextFont(2);
  M5.Display.fillScreen(TFT_BLACK);
  M5.Display.drawString("IR sniffer on G33", 8, 8);
  M5.Display.drawString("Point remote, press keys", 8, 28);

  irrecv.setUnknownThreshold(12);  // ignore short noise bursts
  irrecv.enableIRIn();
  Serial.println("IR sniffer ready (pin 33). Waiting for frames...");
}

void loop() {
  M5.update();
  if (!irrecv.decode(&results)) return;

  // Full dump: protocol, hex code, raw timings — paste this into an issue
  // or use it to pick the right IRremoteESP8266 class.
  Serial.println(resultToHumanReadableBasic(&results));
  String desc = IRAcUtils::resultAcToString(&results);
  if (desc.length()) Serial.println("Decoded: " + desc);
  Serial.println(resultToSourceCode(&results));

  M5.Display.fillRect(0, 50, M5.Display.width(), 40, TFT_BLACK);
  M5.Display.setCursor(8, 54);
  M5.Display.printf("Got: %s", typeToString(results.decode_type).c_str());

  irrecv.resume();
}
