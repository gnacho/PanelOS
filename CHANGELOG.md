# Changelog

## 0.0.3

### Security

- **Encrypted ESPHome API**: Home Assistant gives the panel a key right after adding it (as with ESPHome devices) and connects encrypted from then on; plaintext and other keys are refused. Only Home Assistant's address may connect (`api_allow` in `app.json` adds more).
- The API checks frame sizes, keeps at most three connections, and a malformed message ends only its own connection. A voice run streams the microphones for at most 30 s; conversation events only count while the panel's own run is going on.
- Audio is fetched only from Home Assistant's host, without redirects, with limits on size, format and length; one announcement at a time. The login token goes only to Home Assistant itself; logging out revokes it.
- mDNS answers only the panel's own network; the login web server stops after pairing; camera images are checked for size; only installed wake word models can be activated.
- SSH: root logs in with the key only (password locked); the host key is kept on the persistent partition, and the guides check it instead of ignoring it.
- Kernel and network settings: kernel addresses hidden from unprivileged users, no ICMP redirects or source routing.
- Build: kernel and Buildroot at fixed commits, wake word models checked against SHA-256 sums.
- Integration: `show_view` and `show_message` for administrators and automations only, message lengths limited.
- `docs/security.md`: what is protected, how, and the development aids that stay on for now.

### Voice

- **Long answers without gaps**: a streamed answer starts once 0.8 s of audio is in (at most 1.5 s), so a wait of up to about 0.8 s in Home Assistant's stream no longer cuts the speech; the answer starts up to 0.8 s later.
- The conversation stays on the screen until the answer has been spoken (Home Assistant ends the run when the speech starts).

### First start and discovery

- **Setup screen on the panel**: Home Assistant found on the network (mDNS) or its address typed on a keypad; the panel checks that Home Assistant answers there, saves it and shows the pairing QR code. No SSH needed for the first setup.
- **mDNS announcement** of the ESPHome API (`_esphomelib._tcp`), like any ESPHome device: Home Assistant discovers the panel and keeps its address up to date (inside one network segment). Built into the app, no system service.

### Power cuts and outages

- The panel keeps trying for good when Home Assistant or the network is away, and comes back by itself without a restart: retries every 0.5 s right after the start, then every 5 s without network and every 10 s at most while Home Assistant does not answer.
- A vanished Home Assistant is noticed within about 30 s; failures are logged once per cause instead of on every try.
- Clear status: "Waiting for the network…" or "Home Assistant not reachable, retrying…".
- No 1970 time on the screen: `--:--` until NTP or Home Assistant has set the clock.
- NTP starts whenever Wi-Fi gets an address, however late, and restarts if it exits.
- Assistant mode: the media card stays hidden until its first state arrives.

### Wi-Fi

- **Back to the nearer access point**: wpa_supplicant changes the access point only when the link breaks, so after an access point outage the panel stayed on a farther one for hours. Now, about every 30 minutes and only while the panel is idle (no voice run, no sound, no ringing timer), it scans and moves to an access point of the same network that is at least 10 dB stronger. There is no scan at -49 dBm or better (the thresholds of ESPHome's `wifi.roam`). The move is one-shot: the panel is never tied to one access point. One log line per check.
- wpa_supplicant logs connections, disconnections and access point changes to the system log (syslog support turned on in Buildroot, `-s`). `tools/build_image.sh` rebuilds wpa_supplicant once in an older Buildroot output directory.

### Display

- **Light and dark theme by the light sensor** from the first start, the setup screen included: dark below 10 lx, light above 30 lx, a change only after 10 s, so it does not flicker at dusk.
- **Brightness scale** even in perceived brightness, from 0.8 % (night) to 100 %, for automatic and manual brightness; in the evening (about 50 lx) the panel is now noticeably dimmer than before.
- **Smooth brightness changes**: fades every 20 ms instead of steps twice a second; the backlight has 1000 levels instead of 256.
- **Wake on approach works**, from about 40–45 cm: the proximity sensor runs at its most sensitive gain, 11.84 ms integration time and the IR LED at full power (at the driver's defaults it read 0). The threshold is the signal of a hand at 45 cm from a distance curve measured with a ruler, or 5 times the measured noise if that is more; a slow approach and a hand held just below the threshold do not raise it. The Presence entity uses the same detection.

### Home Assistant

- **Fewer database writes**: illuminance, SoC temperature and Wi-Fi signal are sent only after a set change instead of every minute's reading: light by 15 % of the last value sent (at least 5 lx), temperature by 1 °C and signal by 3 dB on five-minute averages. Each step can be set finer or coarser in the integration's options (**Sensor reporting**), with an explanation of what it means.
- The integration's options open with a choice between **Assistant mode** and **Sensor reporting**.

### Boot

- **Packed kernel**: the boot image is about 30.5 MB instead of 41 MB and fits the 32 MiB `boot` partition, the base for A/B updates. The chain loader unpacks it with caches on in about 1 s; the boot time stays the same.

## 0.0.2

### Boot

- **Super fast boot**: about 12 s from power on to the dashboard (stock firmware: about 53 s). The stock bootloader phase dropped from about 45 s to about 2.5 s.
- **PanelOS logo in the bootloader**: the stock U-Boot shows `assets/bootlogo.bmp` (the first frame of the boot animation) at the brightness of the animation.
- **Chain loader** in front of the kernel (`tools/chainload/`, packed by `tools/arm64_chain_pack.py`): the stock U-Boot gets the stock device tree, the kernel its own one, with the memory layout, command line, initramfs location and serial number carried over.
- **Watchdog**: the chain loader starts the SoC watchdog and the kernel feeds it; a boot that hangs early resets the panel by itself.
- The backlight starts at 180 (bootloader, kernel and boot animation alike), no jump in brightness.

### Panel app

- The UI takes over the display only when it has current content: the clock is set and the first Home Assistant states are in. The boot animation's light band shows the wait.
- The clock comes from Home Assistant until NTP has set it, so it is right on the first screen, also on networks without internet access.
- While the network is not up yet, the Home Assistant connection is retried every 0.5 s instead of with a growing delay.
- Voice satellite: new sensors Wake level, Wake SNR and Wake score, sent with every wake word before the voice run, so Home Assistant can let the satellite that heard best answer.

### Install and build

- The build now takes the stock device tree from a panel backup (`resource` partition, `tools/make_uboot_dtb.sh`); the install guide backs up the panel before the build (steps 5–7 reordered).

### Documentation

- `docs/hardware.md`: the Zigbee module (EFR32MG21, EmberZNet 6.10.1 NCP, EZSP 8, Gecko Bootloader 1.12 with serial XMODEM upload) and the SoC console (UART2 TX on GPIO1_D2 and GPIO2_B4, not broken out to any pad).
- `docs/bringup.md`: chain loader, watchdog and boot times; `docs/decisions.md`: the stock-device-tree decision.

## 0.0.1

First version.

### System

- Mainline Linux kernel (7.3) trimmed to the board (17.8 MB Image) and a Buildroot root filesystem as initramfs, in the panel's recovery partition; the stock firmware and U-Boot stay untouched.
- Kernel patches: a panel descriptor for the Sitronix ST7701S display and CST226SE support in the Hynitron touch driver. Wi-Fi with rtw88 (RTL8723DS), audio through the RK809 codec, the PDM microphones, light and proximity sensor (STK3310).
- Default boot into PanelOS through the stock U-Boot's bootloader message (`nspanel-bootmode open | android | status`); one-shot test boots that fall back to the stock firmware without network (`S41testboot`).
- Settings, Wi-Fi configuration and the Home Assistant login on a persistent partition; Wi-Fi recovery by rebinding the SDIO controller.
- Boot animation from the first second of user space (`tools/splash/`, `nspanel-splash`), with a light band until Wi-Fi is up; the UI comes as soon as Wi-Fi is up.

### Panel app (Rust + Slint, drawn straight to DRM/KMS)

- Native Home Assistant dashboard from a dashboard chosen for the panel: tiles, entities, headings, thermostat, media control, camera, weather, glance and gauge cards; tabs for several views; live states for exactly the dashboard's entities; dashboard edits appear at once.
- Control by tap; details layers for lights (brightness, colour temperature, colour), shutters, climate and media.
- Camera snapshots through Home Assistant, about 1 frame/s, full screen on tap.
- Pairing by QR code (Home Assistant's own login, OAuth), no token typing.
- Themes (dark, light, auto), settings screen, screensaver (clock or screen off), automatic brightness from the light sensor, wake on approach.
- Assistant mode: smart-display home screen with clock, weather and forecast, media card and quick tiles; weather, media, conversation, message and camera layers.

### Voice satellite (ESPHome native API)

- On-device wake words (microWakeWord: Okay Nabu, Hey Jarvis, Hey Mycroft, Alexa and experimental ones), selectable from Home Assistant; an internal "stop" model while the panel speaks or a timer rings.
- Assist pipelines, echo cancellation (WebRTC AEC3) while the speaker plays, so the wake word works during an answer too.
- Follow-up listening; an empty follow-up ends after 5 s.
- Timers with a full-screen alarm (stopped by "stop" or a short sentence without the wake word), announcements, volume and microphone mute.

### Home Assistant

- The panel is an ESPHome device for Home Assistant's built-in integration: settings and sensors as entities (screen, dashboard, theme, brightness, volume, illuminance, SoC temperature, Wi-Fi signal, presence and more).
- Optional integration `custom_components/nspanel_pro` (Home Assistant Marketplace, HACS or manual): assistant mode settings, `show_view` and `show_message` actions.

### Install and tools

- Install over Wi-Fi ADB from the rooted stock firmware, no opening the case (`tools/flash_recovery_adb.sh`); updates from a running PanelOS over SSH (`tools/flash_recovery_ssh.sh`).
- Backup of all stock partitions (`tools/backup_partitions.sh`) and restore paths, including the USB rescue mode (`docs/restore.md`).
- Step-by-step install guide for Windows (WSL), Linux and Mac (`docs/install.md`).
