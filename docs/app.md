# Panel app (`app/`)

Native Rust app with the Slint UI toolkit. It draws a Home Assistant dashboard straight to the display and reads the touchscreen itself: no browser, no compositor, no GPU stack.

## Dashboard

- **Pairing by QR code** (no token typing): on first start the panel shows a QR code with Home Assistant's normal login page (OAuth authorization-code flow, the panel is the IndieAuth client: `client_id` `http://<panel-ip>:8080/`, `redirect_uri` `http://<panel-ip>:8080/auth/callback`; Home Assistant accepts local IP client IDs). After the login on the phone, Home Assistant redirects the phone to the panel, which exchanges the one-time code for a refresh token. The token is stored on the panel only (`/mnt/persist/nspanel/ha_auth.json`, root-only); it can be revoked in the Home Assistant user profile, after which the panel shows the QR code again.
- **Rendering**: the app loads a Home Assistant dashboard config (`lovelace/config`) and renders it natively; only the content comes from Home Assistant, the look is the app's own. Supported: `heading`, `tile`/`entity`/`button`/`light`/`sensor`, `entities`, stacks/grids (flattened), `thermostat`, `media-control`, `picture-entity`/`picture-glance` (camera), and `weather`, `glance`, `gauge`, `weather-forecast` as tiles. Other card types are skipped. A dashboard with several views gets tabs under the top bar. Dashboard edits appear at once (reload on `lovelace_updated`).
- **Live states**: `subscribe_entities` for exactly the dashboard's entities (compressed updates), so the panel does not parse every state change in the house.
- **Control**: tap toggles lights/switches/TVs; shutters have open/stop/close; climate has −/+ and tap on the icon switches off/heat; media has previous/play-pause/next and volume −/+.
- **Details layer** ("more info"): tap a tile (the icon still toggles). Lights: on/off, brightness, colour temperature (kelvin, from the light's range), colour (hue); shutters: position slider + open/stop/close; climate: target ±, hvac mode, fan, preset and swing chips (whatever the entity offers); media: play controls, volume slider, source chips.
- **Camera**: snapshots through Home Assistant's camera proxy, resized by Home Assistant (480×270 in about 0.14 s from a UniFi Protect medium channel). About 1 frame/s while the camera row is on screen, as fast as possible in full screen (tap the picture), nothing while it is scrolled away.
- **Theme**: dark, light, or auto (the default, from the first start, setup and pairing screens included): the light sensor decides, dark below 10 lx, light above 30 lx, unchanged in between, and a change only after the light stayed on the other side for 10 s; the first decision is immediate, before the first frame. Without a sensor reading Home Assistant's `sun.sun` decides, otherwise dark.
- **Settings** (gear in the top bar): theme, automatic or manual brightness, screensaver timeout and style (clock / screen off), wake on approach, display mode, dashboard choice (from Home Assistant's dashboard list), volume, about (version, IP, uptime, memory, SoC temperature, light level), restart app, reboot, log out. Saved in `/mnt/persist/nspanel/settings.json`.
- **Screensaver**: big clock, date and weather (`weather_entity` in the settings file), or screen off; wakes on tap (on release, so the tap does not reach the dashboard) or on approach.
- **Brightness**: one scale for automatic and manual brightness, even in perceived brightness (gamma 2.2), from 0.8 % (still readable at night, no flicker) to 100 %. Automatic: STK3x3x ambient light, 1 lx and darker 0.8 %, 10 lx 16 %, 50 lx 41 %, 150 lx 66 %, 500 lx and brighter 100 %; the sensor's scale is switched automatically because it saturates at about 6500 lx on its most sensitive scale. Manual: 1..100 % on the panel and in Home Assistant. Changes fade every 20 ms in perceived brightness (about 0.8 s down, 0.25 s up, 0.12 s when waking from the screensaver).
- **Approach wake**: the proximity sensor runs at its most sensitive gain, 11.84 ms integration time and the IR LED at full power (`hw.rs`). The app learns the resting level and its jitter (the mean change between consecutive resting readings) and reports an approach after two readings, half a second apart, above the resting level plus the signal of a hand at the wake distance (`PROXIMITY_WAKE_CM`, 45 cm, from the measured distance curve), or plus 5 times the jitter if that is more: noisy readings shorten the range instead of waking the panel. A slow approach does not raise the threshold, since it changes each reading only a little, and readings above half the threshold teach nothing (a hand held just below it would pull it up); whatever stays in front of the sensor for over a minute becomes the new resting level. Measurements and the test view: `docs/bringup.md`.
- Icons: Material Design Icons 7.4.47 (the set Home Assistant uses), `mdi:` names from the entity or Home Assistant-like defaults per domain.

## ESPHome native API

- The app is an ESPHome API server on TCP 6053 (`esphome.rs`; transport in `api_link.rs`): only Home Assistant's address may connect, and Home Assistant gives the panel a key after adding it and connects encrypted from then on (Noise, as ESPHome devices; details in [security.md](security.md)). Message types are generated from `app/proto/api.proto` (aioesphomeapi, MIT). Home Assistant's built-in ESPHome integration adds the panel by IP, or by discovery.
- mDNS (`app/src/mdns.rs`, no system daemon): the panel announces `nspanel-pro._esphomelib._tcp` (port 6053; TXT `friendly_name`, `version`, `mac`, `network`, `project_name`, `project_version`, the same values as in the API's device info), answers queries for it and for `nspanel-pro.local`, and announces again after an address change. Home Assistant matches the `mac` record with a configured device and only updates its address. mDNS stays inside one network segment unless the router repeats it.
- Entities: Screen (switch: off = screensaver now), Dashboard (select, options = the Home Assistant dashboards), Theme, Screensaver after, Screensaver style, Display mode (selects), Brightness, Volume (numbers), Automatic brightness, Wake on approach, Mute microphone (switches), Illuminance, SoC temperature, Wi-Fi signal, Wake level, Wake SNR, Wake score (sensors; the wake values are sent with every wake word before the voice run starts, so Home Assistant can let the satellite that heard best answer: level = RMS of the last second in dBFS, SNR = loudest 100 ms of the last second over the second before it in dB, score = the model's averaged probability), Presence (binary sensor from the proximity sensor), IP address, App version (text sensors), Restart app, Reboot (buttons), plus the voice satellite entities Home Assistant creates itself.
- `src/esphome.rs`: framing, entity list, state diffing (changes pushed when the UI thread notifies), commands mapped to the same setting keys as the on-panel settings screen.
- Recorded sensors are sent only after a set change (`report_device` in `main.rs`, steps in `settings.rs` `Reporting`, set in the integration's options): illuminance by a share of the last value sent (default 15 %, at least a third of it in lux), SoC temperature (1 °C) and Wi-Fi signal (3 dB) on five-minute averages of once-a-minute readings. Details in [home_assistant.md](home_assistant.md#sensor-reporting).
- Start-up: the server accepts Home Assistant's connection only after the dashboard list is known (at most 20 s), so the Dashboard select is complete at once.
- rtw88 does not fill `/proc/net/wireless`; the Wi-Fi signal comes from `iw dev wlan0 link`.

## Voice satellite

- The panel is an Assist satellite through the same ESPHome API connection (`VOICE_FEATURES` = voice assistant, speaker, API audio, timers, announcements; Home Assistant only offers the wake word selects when the announce flag is set). Home Assistant shows it as an assist satellite with Assistant / Assistant 2 and Wake word / Wake word 2 selects, like ESPHome voice satellites.
- **Wake words** run on the panel: microWakeWord models downloaded by `tools/build_image.sh` into `/usr/share/nspanel/wakewords`: `okay_nabu` (OHF-Voice release 20241226.3, cutoff 0.85), `hey_jarvis`, `hey_mycroft`, `alexa`, and the experimental `hey_home_assistant`, `okay_computer`, `choo_choo_homie`, `hey_peppa_pig`. The internal `stop` model is not offered in Home Assistant: it listens only while the panel speaks or a timer rings (and 2 s after), and stops it. Home Assistant's Wake word selects list what the panel reports; any microWakeWord model (`<id>.tflite` + `<id>.json`) in that folder appears there. Only enabled models run: the idle app takes 6.9 % of one core with two wake words. TensorFlow Lite (Buildroot package) and the TFLite micro frontend are compiled into the app (`app/native/`).
- **Wake word reset**: the streaming models keep their recent input inside the model (resource variables), so after a conversation the wake word that started it would fire again as soon as listening resumes. `mww_reset` feeds 3 s of silence features through every model.
- **Echo cancellation**: WebRTC AudioProcessing 1.3 (AEC3 + high-pass filter, Buildroot package `webrtc-audio-processing`, `app/native/aec.cc`), 16 kHz mono, 10 ms frames. The reference is the player's own output (written to ALSA in 10 ms pieces, 48 kHz → 16 kHz, ALSA buffer 100 ms). Measured: microphone level 15–28 dB lower during a spoken answer; the panel's own voice saying a wake word does not wake it. It runs only while the speaker plays and 2 s after (continuously it would cost about a quarter of a core). With it the wake word also works while the panel speaks: it cuts the answer off and starts a new conversation. Noise suppression is off (the wake word frontend has its own; Home Assistant's STT gets the echo-free voice).
- **Audio**: in from the PDM microphones (`hw:CARD=pdmmics`), 4 channels at 16 kHz, the mono signal is the mean of channels 2 and 3 with a fixed gain; out to `hw:CARD=rk809` at 48 kHz (chimes, TTS, timer beeps; 16 kHz, WAV and MP3 sources are resampled; MP3 decoded with `minimp3`).
- **Streamed answers**: Home Assistant sends the spoken answer over the API about as fast as it is spoken, with waits of up to about 0.7 s between chunks; the panel starts it once 0.8 s of audio is in (or after 1.5 s), and logs the chunk statistics, the longest wait (as the API receives the audio and as the voice thread does) and speaker underruns. The conversation stays on screen until the speech ends.
- **Flow**: wake word → `VoiceAssistantRequest` → audio over the API until Home Assistant's VAD ends the run → events (STT text, intent, TTS start/end) drive the conversation display. `continue_conversation` opens the microphone again for a follow-up. If nobody starts to speak within 5 s of a follow-up, the panel aborts the run (`VoiceAssistantRequest start=false`: Home Assistant cancels the pipeline, no answer); otherwise Home Assistant's segmenter would wait 15 s. Speech start is Home Assistant's STT-VAD-start event or the panel's own level detector (10 ms frames 12 dB over the room level for ~150 ms; the room level is followed while the panel is idle and silent), so a reply that begins just before the 5 s mark is not cut off. Home Assistant's API TTS stream can be empty (a WAV of unknown length), so the panel fetches the TTS URL from the TTS-end event itself (`/api/tts_proxy/<token>.wav`).
- **Timers** (Assist timer events): nearest timer in the top bar, full-screen alarm with beeps for at most a minute. While it rings the panel listens without a wake word, run after run, like ESPHome satellites: a short sentence (at most 4 words, such as "stop") stops it at once, and the "Stop" model also works.
- **Announcements** (`assist_satellite.announce`) play and report back.
- **Volume**: settings → Sound → Volume and the `Volume` number entity in Home Assistant (0–100 %, step 5, default 80 %). It sets the RK809 "Master Playback Volume": 100 % = 0 dB, −0.4 dB per percent, 0 % = off. A short chime plays at the new level.
- **Mute microphone**: switch in Home Assistant and on the panel (red icon in the top bar).
- Logging and testing: every pipeline event is logged (`Assist event <type> key=value…`, `voice: run end, follow-up yes/no`). `/tmp/nspanel-voice-test.wav` (16 kHz mono) is sent as the speech after the next wake word, `/tmp/nspanel-voice-followup.wav` as the speech of the next follow-up, so whole conversations can be replayed. `NSPANEL_AEC_DUMP=<file>` writes microphone and processed samples interleaved.

## Assistant mode

A second display mode next to the native dashboard, in the style of a smart display; chosen on the panel (Settings → Display mode) or with the `Display mode` select in Home Assistant.

- **Home screen**: wake word hint (or "Microphone off"), big clock and long date, current weather (condition, ↑/↓ for today, humidity, wind), next 5 hours (forecasts through `weather/subscribe_forecast`; the days are in the weather layer), media card for the chosen player (artwork from `entity_picture`, JPEG only, scaled down on the panel), quick tiles (same rows as the dashboard: pairs, shutters, thermostats, players). Buttons: dashboard (opens the native dashboard; it returns home after a minute untouched or with the home button), settings.
- **Layers**: weather (5 hours + up to 7 days), media (artwork, previous/play/next, volume), full-screen conversation (moving bars while listening/thinking, what was heard, the answer in big text; tap to close), message, camera.
- **Configuration** comes from the Home Assistant integration in `custom_components/nspanel_pro` (see [home_assistant.md](home_assistant.md)): weather, media player, quick tiles, full-screen conversation. The panel subscribes with `nspanel_pro/subscribe` and caches the last configuration in `settings.json`, so a restart needs no reconnect. Without the integration the mode still works: weather from `weather_entity` in the settings file, no media card, no tiles, a hint to add the integration. While the integration is missing the panel retries every minute, so installing it needs no panel restart.
- **Shown by Home Assistant** (integration actions): `nspanel_pro.show_view` (`home`, `dashboard`, `weather`, `media`, `camera` with `entity_id`; `seconds`, 0 = until touched) and `nspanel_pro.show_message` (`title`, `message`, `icon`, `seconds`, `chime`). A shown layer wakes the screen, keeps the screensaver away while it is up, and closes by itself.
- The voice assistant works in both modes; in assistant mode the conversation is full screen (unless switched off in the integration), in dashboard mode it is a panel at the bottom.
- The look is a first working version; the whole design will be redone.

## Connection, start and outages

- **First screen with current content**: the Home Assistant client starts before the app takes the display over from the boot splash. The app waits until the clock is set and the first full state list is in (plus 300 ms for the assistant settings and forecasts); at most 25 s, at once when pairing is needed. Meanwhile the boot splash shows its light band.
- **Clock**: the panel has no battery-backed clock and starts in 1970. Until NTP has set it, the app takes the time from the `Date` header of Home Assistant's WebSocket handshake (`app/src/clock.rs`), which also works without internet access. Until either has set it, the UI shows `--:--` and no date.
- **NTP** (`S45ntp`): busybox `ntpd` starts as soon as Wi-Fi has an address, however late that is, and is started again 30 s after it exits.
- **Retries, for good** (a power cut can keep Home Assistant or the network away for hours): without network every 0.5 s during the first minute after the start, then every 5 s; while Home Assistant does not answer 2, 4, 8, then every 10 s. A vanished server is noticed within about 30 s (ping after 20 s without traffic, 10 s for the answer). A failure is logged once per cause, the recovery with the length of the outage.
- **Status line**: "Waiting for the network…", "Home Assistant not reachable, retrying…", or Home Assistant's own error (login, dashboard).
- Assistant mode without data: the media card stays hidden until its first state arrives (it would show the raw entity id).
- **Wi-Fi roam check** (`app/src/roam.rs`): from 5 minutes after the start, about every 30 minutes, and only while the panel is idle (no voice run, no awaited or playing answer, no announcement, no ringing timer; otherwise again a minute later). At -49 dBm or better there is no scan. Otherwise the panel scans (`wpa_cli scan`, results after 8 s) and moves with `wpa_cli roam` to the strongest access point of the same network if it is at least 10 dB stronger than the current one as the same scan saw it. The thresholds are those of ESPHome's `wifi.roam`. The move is one-shot: the network is never tied to one access point, so a panel moved to another room keeps working. Each check writes one line (`roam check: …`), a move also the result 15 s later. Tested on the panel: moved by hand (`wpa_cli roam`) to an access point about 20 dB weaker, the first check moved it back; the re-association took about 1 s, and the Home Assistant connections stayed up through both moves.
- Tested on the panel without switching anything off in the house (an `unreachable` route to Home Assistant, all routed traffic blocked except the test computer, a wrong Wi-Fi password in the RAM copy of the Wi-Fi config): Home Assistant gone for 75 s → noticed in 30 s, back 9 s after it returned; start without Home Assistant and internet → UI after 25 s with `--:--`, NTP set the clock 8 s after internet came back, Home Assistant connected 6–9 s after it returned; Wi-Fi gone for 90 s → noticed in 15 s, back in about 15 s.

## First start

Without `app.json` the app shows the setup screen "Connect to Home Assistant": Home Assistant instances found over mDNS (`_home-assistant._tcp`, the first one is listed) and a keypad for the address (digits, `.`, `:`). Without a port it tries 8123, then 80. It checks that Home Assistant answers (`/auth/providers` needs no login and only exists there), writes `app.json` and restarts the app, which then shows the pairing QR code.

## Configuration

`/mnt/persist/nspanel/app.json` holds the connection settings; the login token is kept separately in `ha_auth.json`.

```json
{"ha_url": "http://192.168.1.10:8123"}
```

- `ha_url`: the address of Home Assistant, with the port if it uses one (Home Assistant OS installed since 2026.8 listens on port 80; older installations and containers on 8123). Use the IP address: the panel does not resolve `.local` (mDNS) names.

- `dashboard`: url_path of the dashboard to show; empty or missing = the default dashboard. It can also be chosen on the panel's settings screen or with the Dashboard select in Home Assistant.
- `http_port` (default 8080): port of the login callback server.

## Structure

| File | Role |
|---|---|
| `app/ui/main.slint` | UI: pairing screen, top bar, dashboard rows (heading, tile pair, shutter, climate, media, camera), full-screen camera |
| `app/ui/assistant.slint` | Assistant mode: home screen, full-screen conversation, weather, media and message layers |
| `app/ui/overlays.slint`, `rows.slint`, `widgets.slint`, `theme.slint` | Details layer, settings, screensaver, pairing, conversation panel, timer alarm; dashboard rows; small widgets; colours |
| `app/src/main.rs` | Start-up, UI state, Home Assistant events → rows, taps → service calls, camera visibility, screensaver, theme, show requests, hand-over from the boot splash |
| `app/src/assistant.rs` | Assistant mode contents: effective configuration, forecasts, media card, tile rows |
| `app/src/artwork.rs` | Media artwork fetch and downscale |
| `app/src/esphome.rs` | ESPHome API server: device entities, voice satellite messages |
| `app/src/api_link.rs` | ESPHome API transport: plaintext or Noise, the API key, allowed hosts |
| `app/src/voice.rs` | Microphone capture, echo cancellation, wake words (FFI to `app/native/`), audio player, conversation state machine |
| `app/src/hw.rs`, `settings.rs`, `config.rs` | Backlight, light/proximity sensor, Wi-Fi/SoC readings, volume; panel settings; `app.json` |
| `app/src/roam.rs` | Idle Wi-Fi roam check: scan, compare, one-shot move to a stronger access point of the same network |
| `app/src/ha.rs` | WebSocket client thread: auth, dashboard config, `subscribe_entities`, service calls, keep-alive, reconnect with back-off |
| `app/src/auth.rs` | Token storage and refresh (OAuth refresh token → access tokens) |
| `app/src/pairing.rs` | QR code, small HTTP server for the login redirect |
| `app/src/dashboard.rs` | Dashboard config → rows; entity state → tile text, icon, colour |
| `app/src/camera.rs` | Camera snapshot thread (JPEG decode) |
| `app/src/mdns.rs` | mDNS: announces the ESPHome API, finds Home Assistant for the setup screen |
| `app/src/clock.rs` | Wall clock: set from Home Assistant's HTTP `Date` header until NTP has run (no battery-backed clock) |
| `app/src/icons.rs` | Material Design Icons name → glyph; default icons |
| `app/src/display.rs` | DRM/KMS output: two dumb buffers (XRGB8888) with page flips; own `TargetPixel` type |
| `app/src/touch.rs` | Touch from evdev (single-touch emulation) |
| `app/src/platform.rs` | Slint platform: software renderer (`SwappedBuffers`), one `poll(2)` loop for page flips, touch, cross-thread wake-ups (eventfd) and timers |

## Build, run, develop

```bash
tools/build_image.sh       # builds host Rust + fontconfig, then the app (tools/build_app.sh), then the image with the app
tools/build_app.sh         # app only -> /root/nspanel/app-build/target/aarch64-unknown-linux-gnu/release/nspanel-app
```

- The image starts the app from `/etc/init.d/S90nspanel-app` (restarted when it exits, output to syslog).
- Development aids:
  - `NSPANEL_FIXTURE=<file>` renders a saved dashboard config + states without Home Assistant; the fixture may also carry `panel` (integration config), `forecast_hourly` and `forecast_daily`.
  - `NSPANEL_DEV_INPUT=/tmp/nspanel-input` creates a FIFO: lines `tap x y`, `down x y`, `move x y`, `up x y` act as touches, JSON lines are app commands (`{"show": {...}}` with the integration's fields, `{"assist": [state, heard, answer]}`).
  - `kill -USR1 <pid>` writes the screen to `/tmp/nspanel-screen.ppm`; `kill -USR2 <pid>` scrolls one page.

## Dependencies and gotchas

- Slint 1.18 with `renderer-software` and `software-renderer-systemfonts` (runtime TrueType rendering, so any text from Home Assistant renders, accents included). DejaVu Sans and the MDI font are embedded in the binary.
- `software-renderer-systemfonts` links **fontconfig**, and Slint's font backend (fontique 0.11) **panics if fontconfig finds no font at all**. The image therefore carries fontconfig and one system font (DejaVu Sans).
- Embedding pre-rendered glyphs (`EmbedForSoftwareRenderer`) was not chosen: it only embeds ASCII plus the characters of the UI's string literals, so dynamic text would miss glyphs.
- Other crates: drm, tungstenite (plain `ws://`), ureq 2 (plain HTTP), qrcode, jpeg-decoder, serde_json, minimp3. No TLS yet: an `https://` Home Assistant URL needs a TLS feature.
- The touchscreen shows as "Hynitron CST226SE Touchscreen" (kernel patch 0002 sets a per-chip input name; the mainline driver hard-codes "cst3xx").

## Licence note

Slint is triple-licensed: GPLv3, a royalty-free licence that **excludes embedded systems**, or a commercial licence. This firmware uses the GPLv3 option. The project licence is AGPL-3.0-or-later; GPLv3 section 13 explicitly permits combining GPLv3 code with AGPLv3 code into one work. Material Design Icons: Apache-2.0 (Pictogrammers Free License). DejaVu fonts: Bitstream Vera licence. minimp3: CC0 (decoder), MIT (crate).
