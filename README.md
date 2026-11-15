# PanelOS

Open-source Linux firmware for smart home wall panels: it replaces the closed vendor firmware with a minimal **mainline Linux** system built with **Buildroot**, running a native **Rust + Slint** Home Assistant dashboard and voice satellite. No browser, no cloud.

Version **0.0.3** (what is new: [CHANGELOG.md](CHANGELOG.md)). Licence: AGPL-3.0-or-later ([LICENSE](LICENSE)).

## Supported devices

| Device | SoC | State |
|---|---|---|
| Sonoff NSPanel Pro (86 model) | Rockchip PX30 | working: dashboard, voice satellite, boot logo and animation, about 12 s boot |

## Features

- **Assistant mode**, the heart of PanelOS: a smart display for Home Assistant, made for talking to Assist and for a glance at the house. Today: a home screen with clock, weather, media and quick tiles, a full-screen conversation view, weather details, the media player, camera views, messages and timers. The new interface, designed and being built page by page:
  - **Home**: widgets on living backgrounds that show the weather as it is outside right now (rain, snow, fog, wind, thunder, the sun and the moon where they really are), or calm drawn scenes;
  - **Music**: what is playing in several looks, picking and grouping players;
  - **Timers, alarms and a stopwatch**, with several timer themes;
  - **Thermostat** in several looks, with a glow that shows from afar when it heats or cools;
  - **Camera**: the whole picture or filled, all cameras in a grid, a ring overlay for a doorbell;
  - **To-do lists** and a **calendar** (day, three days, week, month);
  - **Info and alerts**, critical alerts included;
  - **Status screens** for what happens at home (washing machine, robot vacuum and more);
  - **Quick controls**: tiles, sliders, groups;
  - **Voice feedback**, full screen while listening and answering;
  - **Screensaver clocks** in many styles;
  - **Settings**: a quick shade pulled down from the top, child lock, storage.

  **Weather: [Bellweather](https://github.com/lauden-io/Bellweather)** was made for exactly this: the weather here and now from real observations (radar, nearby stations) and a forecast blended from many models. The living backgrounds read it, and it gives the best experience; any Home Assistant weather entity works too.
- **Native Home Assistant dashboard**: drawn by the panel itself from a Home Assistant dashboard (tiles, shutters, climate, media, camera), details layers, themes, screensaver, automatic brightness, wake on approach. Pairing with a QR code, no token typing.
- **Voice satellite**: on-device wake words (microWakeWord), Assist pipelines, echo cancellation, timers, announcements, follow-up listening.
- **Dashboard mode** (secondary): a Home Assistant dashboard in a web view, custom cards included; within the limits of the panel's hardware, so a light, simple dashboard made for it works best ([docs/home_assistant.md](docs/home_assistant.md#dashboard-mode)).
- **First-start setup on the panel**: Home Assistant found on the network (mDNS) or its address typed on a keypad; pairing with a QR code.
- **Home Assistant device**: the PanelOS integration (`custom_components/panelos`) adds the panel as a device with its display, settings and sensors, sets its dashboard and assistant modes and shows views or messages on it; the voice satellite is an ESPHome device (announced over mDNS like any ESPHome device), connected via the panel.
- **Super fast boot**: about 12 s from power on to the dashboard, more than four times faster than the stock firmware (about 53 s). The PanelOS logo already shows in the bootloader, then the boot animation; the UI comes with the right time and current Home Assistant data. After a power cut it waits for Home Assistant and the network as long as it takes and comes back by itself.

Details: [docs/app.md](docs/app.md).

## Future plans

- More devices: other NSPanel models (NSPanel Pro 120, NSPanel Pro Gen2), the Shelly Wall Display (MediaTek MT6580; mainline Linux support still to be checked) and Wall Display XL (Rockchip RK3566), and the ESP32-based NSPanel as a separate, smaller target (its screen is driven by its own controller over a serial link).
- A 3D-printable adapter plate for mounting the US NSPanel on a European 60 mm wall box (screw spacing 83.5 mm vs 60 mm).
- Ready-made images on GitHub Releases, so a user does not have to build. A release image must not carry a personal SSH key: the user's own key is added at install.
- Native builds on Macs with Apple silicon and other arm64 computers: Buildroot's Bootlin toolchain only runs on x86-64 hosts, so an arm64 host needs Buildroot's own toolchain (built from source). Until then the build runs in an emulated x86-64 virtual machine (see [docs/install.md](docs/install.md#mac)).
- Signed over-the-air updates from Home Assistant's Settings → Updates, written in the background into the second system slot (A/B: the recovery partition and, once the stock Android is removed, the boot partition). The panel stays usable while it updates and is out only for the restart; a failed update restarts into the previous version by itself and says so.
- A wiki once more devices are supported.

## Why (NSPanel Pro)

- The stock software is slow: a heavy React Native launcher, constant cloud sync and an old Chromium 107 WebView.
- The panel runs Android 8.1 on every Sonoff firmware version (3.x and 4.x alike), so the system WebView can never go past Chromium 138.
- Enabling ADB/root permanently blocks official firmware updates.
- The hardware itself is fine for a wall panel: 4x Cortex-A35, Mali-G31, 2 GB RAM, 480x480 MIPI-DSI screen, capacitive touch, light and proximity sensors, microphones and a speaker.

## Install (NSPanel Pro 86)

Step-by-step guide, no Linux knowledge needed: **[docs/install.md](docs/install.md)**.

In short: turn on developer mode and ADB in the eWeLink app, back up the panel, build the image on a Linux or WSL computer (it takes the stock bootloader's device tree from the backup), write PanelOS to its recovery partition over Wi-Fi ADB, make it the default boot, type the address of Home Assistant on the panel's setup screen (or pick it when found), and log in with the QR code on the screen.

## Known limitations

- No TLS yet: Home Assistant must be reachable over plain `http://`.
- mDNS (finding Home Assistant on the setup screen, Home Assistant discovering the panel) only works inside one network segment, unless the router repeats mDNS between them.
- Web mode (the real Home Assistant frontend) is not started.
- The look is a first version; a full redesign is planned.

## Repository layout

- `app/` the panel app (Rust + Slint), see [docs/app.md](docs/app.md)
- `custom_components/panelos/` Home Assistant integration (the panel's device, dashboard and assistant mode settings, screen actions), installable with the Home Assistant Marketplace or HACS, see [docs/home_assistant.md](docs/home_assistant.md)
- `docs/` hardware notes ([docs/hardware.md](docs/hardware.md)), backup and restore, bring-up notes ([docs/bringup.md](docs/bringup.md)), security and development aids ([docs/security.md](docs/security.md)), design decisions and plans, feature list
- `kernel/` board device tree (`dts/`), mainline kernel patches (`patches/`), config fragment (`configs/`)
- `buildroot/` Buildroot external tree (`BR2_EXTERNAL`): root filesystem defconfig, init scripts, boot splash, small tools
- `assets/` logo and logotype (SVG), generated boot pictures
- `testinit/` minimal PID 1 used for the first bring-up test boots
- `tools/` build scripts, backup and restore, flashing over ADB or SSH, resource image pack/unpack, DTB dump, pstore log, boot splash renderer (`tools/splash/`), wake word benchmark
- `backup/` local firmware backups, git-ignored, **never published**

## Warning

Flashing can brick the device. Make a full backup first (see [docs/restore.md](docs/restore.md)) and prove that you can restore it before you write anything. The partitions contain device-unique data (MAC addresses, keys, serial numbers): never share your backup images.

## License

AGPL-3.0-or-later (full text in LICENSE). The panel app uses the Slint toolkit under its GPLv3 option, which GPLv3 section 13 allows to be combined with AGPL-3.0 code. Third-party parts keep their own licences (Linux kernel GPL-2.0, Buildroot packages, DejaVu fonts, Material Design Icons, the microWakeWord models).
