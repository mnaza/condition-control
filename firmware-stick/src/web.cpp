#include "web.h"

#include <Preferences.h>
#include <WebServer.h>

#include "electra_off.h"
#include "ir_send.h"
#include "net.h"
#include "web_api.h"

static AcState* g_state = nullptr;
static bool g_dirty = false;
static WebServer server(80);
static Preferences prefs;
static int g_offVariant = kElectraOffVariantDefault;

// Russian UI; values in the JS mirror the HA payload strings that
// webApplyParam/ac_state understand.
static const char kIndexHtml[] PROGMEM = R"HTML(<!doctype html>
<html lang="ru"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>AC пульт</title>
<style>
 body{font-family:system-ui,sans-serif;background:#111;color:#eee;margin:0;
      max-width:420px;margin:0 auto;padding:12px}
 h1{font-size:1.1em;color:#8cf;display:flex;justify-content:space-between}
 h1 small{font-weight:normal;color:#888}
 .temp{font-size:3.5em;text-align:center;margin:8px 0}
 .temp.off{color:#555}
 .row{display:flex;gap:6px;margin:10px 0;flex-wrap:wrap}
 .row span.lbl{flex-basis:100%;color:#888;font-size:.85em}
 button{flex:1;padding:12px 4px;font-size:1em;border:1px solid #444;
        border-radius:8px;background:#222;color:#eee;cursor:pointer}
 button.on{background:#2a6;border-color:#2a6;color:#fff}
 button.pwr{padding:16px;font-size:1.2em}
 button.pwr.on{background:#c33;border-color:#c33}
 details{margin-top:18px;border-top:1px solid #333;padding-top:8px}
 summary{color:#888;cursor:pointer}
 input,select{width:100%;padding:8px;margin:4px 0;background:#222;
              color:#eee;border:1px solid #444;border-radius:6px;box-sizing:border-box}
 .hint{color:#777;font-size:.8em}
</style></head><body>
<h1>Кондиционер <small id="link"></small></h1>
<div class="temp off" id="temp">--</div>
<div class="row"><button class="pwr" id="pwr" onclick="send('power','toggle')">Питание</button></div>
<div class="row"><span class="lbl">Режим</span>
 <button data-k="mode" data-v="cool" onclick="send('mode','cool')">Холод</button>
 <button data-k="mode" data-v="heat" onclick="send('mode','heat')">Тепло</button>
 <button data-k="mode" data-v="dry" onclick="send('mode','dry')">Осушение</button>
 <button data-k="mode" data-v="fan_only" onclick="send('mode','fan_only')">Вент.</button>
 <button data-k="mode" data-v="auto" onclick="send('mode','auto')">Авто</button>
</div>
<div class="row"><span class="lbl">Температура</span>
 <button onclick="bump(-1)">&minus;</button>
 <button onclick="bump(1)">+</button>
</div>
<div class="row"><span class="lbl">Вентилятор</span>
 <button data-k="fan" data-v="auto" onclick="send('fan','auto')">Авто</button>
 <button data-k="fan" data-v="low" onclick="send('fan','low')">Мин</button>
 <button data-k="fan" data-v="medium" onclick="send('fan','medium')">Сред</button>
 <button data-k="fan" data-v="high" onclick="send('fan','high')">Макс</button>
</div>
<div class="row"><span class="lbl">Жалюзи</span>
 <button id="swing" onclick="send('swing',st.swing?'off':'on')">Качание</button>
</div>
<details><summary>Настройки</summary>
 <p class="hint">Вариант кодировки выключения (v3 подтверждён на YKR-L/201E):</p>
 <select id="offv" onchange="fetch('/api/offvariant?v='+this.value).then(r=>r.json()).then(render)">
  <option value="0">v1 — стандартный (библиотека)</option>
  <option value="1">v2 — байт9 |= 0x10</option>
  <option value="2">v3 — байт11 = 0x05 (рабочий)</option>
  <option value="3">v4 — оба</option>
 </select>
 <p class="hint">Wi-Fi (сохранить и перезагрузить):</p>
 <form onsubmit="event.preventDefault();fetch('/api/wifi',{method:'POST',
   body:new FormData(this)}).then(()=>alert('Перезагружаюсь...'))">
  <input name="ssid" placeholder="SSID" required>
  <input name="pass" placeholder="Пароль" type="password">
  <button>Сохранить и перезагрузить</button>
 </form>
 <p class="hint">MQTT / Home Assistant (пустой адрес — MQTT выключен):</p>
 <form id="mqf" onsubmit="event.preventDefault();fetch('/api/mqtt',{method:'POST',
   body:new FormData(this)}).then(r=>r.ok?alert('Перезагружаюсь...'):alert('Неверный порт'))">
  <input name="host" placeholder="Адрес брокера (IP или имя)">
  <input name="port" placeholder="Порт (1883)" inputmode="numeric">
  <input name="user" placeholder="Логин (не обязателен)">
  <input name="pass" placeholder="Пароль" type="password">
  <button>Сохранить и перезагрузить</button>
 </form>
</details>
<script>
let st={};
function render(s){st=s;
 const t=document.getElementById('temp');
 t.textContent=s.temp+'°C';
 t.className='temp'+(s.power?'':' off');
 document.getElementById('pwr').className='pwr'+(s.power?' on':'');
 document.getElementById('pwr').textContent=s.power?'Выключить':'Включить';
 document.querySelectorAll('button[data-k]').forEach(b=>{
  b.className=(st[b.dataset.k]===b.dataset.v)?'on':'';});
 document.getElementById('swing').className=s.swing?'on':'';
 document.getElementById('offv').value=s.offVariant;
 document.getElementById('link').textContent=
  (s.wifi?'Wi-Fi':'AP')+(s.mqtt?' + MQTT':'');
}
function send(k,v){fetch('/api/set?'+k+'='+v).then(r=>r.json()).then(render);}
function bump(d){send('temp',st.temp+d);}
fetch('/api/status').then(r=>r.json()).then(render);
setInterval(()=>fetch('/api/status').then(r=>r.json()).then(render),5000);
fetch('/api/mqtt').then(r=>r.json()).then(m=>{const f=document.getElementById('mqf');
 f.host.value=m.host;f.port.value=m.port;f.user.value=m.user;});
</script></body></html>)HTML";

static void sendStatus() {
  char buf[192];
  webStatusJson(*g_state, netWifiUp(), netMqttUp(), g_offVariant, buf,
                sizeof(buf));
  server.send(200, "application/json", buf);
}

static void handleSet() {
  bool changed = false;
  AcState before = *g_state;
  for (int i = 0; i < server.args(); i++) {
    changed |= webApplyParam(server.argName(i).c_str(),
                             server.arg(i).c_str(), *g_state);
  }
  if (changed && before != *g_state) g_dirty = true;
  sendStatus();
}

static void handleOffVariant() {
  int v = server.arg("v").toInt();
  if (v >= 0 && v < kElectraOffVariantCount) {
    g_offVariant = v;
    prefs.putInt("offv", v);
    irSetOffVariant(v);
  }
  sendStatus();
}

static void handleMqttGet() {
  char buf[192];
  snprintf(buf, sizeof(buf), "{\"host\":\"%s\",\"port\":%u,\"user\":\"%s\"}",
           netMqttHost(), netMqttPort(), netMqttUser());
  server.send(200, "application/json", buf);
}

static void handleMqttPost() {
  String portArg = server.arg("port");
  uint16_t port = 1883;
  if (portArg.length() > 0) {
    int p = webParsePort(portArg.c_str());
    if (p == 0) {
      server.send(400, "text/plain", "bad port");
      return;
    }
    port = static_cast<uint16_t>(p);
  }
  netSaveMqtt(server.arg("host").c_str(), port, server.arg("user").c_str(),
              server.arg("pass").c_str());
  server.send(200, "text/plain", "rebooting");
  delay(500);
  ESP.restart();
}

static void handleWifi() {
  String ssid = server.arg("ssid");
  if (ssid.length() == 0) {
    server.send(400, "text/plain", "ssid required");
    return;
  }
  netSaveCredentials(ssid.c_str(), server.arg("pass").c_str());
  server.send(200, "text/plain", "rebooting");
  // One-shot blocking path: flush the response, then restart.
  delay(500);
  ESP.restart();
}

void webInit(AcState& state) {
  g_state = &state;
  prefs.begin("web", false);
  g_offVariant = prefs.getInt("offv", kElectraOffVariantDefault);
  irSetOffVariant(g_offVariant);

  server.on("/", HTTP_GET,
            []() { server.send_P(200, "text/html", kIndexHtml); });
  server.on("/api/status", HTTP_GET, sendStatus);
  server.on("/api/set", HTTP_GET, handleSet);
  server.on("/api/offvariant", HTTP_GET, handleOffVariant);
  server.on("/api/wifi", HTTP_POST, handleWifi);
  server.on("/api/mqtt", HTTP_GET, handleMqttGet);
  server.on("/api/mqtt", HTTP_POST, handleMqttPost);
  // AP-mode convenience: any unknown URL lands on the control page.
  server.onNotFound([]() {
    server.sendHeader("Location", "/");
    server.send(302, "text/plain", "");
  });
  server.begin();
}

void webLoop() { server.handleClient(); }

bool webConsumeDirty() {
  bool d = g_dirty;
  g_dirty = false;
  return d;
}
