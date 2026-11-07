# Panel app (`app/`)

Native Rust app with the Slint UI toolkit. It draws a Home Assistant dashboard straight to the display and reads the touchscreen itself: no browser, no compositor, no GPU stack.

## Dashboard

- **Pairing by QR code** (no token typing): on first start the panel shows a QR code with Home Assistant's normal login page (OAuth authorization-code flow, the panel is the IndieAuth client: `client_id` `http://<panel-ip>:8080/`, `redirect_uri` `http://<panel-ip>:8080/auth/callback`; Home Assistant accepts local IP client IDs). After the login on the phone, Home Assistant redirects the phone to the panel, which exchanges the one-time code for a refresh token. The token is stored on the panel only (`/mnt/persist/nspanel/ha_auth.json`, root-only); it can be revoked in the Home Assistant user profile, after which the panel shows the QR code again.
- **Rendering**: the app loads a Home Assistant dashboard config (`lovelace/config`) and renders it natively; only the content comes from Home Assistant, the look is the app's own. Supported: `heading`, `tile`/`entity`/`button`/`light`/`sensor`, `entities`, stacks/grids (flattened), `thermostat`, `media-control`, `picture-entity`/`picture-glance` (camera), and `weather`, `glance`, `gauge`, `weather-forecast` as tiles. Other card types are skipped. A dashboard with several views gets tabs under the top bar. Dashboard edits appear at once (reload on `lovelace_updated`).
- **Live states**: `subscribe_entities` for exactly the dashboard's entities (compressed updates), so the panel does not parse every state change in the house.
- **Control**: tap toggles lights/switches/TVs; shutters have open/stop/close; climate has −/+ and tap on the icon switches off/heat; media has previous/play-pause/next and volume −/+.
- **Details layer** ("more info"): tap a tile (the icon still toggles). Lights: on/off, brightness, colour temperature (kelvin, from the light's range), colour (hue); shutters: position slider + open/stop/close; climate: target ±, hvac mode, fan, preset and swing chips (whatever the entity offers); media: play controls, volume slider, source chips.
- **Camera**: snapshots through Home Assistant's camera proxy, resized by Home Assistant (480×270 in about 0.14 s from a UniFi Protect medium channel). About 1 frame/s while the camera row is on screen, as fast as possible in full screen (tap the picture), nothing while it is scrolled away.
- **Theme**: dark, light, or auto (sun below the horizon from `sun.sun`, otherwise a dark room by the light sensor).
- **Settings** (gear in the top bar): theme, automatic or manual brightness, screensaver timeout and style (clock / screen off), wake on approach, display mode, dashboard choice (from Home Assistant's dashboard list), volume, about (version, IP, uptime, memory, SoC temperature, light level), restart app, reboot, log out. Saved in `/mnt/persist/nspanel/settings.json`.
- **Screensaver**: big clock, date and weather (`weather_entity` in the settings file), or screen off; wakes on tap (on release, so the tap does not reach the dashboard) or on approach.
- **Automatic brightness**: STK3x3x ambient light, logarithmic curve (minimum in the dark, maximum from about 500 lx), smoothed; the sensor's scale is switched automatically because it saturates at about 6500 lx on its most sensitive scale.
- **Approach wake**: learns the resting proximity level, wakes after two readings clearly above it.
- Icons: Material Design Icons 7.4.47 (the set Home Assistant uses), `mdi:` names from the entity or Home Assistant-like defaults per domain.

## ESPHome native API

- The app is an ESPHome API server on TCP 6053 (plaintext for now; Noise encryption and mDNS discovery still to do). Message types are generated from `app/proto/api.proto` (aioesphomeapi, MIT). Home Assistant's built-in ESPHome integration adds the panel by IP.
- Entities: Screen (switch: off = screensaver now), Dashboard (select, options = the Home Assistant dashboards), Theme, Screensaver after, Screensaver style, Display mode (selects), Brightness, Volume (numbers), Automatic brightness, Wake on approach, Mute microphone (switches), Illuminance, SoC temperature, Wi-Fi signal, Wake level, Wake SNR, Wake score (sensors; the wake values are sent with every wake word before the voice run starts, so Home Assistant can let the satellite that heard best answer: level = RMS of the last second in dBFS, SNR = loudest 100 ms of the last second over the second before it in dB, score = the model's averaged probability), Presence (binary sensor from the proximity sensor), IP address, App version (text sensors), Restart app, Reboot (buttons), plus the voice satellite entities Home Assistant creates itself.
- `src/esphome.rs`: framing, entity list, state diffing (changes pushed when the UI thread notifies), commands mapped to the same setting keys as the on-panel settings screen.
- Start-up: the server accepts Home Assistant's connection only after the dashboard list is known (at most 20 s), so the Dashboard select is complete at once.
- rtw88 does not fill `/proc/net/wireless`; the Wi-Fi signal comes from `iw dev wlan0 link`.

## Voice satellite

- The panel is an Assist satellite through the same ESPHome API connection (`VOICE_FEATURES` = voice assistant, speaker, API audio, timers, announcements; Home Assistant only offers the wake word selects when the announce flag is set). Home Assistant shows it as an assist satellite with Assistant / Assistant 2 and Wake word / Wake word 2 selects, like ESPHome voice satellites.
- **Wake words** run on the panel: microWakeWord models downloaded by `tools/build_image.sh` into `/usr/share/nspanel/wakewords`: `okay_nabu` (OHF-Voice release 20241226.3, cutoff 0.85), `hey_jarvis`, `hey_mycroft`, `alexa`, and the experimental `hey_home_assistant`, `okay_computer`, `choo_choo_homie`, `hey_peppa_pig`. The internal `stop` model is not offered in Home Assistant: it listens only while the panel speaks or a timer rings (and 2 s after), and stops it. Home Assistant's Wake word selects list what the panel reports; any microWakeWord model (`<id>.tflite` + `<id>.json`) in that folder appears there. Only enabled models run: the idle app takes 6.9 % of one core with two wake words. TensorFlow Lite (Buildroot package) and the TFLite micro frontend are compiled into the app (`app/native/`).
- **Wake word reset**: the streaming models keep their recent input inside the model (resource variables), so after a conversation the wake word that started it would fire again as soon as listening resumes. `mww_reset` feeds 3 s of silence features through every model.
- **Echo cancellation**: WebRTC AudioProcessing 1.3 (AEC3 + high-pass filter, Buildroot package `webrtc-audio-processing`, `app/native/aec.cc`), 16 kHz mono, 10 ms frames. The reference is the player's own output (written to ALSA in 10 ms pieces, 48 kHz → 16 kHz, ALSA buffer 100 ms). Measured: microphone level 15–28 dB lower during a spoken answer; the panel's own voice saying a wake word does not wake it. It runs only while the speaker plays and 2 s after (continuously it would cost about a quarter of a core). With it the wake word also works while the panel speaks: it cuts the answer off and starts a new conversation. Noise suppression is off (the wake word frontend has its own; Home Assistant's STT gets the echo-free voice).
- **Audio**: in from the PDM microphones (`hw:CARD=pdmmics`), 4 channels at 16 kHz, the mono signal is the mean of channels 2 and 3 with a fixed gain; out to `hw:CARD=rk809` at 48 kHz (chimes, TTS, timer beeps; 16 kHz, WAV and MP3 sources are resampled; MP3 decoded with `minimp3`).
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
| `app/src/voice.rs` | Microphone capture, echo cancellation, wake words (FFI to `app/native/`), audio player, conversation state machine |
| `app/src/hw.rs`, `settings.rs`, `config.rs` | Backlight, light/proximity sensor, Wi-Fi/SoC readings, volume; panel settings; `app.json` |
| `app/src/ha.rs` | WebSocket client thread: auth, dashboard config, `subscribe_entities`, service calls, keep-alive, reconnect with back-off |
| `app/src/auth.rs` | Token storage and refresh (OAuth refresh token → access tokens) |
| `app/src/pairing.rs` | QR code, small HTTP server for the login redirect |
| `app/src/dashboard.rs` | Dashboard config → rows; entity state → tile text, icon, colour |
| `app/src/camera.rs` | Camera snapshot thread (JPEG decode) |
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
