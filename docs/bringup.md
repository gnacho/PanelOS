# Bring-up notes (Sonoff NSPanel Pro 86)

How the mainline system was brought up on the panel, with the facts and pitfalls found on the way.

## Stock boot images

| Partition | Format | Content |
|---|---|---|
| `kernel` | `KRNL` wrapper | arm64 kernel Image (19.7 MB) |
| `boot` | Android boot image v0, page size 2048 | kernel + 1.4 MB ramdisk + "second" (1 MB) |
| `recovery` | Android boot image v0, page size 2048 | kernel + 9.3 MB recovery ramdisk + "second" |
| `resource` | `RSCE` | `rk-kernel.dtb` + boot logos |

The "second" stage of the boot images is about the size of the resource image; the vendor U-Boot takes the device tree from there. Kernel command line in the image header: `buildvariant=userdebug`; the rest comes from the DT `chosen/bootargs` and U-Boot.

## How the stock U-Boot starts an Android-image kernel

Stock U-Boot: Rockchip `next-dev` 2017.09, built 2024-03-06. Default environment: `kernel_addr_r=0x02080000`, `fdt_addr_r=0x01f00000`, `ramdisk_addr_r=0x0a200000`; `CONFIG_SYS_BOOTM_LEN` is 64 MiB for PX30. It does not check the boot image `id` hash.

An uncompressed kernel runs **in place** at `0x02080000`, and nothing aligns it first. That suits the vendor 4.4 kernel (`text_offset` 0x80000 on a 2 MiB base), but a mainline kernel has `text_offset` 0 and needs a 2 MiB-aligned start. `tools/arm64_branch_pad.py` therefore puts the Image at offset 0x180000 (landing on `0x02200000`) behind a single branch instruction, and pads the blob to the kernel's full `image_size`, so a ramdisk placed after it cannot overlap the running kernel.

## The recovery slot and the default boot

The system lives in the **recovery** partition; `boot`, `kernel` and U-Boot stay untouched.

- **Install**: write the image to `recovery` from the running stock Android over Wi-Fi ADB (`tools/flash_recovery_adb.sh`: `adb push`, then `su -c 'cat img > /dev/block/by-name/recovery'` and an md5 check; the partition is not mounted while Android runs). No cable and no opened case needed, for anyone with ADB enabled through the eWeLink developer mode. The USB route (`rkdeveloptool wl` at sector 204800, BOOT button) is the rescue path (`docs/restore.md`). From a running PanelOS system: `tools/flash_recovery_ssh.sh`.
- **One-shot boot**: `adb reboot recovery` from Android, or `reboot-to recovery` from PanelOS. The DT uses `syscon-reboot-mode` (PMU GRF offset 0x200, recovery = `0x5242c303`).
- **Default boot**: the stock U-Boot reads an Android bootloader message (BCB) at **16 KiB into the `misc` partition** (Rockchip `boot_mode.c`, `BCB_MESSAGE_BLK_OFFSET`). While its command is `boot-recovery`, every start, cold or warm, boots the recovery partition, and nothing clears it (only the stock recovery would). `nspanel-bootmode open` writes that command, `nspanel-bootmode android` clears it, `status` shows it.
- The stock Android restores its own recovery at every boot (`flash_recovery` service: `/system/bin/install-recovery.sh` applying `/system/recovery-from-boot.p`). Without the boot command an image in `recovery` therefore only survives until the next Android boot: write it and boot it in one go.
- `boot` is only 32 MB, so a full image (about 41 MB) cannot replace the stock boot image yet. Planned: a smaller image in `boot` as the stable slot, `recovery` as the one-shot test slot (A/B, see `docs/decisions.md`).
- The stock Android partitions (system, vendor, userdata, …) stay untouched; a full backup (`tools/backup_partitions.sh`) is the way back to stock.
- **Test-boot safety net** (`S41testboot`): when the default boot is not the open system (a one-shot test boot) and there is no network within 90 s, the panel reboots into the stock Android, so the previous image can be restored over ADB without anyone at the panel. A hang in U-Boot itself is not covered: that needs a power cycle.

## Logs without a serial cable

The vendor kernel keeps a `ramoops` area at physical `0x110000`, size `0xf0000` (record 128 KiB, console 512 KiB, pmsg 320 KiB, no ECC). The mainline DT reserves the same region with the same layout. After a **warm** reset (power cycling clears RAM) the stock Android exposes the previous console log in `/sys/fs/pstore`, readable over ADB with `su` (`tools/read_pstore_adb.sh`).

A serial console (UART2, 1500000 baud, pads on the board) remains the fallback when this is not enough.

## Power

A laptop USB port does not carry a full boot; use a 2 A charger.

## Display

- Both VOPs and the DSI bind, `card0-DSI-1` connects at 480×480; the ST7701S descriptor (`kernel/patches/0001`) works as is.
- Kernel text is not shown on the display (`console=tty0` not set, `vt.global_cursor_default=0`, `CONFIG_LOGO` off); kernel messages go to the serial console and pstore.

## Touch (CST226SE)

- The controller answers at I2C1 0x5a. The mainline `hynitron_cstxxx` driver fails: `cst3xx_bootloader_enter unable to enter bootloader mode`.
- The stock driver strings (`hyn_ts`, `cst2xx_ts_init`, firmware update command `0xD110`) place the CST226SE in Hynitron's CST2xx family, but it speaks the CST3xx commands and frames: chip ID via `D2 04` = 0xA8, 480×480, firmware 0x03000001. The CST3xx bootloader step (`A0 01 AA`) leaves it stuck in bootloader mode until a reset.
- `kernel/patches/0002` adds `hynitron,cst226se` without the bootloader step (reset delay 100 ms); the DT sets `touchscreen-swapped-x-y`. Five-finger multitouch, X 3..472, Y 5..480.
- The touch IRQ also fires about every 1.5 s without a touch (controller heartbeat), so the IRQ count alone does not prove touches.

## Restart through PSCI

With `system-power-controller` on the RK809 node, mainline `rk8xx-core` registers a PMIC restart handler (`DEV_RST`, priority 192) above PSCI (129). The PMIC reset clears the PMU GRF register at 0xff010200, so U-Boot sees a normal boot and `reboot-to recovery` lands in Android. The DT therefore has no `system-power-controller`; restart goes through PSCI and keeps the boot-mode flag.

## Wi-Fi (RTL8723DS, rtw88 over SDIO)

- Since Linux 7.x `pwrseq_simple` turns a single `reset-gpios` entry into a reset control served by the generic GPIO reset controller; it must be built in (`CONFIG_RESET_GPIO=y`). `cfg80211` needs the Buildroot `wireless-regdb` package.
- **Warm restarts**: WL_REG_ON (GPIO0_A2) is in the PMU domain and keeps its level across a PSCI restart, so the module stayed powered with its old firmware and the probe failed (`failed to dump efuse logical map`, -22). WL_REG_ON is now a fixed regulator (`vcc_wl`, `vmmc-supply` of the SDIO host) with 200 ms minimum off time and 100 ms start-up delay.
- **Self-heal** (`S40wifi`): if `wlan0` is not there after 8 s, the SDIO host (`ff380000.mmc`) is unbound and bound again, which switches `vcc_wl` off and on and probes the card again (up to 2 attempts; `wlan0` is back 2 s after the bind). `stop` waits for the old processes, so a restart never runs two `wpa_supplicant`s.
- **DHCP**: busybox `udhcpc` with default settings gave up after 3 tries and paused 20 s; with `-t 30 -T 1 -A 2` the address comes 1.7 s after association.
- `S40wifi` runs in the background, so the rest of the system does not wait for the network. Wi-Fi never reboots the panel; it keeps retrying.
- Settings are persistent (`S10persist` mounts the stock Android's `cache` partition, ext4, at `/mnt/persist`; files in `/mnt/persist/nspanel/`). The Wi-Fi network is imported once from the stock Android config (root-only, 600).

## Light and proximity sensor (STK3x3x)

Chip ID 0x5F is not in the mainline list; the `stk3310` driver warns "new unknown chip id" and works. Readings through IIO: room light about 545 raw (scale 0.1), covered 8, a flashlight saturates at 65535; proximity about 30 at rest, 140–235 with a finger about 5 cm away, 65535 when covered.

## Audio (RK809)

- **Stock DT wiring**: RK809 codec (`mic-in-differential`, `adc-for-loopback`, `pdmdata-out-enable`), simple card with "Mic Jack" and "Headphone Jack". Playback on I2S1; capture through the SoC PDM controller.
- **Mainline**: `rk817` codec driver (it covers the RK809 MFD cell), I2S1 with SDI, `simple-audio-card`, MCLK from `SCLK_I2S1_OUT`. Card `rk809` with Master Playback/Capture Volume, Mic Capture Gain, Playback Mux (HP/SPK).
- The built-in speaker is on the codec's class-D output (**SPK**); HP is not connected. `S30audio` selects SPK at boot.
- The codec's capture input is **not a microphone**: it is the speaker echo reference (`adc-for-loopback`; a speech test left it at −63.5 dBFS, while tones on SPK came back nearly flat from 150 Hz to 8 kHz, about +27 dB from DAC to ADC).

## Microphones (PDM)

- The stock vendor partition has an AISpeech voice HAL with a "dual card" capture (`ff0a0000.pdm-rk817-hifi`) and an echo-canceller config for 4 capture channels, 2 microphones and 1 reference.
- `px30.dtsi` has no PDM node, so the board DTS adds it (`rockchip,px30-pdm`, values from the stock DT) with a generic `dmic-codec` and a second simple card `pdm-mics`. Data line 0 (GPIO2_C5) stays with the I2S1 SDI; both PDM clock outputs and data lines 1–3 are muxed.
- Only channels 2 and 3 (data line 1, GPIO3_D0) carry a signal: room noise about −64 dBFS, speech −56…−63 dBFS without gain. The two channels are highly correlated (0.94, no delay at 16 kHz); whether they are two capsules or one capsule read on both clock edges is not proven. The app uses their mean with a fixed gain.
- Speaker measured with the microphones: 1.5 kHz on SPK at Master Playback Volume 200 and −6 dBFS → −31.5 dBFS at the microphones; on HP nothing.

## Wake word benchmark

- Tool: `tools/wakeword_bench/` (C++, TensorFlow Lite 2.11 from Buildroot, TFLite micro frontend with the ESPHome `micro_wake_word` feature settings, one thread). Models: microWakeWord v2 `okay_nabu`, `hey_jarvis` and `vad`.
- CPU: governor `schedutil`, OPPs 600–1296 MHz.
- **Synthetic** (random input, clock at 1296 MHz): okay_nabu 161 µs, hey_jarvis 127 µs, vad 80 µs per inference; one inference per model every 30 ms → 1.23 % of one core for all three.
- **Live** (microphone, gain ×8, real room sound): frontend 164 µs per 10 ms step; okay_nabu 513 µs, hey_jarvis 374 µs, vad 248 µs per inference (the CPU clocks down between the short bursts). Whole process **5.6 % of one core**.
- No false wake on room speech (highest okay_nabu 0.62, cutoff 0.97); both words were detected when spoken, with no cross-detection.
- `arecord` writes a WAV header to stdout unless `-t raw` is given; the 44-byte header shifts 4-channel frames by two channels.

## Boot time

- The stock bootloader phase grows with the image size, about 1 s per MB: about 50 s with a 49 MB image, about 45 s with the current 41 MB image (reset to kernel start). It cannot be split further without the serial console.
- From kernel start: `/init` at about 1.1 s, boot splash at 1.7 s, the app starts at about 5 s, Wi-Fi associates at about 10.5 s, the UI takes over at about 12 s.
- The kernel is trimmed to the board (`kernel/configs/nspanel.config`): no USB, Ethernet, SCSI/SATA, MTD, SPI, NFS, SquashFS, netfilter, EFI, kexec, hibernation, MD, HID, remoteproc, SCMI, virtio, KALLSYMS_ALL, profiling, IKCONFIG, security modules; optimised for size. Image 17.8 MB instead of 28.9 MB. `CONFIG_MODULES=n` is a trap: it turns every defconfig `=m` into `=y` and makes the kernel larger.
- A first boot of a new kernel goes as a one-shot boot (`nspanel-bootmode android`, then `reboot-to recovery`): a failure ends in the stock Android after the panic reboot.

## Boot splash

- `buildroot/board/nspanel/src/nspanel-splash.c`, started by `S00splash` right after `/init` (backlight on at the same time): a static binary with plain DRM ioctls that plays `usr/share/nspanel/splash.bin`.
- `tools/splash/storyboard.py` renders the animation pixel-exact from `assets/logo.svg` and `assets/logotype.svg` (own SVG rasteriser `svgraster.py`, both fill rules, 8× supersampling) as delta frames; the player only copies changed pixels.
- Sequence (5.5 s, 60 fps): the logotype "L⌂UDEN" (white letters, red house as the A) → the letters fade and the red house glides to the logo's place → the red disc grows and swallows it → the white line draws itself (every segment as its own straight band; the join appears with the inner roof) → the small logotype with "Elevate your home." fades in under the logo.
- After the finished picture (kept at least 1 s) the splash waits for Wi-Fi, i.e. a default route on `wlan0` in `/proc/net/route`. Meanwhile the small logotype dims to 60 % (the slogan does not), a soft light band (Gaussian, σ 14 px) crosses it in 1.2 s at 30 fps, then a 0.4 s dimmed pause, and this repeats. A 0.4 s fade leads into the loop and out of it, back to the finished picture.
- Hand-over: the app sends SIGUSR1 when it wants the display. Once Wi-Fi is up (checked at the end of each pass), or 30 s after the splash started, the splash gives up DRM master, keeps the picture and creates `/var/run/nspanel-splash.done`. The app waits up to 40 s for it, shows the same picture (`app/assets/intro.rgba`) as its first frame and fades it into the UI. The splash logs the hand-over to the kernel log (`nspanel-splash: display handed over …`).
- `splash.bin` v3 (1.36 MB): the sequence, the fade-in, the loop (starting and ending on the dimmed picture, so it repeats without a seam) and the fade-out.
- The kernel's framebuffer console still binds (`Console: switching to colour frame buffer device 60x30`). With `CONFIG_LOGO` on, the penguin logo stayed in its buffer and showed again for a moment at shutdown, when the app released the display.

## Bootloader logo (not done)

- The stock U-Boot only draws a logo when the kernel DTB carries the vendor display description. From the stock resource image: `logo.bmp` 480×480, 8-bit palette, uncompressed; DTB `display-subsystem/route/route-dsi` with `logo,uboot = "logo.bmp"`, `logo,mode = "center"`, `connect` to the VOP-big DSI output; DSI panel `simple-panel-dsi` with `panel-init-sequence` and `display-timings`.
- A first attempt added these vendor nodes (route-dsi, `logo-memory-region` + `rockchip,drm-logo` reserved memory, a vendor `simple-panel-dsi` node with the stock init sequence and timings, `mipi_dphy` phy name, `hs_clk` from the PHY, `rockchip,px30-mipi-dphy` as second PHY compatible, `rockchip,lane-rate`) and packed `logo.bmp` into the resource image. Result: the stock U-Boot hung before loading the kernel (black screen); a power cycle brought up the stock Android. Which node causes the hang is unknown; the next attempt needs the serial console.
- The generated logo is in `assets/bootlogo.bmp`; packing is switched off in `tools/build_image.sh`.

## Build pitfalls

- `tools/build_image.sh` strips Windows line ends only from text files (`grep -I`). Stripping every file once removed the 0D byte of each 0D 0A pair in `splash.bin`, and the animation stopped after its first frame. Compare binaries in the target with the source (md5).
- The syslog-to-kmsg forwarder must skip `kern.*` lines and every line containing ` kernel:` or `syslog:`, otherwise it feeds its own output back (klogd → syslog → kmsg → klogd) and keeps a core busy.
- Restart services over SSH with `setsid`, otherwise the app supervisor dies with the SSH session.
