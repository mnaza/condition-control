# The root cause I was certain of was wrong

*A note on an ESP32 IR bug, a very good theory, and the measurement that killed it.*

I have a small Rust firmware that turns an M5StickC Plus2 into a Wi-Fi bridge for an air conditioner. It
speaks the AC's IR protocol from scratch on the ESP32's RMT peripheral — 38 kHz carrier, 13-byte Electra
frames, 646 µs marks, 1647/547 µs spaces.

For a while it had the worst kind of bug: it worked whenever I was testing it, and stopped working when I
wasn't. Press the button, the AC responds. Leave it alone for an hour, come back, press the button — nothing.
Sometimes. The IR LED visibly fired either way; a phone camera showed the flash both when the AC obeyed and
when it ignored the frame.

## The theory

Rather than guess, I went and read ESP-IDF v5.2.3.

The firmware enables dynamic frequency scaling and light sleep via `esp_pm_configure(160/40 MHz)`. The RMT
channel is clocked from APB. And here is the thing I found: the **legacy** RMT driver, which this firmware
uses, contains no power-management code at all — `grep pm_lock driver/deprecated/rmt_legacy.c` returns
nothing. The newer driver does, and it takes `ESP_PM_APB_FREQ_MAX` for APB-clocked channels, with a comment
explaining exactly why: *APB clock frequency can be changed during DFS.*

On the ESP32, APB runs at 80 MHz while the CPU is at 80 MHz or above. Drop the CPU to its 40 MHz minimum and
APB follows it down. Everything clocked from APB stretches by a factor of two.

So: whenever the CPU idled at 40 MHz, every mark and space in my frame would double, and the 38 kHz carrier
would fall to about 19 kHz. The LED would still light — an LED does not care about the carrier — so a phone
camera would show a perfectly healthy flash. But the AC's receiver is a band-pass demodulator centred on
38 kHz. At 19 kHz it hears nothing. The frame is transmitted, visible, and silently discarded.

That explains the intermittency exactly. The commands landed when the CPU happened to be busy, which is
precisely when I was standing there poking at it over Wi-Fi, and failed when the device was idle, which is
every other moment of its life.

I took an `ESP_PM_APB_FREQ_MAX` lock around each transmission, mirroring what the newer driver does, released
it on the error path so a failed send could not pin the clock high forever, tagged v0.3.27, and wrote "root
cause of the AC ignoring commands" in the commit message.

It is a good theory. It is mechanistically sound, it is supported by the vendor's own source, and it accounts
for every observed symptom. It is also wrong.

## The measurement

What nagged me was that I had never actually *seen* the CPU at 40 MHz. I had seen a plausible mechanism and a
symptom that matched it, which is not the same thing.

So I added two fields to `/api/health`: `cpuBefore`, the CPU clock sampled immediately before the APB lock is
taken, and `cpuDuring`, sampled again with the lock held.

Both read 160 MHz.

Not just when I hit the endpoint over HTTP — that would prove nothing, since the request itself wakes
everything up. Also on the scheduler path, after four minutes of complete network silence, with nothing
happening that could have boosted the clock.

The CPU never drops to 40 MHz. The Wi-Fi driver evidently holds a power-management lock of its own for as
long as the station is associated, so DFS never gets to take the clock down in the first place. APB was at
80 MHz the whole time. The carrier was 38 kHz the whole time. Dynamic frequency scaling was never corrupting
anything.

I kept the lock. It is what the newer driver does, it costs nothing while Wi-Fi is already pinning the clock,
and it keeps the timing correct if that ever stops being true — Wi-Fi off, AP mode, a future IDF that changes
the locking behaviour. But it is insurance, not a fix, and the commit that added it now has a successor
commit saying so.

## The part that should worry you

Here is what makes this worth writing down rather than quietly amending.

The symptom is gone. The device works. And I still do not know why.

Between the failures and now, several things shipped. One of them, v0.3.25, made the firmware send each state
frame **twice, 40 ms apart** — safe to do because every protocol here encodes state absolutely rather than as
a toggle, so a repeat is idempotent. It gives a weak link a second chance to be decoded. That change would
mask an intermittent single-frame loss no matter what was causing it.

So the sequence available to me was:

1. Observe an intermittent failure.
2. Find a mechanism that explains it beautifully and is backed by the vendor's source.
3. Ship a fix for that mechanism.
4. Observe that the problem is gone.
5. Conclude I was right.

Step 4 was true. Step 5 would have been false, and nothing in the experience of steps 1 through 4 would have
told me so. A plausible mechanism plus a symptom that stops is not evidence. It is the shape evidence takes
when you have stopped looking.

The only thing that broke the loop was going back to something I had already declared solved and measuring
the one quantity the whole theory rested on. That took an afternoon and produced no new features. It is the
most useful work I did on this project.

## Where it stands

The real cause of the original intermittency is **still unknown**. The candidates I have not ruled out include
a marginal emitter, a receiver more sensitive to duty cycle or alignment than I assumed, and something in the
main loop that was stalling transmission — v0.3.24 fixed a dead MQTT broker blocking the loop, which is at
least as plausible a real fix as anything else that shipped in that window.

What I have now is a device that works, an explanation I know to be false, and instrumentation that will tell
me the truth the next time it breaks. That is a better position than the one I was in when I thought I had
solved it.

---

*Firmware: [github.com/mnaza/condition-control](https://github.com/mnaza/condition-control) — Rust, ESP32,
IR reimplemented against IRremoteESP8266, signed OTA, Home Assistant discovery. The commits in question are
`5b9ba83` (the claim) and `222f115` (the retraction).*
