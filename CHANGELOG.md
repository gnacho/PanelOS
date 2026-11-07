# Changelog

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
