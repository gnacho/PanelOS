# Design decisions

| Decision | Reason |
|---|---|
| Replace the vendor firmware completely | Removes the software bloat; on the NSPanel Pro, Android 8.1 caps the WebView at Chromium 138 on every Sonoff firmware |
| Mainline Linux kernel from the start | Long-term maintainability, open GPU driver (Panfrost), no vendor kernel lock-in |
| Buildroot root filesystem | Small, fast boot, reproducible |
| Native Rust app with the Slint UI toolkit | No browser and no WebView; renders directly on DRM/KMS; small and fast on a Cortex-A35 |
| Home Assistant WebSocket API for the dashboard, ESPHome native API for the device and the voice satellite | Works with Home Assistant's built-in integrations; no broker and no custom integration needed for the basics |
| Zigbee radio optional, router role at most | Not needed for the panel's main job |
| Features inspired by an open-source Flutter panel app, no code reused | Feature ideas only |
| Project licence AGPL-3.0-or-later | Slint is used under GPLv3 (its royalty-free licence excludes embedded systems); GPLv3 §13 permits combining it with AGPL-3.0 code |

## Kernel line

The current base is the latest stable kernel (7.3.x). There is no 7.x longterm kernel yet; by long-standing practice the last mainline release of a year becomes the next longterm kernel, so the 2026 one is expected to be 7.4 (a convention, not a promise). Plan: track the latest stable and move to the 2026 longterm kernel once it is announced (https://www.kernel.org/releases.json).

## Updates (planned)

- One signed bundle per release (kernel, DTB, root filesystem, panel app), published as a GitHub release. The panel verifies the signature before installing.
- A/B with test boot and automatic fallback: the new system goes to the spare slot and boots once from there (the stock U-Boot's one-shot recovery boot). It becomes the default only after a health check (Wi-Fi, app running, Home Assistant connected); otherwise the next boot returns to the previous version.
- Home Assistant: an `update` entity (installed version, latest version, release notes, install command, progress), so the update shows up under Settings → Updates like any other; auto-update is optional.
- On the panel: a settings page with the version, "check for updates", "install" and an auto-update switch.
- First install over Wi-Fi ADB from the stock firmware (no opening the case), every later update over the air.
- The panel can update itself without Home Assistant too: it checks GitHub Releases directly (or a local file/URL).

## Home Assistant connection

- **Dashboard**: Home Assistant's WebSocket API.
- **Device and voice**: the panel speaks the ESPHome native API, so Home Assistant's built-in ESPHome integration adds it like any ESPHome device: settings and sensors as entities, plus the Assist satellite over the same connection. Several panels are handled like any ESPHome device; automations can switch modes.
- **No token typing on the panel.** Pairing uses Home Assistant's standard OAuth (IndieAuth) login: the panel shows a QR code, the user scans it with a phone and logs in to Home Assistant as usual, Home Assistant redirects to the panel (client_id and redirect_uri on the panel's own address), and the panel exchanges the code for a refresh token. The token is visible and revocable in the Home Assistant user profile.
- The QR login shows Home Assistant's normal login form, so any user can log in, for example a dedicated non-admin "panel" user, independent of the account the phone's Home Assistant app uses (a private browser window avoids any confusion). An on-panel login with a touch keyboard is planned as an alternative.
- **Dashboard choice:** the panel lists the Home Assistant dashboards; the user picks one on the panel's settings screen or with the Dashboard select in Home Assistant.
- **Kiosk by design:** the app draws the dashboard natively, not the Home Assistant web frontend, so there is no Home Assistant header or sidebar at all. Trade-off: only the card types the app implements render; custom frontend cards (HACS) do not.
- Still to do: Noise encryption and mDNS discovery for the ESPHome API.

## Display modes

| Mode | What it is | Engine |
|---|---|---|
| Native dashboard (default) | Predefined card types; content from a Home Assistant dashboard chosen for the panel; own look, themes, more-info dialogs | Rust/Slint app |
| Assistant | Smart-display screens: clock, weather, now playing, timers, what Assist heard and answered, a few quick tiles; changes with the conversation | Same app; configured with the own Home Assistant integration (`custom_components/nspanel_pro`). No View Assist dependency: View Assist needs a web browser on the display |
| Web (planned) | The real Home Assistant frontend as a kiosk, custom (HACS) cards included | WPE WebKit + cog on DRM, GPU through Panfrost; its speed on the PX30 has to be measured first |

- The **voice satellite** works in every mode: PDM microphones → on-device wake word → Home Assistant Assist pipeline, echo cancellation with the player's own output as reference, TTS and media through the speaker.
- **Configuration in three layers:**
  1. On the panel (settings screen): what must work without Home Assistant – Wi-Fi, pairing, mode, brightness, theme, screensaver, about.
  2. In Home Assistant as ESPHome device entities: mode, dashboard, theme, brightness, screensaver, wake words, volume, microphone mute, sensors.
  3. Content: Home Assistant dashboards; the assistant mode through the own integration.
- The look is a placeholder until a full redesign.

## Future plans

See the [README](../README.md#future-plans).

## Stock firmware redistribution

The stock firmware backup is not published: it is Sonoff's copyrighted software, and several partitions carry device-unique identity (MAC addresses, keys, serial numbers, licence flags) that must never be copied to another unit. Instead the project ships the backup tool so every user backs up their own panel, and the installer has to support the firmware versions people actually run. Downgrading 4.x to 3.0.0 is not a planned path.

## Install compatibility

- Required: Wi-Fi ADB **with root** (`su`), as on the stock 3.0.0 userdebug build. Some community reports say newer stock firmware removed basic tools (`dd`, `chmod`, ...); the installer should bring its own static tools.
- Only the 86 model (480x480, ST7701S) so far. The 120 model and the second generation are different hardware.
- Known to work: a unit rooted on an older stock firmware and then updated to 3.0.0 (the stock launcher blocks OTA updates on rooted units, so it stays there).
- Verified on stock firmware 3.0.0 only. The stock U-Boot on 4.x firmware may load kernels differently; the installer must check prerequisites and refuse to write anything if they are not met.

## Open questions

- Keep the vendor U-Boot or move to mainline U-Boot for PX30.
- Bootloader logo: which vendor DT node makes the stock U-Boot hang (needs the serial console).
- Whether the two microphone channels are two capsules or one capsule read on both clock edges.
- Identify the device at i2c-2 0x3c and the Zigbee UART.
