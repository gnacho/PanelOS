# Hardware

Measured on one Sonoff NSPanel Pro 86 unit (stock firmware 3.0.0, Android 8.1, vendor kernel 4.4.138) through ADB with root.

## Main components

| Part | Chip | Bus / address | Stock driver | Notes |
|---|---|---|---|---|
| SoC | Rockchip PX30, 4x Cortex-A35 @ 1.5 GHz | | | DT model string: "Rockchip PX30 evb ddr3 board" |
| GPU | Mali-G31 MP1 (Bifrost) | | mali | mainline: Panfrost |
| RAM | 2 GB DDR3 | | | |
| Storage | eMMC, 15269888 sectors (~7.3 GiB) | mmcblk1 | | |
| Display | 480x480 MIPI-DSI panel, Sitronix ST7701S | DSI | vendor | mainline `panel-sitronix-st7701` with a new descriptor (`kernel/patches/0001`) |
| Touch | Hynitron CST226SE | i2c-1 0x5a | cst226se | mainline `hynitron_cstxxx` with `kernel/patches/0002`; an unbound "CST226" node also sits at i2c-1 0x1a |
| Light + proximity | Sensortek STK3x3x | i2c-2 0x47 | sensors | stock exposes input devices "lightsensor-level" and "proximity" |
| Light + proximity (alt part) | Sensortek STK3A5X | i2c-2 0x46 | none bound | second-source footprint, unused on this unit |
| Unknown | "tp" | i2c-2 0x3c | smatek_dridev | vendor device, function unknown |
| PMIC + audio codec | Rockchip RK809 | i2c-0 0x20 | rk808, rk809-codec | power key is input "rk8xx_pwrkey"; codec on I2S1 (MCLK GPIO2_C3, SCLK C2, LRCK C1, SDO C4, SDI C5); its ADC input is the speaker echo reference, not a microphone |
| Microphones | digital (PDM) | SoC PDM controller ff0a0000, data line 1 = GPIO3_D0 | AISpeech voice HAL (AEC: 2 mics + 1 reference) | verified by a speech test (channels 2+3); clock output (GPIO2_C6 or GPIO3_C7) not identified yet |
| Wi-Fi + Bluetooth | Realtek RTL8723DS | SDIO (024c:d723), BT on UART | 8723ds (out of tree) | mainline: rtw88 8723DS SDIO (works) |
| Zigbee | Silicon Labs radio | UART (one of ttyS3/4/5) | userspace | optional, router role at most |
| Ethernet MAC | rk_gmac-dwmac | | | present in DT, no connector expected |
| Backlight | PWM | /sys/class/backlight/backlight | pwm-backlight | 0..255 |
| Thermal | soc-thermal, gpu-thermal | /sys/class/thermal | | |

## Board and opening the panel

Board marking on a pre-production unit: `ST-PX30-T6E-V1.0`, dated 2021-07. Details below come from blakadder's 2022 teardown photos (https://blakadder.com/nspanel-pro-sideload/) and the vendor device tree; check them against your own unit.

- The wall base holds the mains-powered part. The display unit gets low voltage through the 8-pin connector in the middle of its back. Disconnect mains before you work on anything.
- Opening the display unit: remove the two screws in the top corners of the back cover, pry the cover off starting at the notch on the top edge, then remove the five small screws that hold the board.
- To lift the board, peel the tape off the touch-panel connector (marked `TP`, bottom right), flip up its latch and pull the orange flex out. Do not touch the display flex (marked `LCD`, right edge).
- `OTG`: micro-USB port on the bottom edge of the board, used for ADB and Rockchip USB flashing.
- `BOOT`: tactile button on the right side, next to the `LCD` connector. Verified: holding it while plugging in the OTG USB cable (no other power) makes the panel enumerate as `2207:330d` "USB download gadget": the vendor U-Boot's rockusb Loader mode (`rkdeveloptool ld`), not the boot ROM's mask ROM mode. Loader mode depends on an intact U-Boot on the eMMC.
- Power over the OTG port: a laptop USB port is enough for U-Boot download mode, but **not for a full boot** (the stock Android hangs on the Sonoff logo indefinitely). From a 2 A USB charger it boots normally. For work that needs data and full power at the same time, use a powered USB hub.
- Debug UART pads at the top left: `GND`, `RX`, `TX`, `VCC` (plus `RST`). Stock console: UART2 (`ff160000`), **1500000 baud, 8N1**, 3.3 V logic (vendor bootargs `console=ttyFIQ0 earlyprintk=uart8250,mmio32,0xff160000`). Needs a USB-UART adapter that supports 1.5 Mbaud (FT232, CH340, CP2102N).
- On reassembly, seat the light/proximity sensor board (marked `ALS`, top centre) correctly.

## Display panel

Decoded from the vendor device tree (`resource` partition, `rk-kernel.dtb`, node `dsi@ff450000/panel@0`, `simple-panel-dsi`).

- Controller: **Sitronix ST7701S** (the init sequence uses its `FF 77 01 00 00 1x` bank-select commands). Mainline has a driver for this family, `drivers/gpu/drm/panel/panel-sitronix-st7701.c`; `kernel/patches/0001` adds this panel's descriptor.
- 480x480, 72 x 70 mm, MIPI-DSI 2 lanes, RGB888, flags `0x803` (video mode, burst, low-power commands).
- Timing: pixel clock 16 MHz; horizontal front porch 12, sync 10, back porch 15; vertical front porch 12, sync 8, back porch 15; all sync signals active low.
- Power: RK809 `SWITCH_REG2`. Reset: GPIO3_A0, active low. Delays: reset 20 ms, init 20 ms, prepare 20 ms, enable 40 ms.
- Backlight: `pwm-backlight` on PWM1 (`pwm@ff200010`), period 25 us (40 kHz).

Init sequence (DCS, in order; delays in ms after the command):

| Cmd | Data | Delay |
|---|---|---|
| FF | 77 01 00 00 10 | |
| C0 | 3B 00 | |
| C1 | 0D 02 | |
| C2 | 21 08 | |
| B0 | 00 11 18 0E 11 06 07 08 07 22 04 12 0F AA 31 18 | |
| B1 | 00 11 19 0E 12 07 08 08 08 22 04 11 11 A9 32 18 | |
| FF | 77 01 00 00 11 | |
| B0 | 60 | |
| B1 | 30 | |
| B2 | 87 | |
| B3 | 80 | |
| B5 | 49 | |
| B7 | 85 | |
| B8 | 21 | |
| C1 | 78 | |
| C2 | 78 | 20 |
| E0 | 00 1B 02 | |
| E1 | 08 A0 00 00 07 A0 00 00 00 44 44 | |
| E2 | 11 11 44 44 ED A0 00 00 EC A0 00 00 | |
| E3 | 00 00 11 11 | |
| E4 | 44 44 | |
| E5 | 0A E9 D8 A0 0C EB D8 A0 0E ED D8 A0 10 EF D8 A0 | |
| E6 | 00 00 11 11 | |
| E7 | 44 44 | |
| E8 | 09 E8 D8 A0 0B EA D8 A0 0D EC D8 A0 0F EE D8 A0 | |
| EB | 02 00 E4 E4 88 00 40 | |
| EC | 3C 00 | |
| ED | AB 89 76 54 02 FF FF FF FF FF FF 20 45 67 98 BA | |
| FF | 77 01 00 00 00 | |
| 11 | (sleep out) | 120 |
| 29 | (display on) | 120 |

## GPIO and pin map (from the vendor device tree)

| Function | Pin | Notes |
|---|---|---|
| Panel reset | GPIO3_A0 | active low |
| Wi-Fi power sequence reset | GPIO0_A2 | active low, `mmc-pwrseq-simple` |
| Wi-Fi host wake IRQ | GPIO0_A5 | |
| Bluetooth reset | GPIO0_C1 | |
| Bluetooth device wake | GPIO0_A1 | |
| Bluetooth host wake IRQ | GPIO0_B2 | |
| Bluetooth UART RTS (GPIO mode) | GPIO1_C3 | 32 kHz clock from the RK809 |
| MBI5041 (bit-banged SPI1) | GPIO3_B4 MOSI, GPIO3_B7 CLK, GPIO3_B5 CS | 16-channel LED driver, purpose unknown |

UARTs enabled in the vendor DT: `serial@ff158000`, `serial@ff170000`, `serial@ff178000` (Bluetooth and the Zigbee radio are among them). The vendor DT also lists alternative parts that are disabled on this unit: Goodix GT1x and GSL2682 touch, APDS9933 light sensor, SHT21 and CHT8305 temperature/humidity sensors, MMA7660 accelerometer, AK8963 compass, OV5695 camera.

## eMMC partition map

512-byte sectors. Measured on firmware 3.0.0; check your own unit before writing.

| Name | Start sector | Size (sectors) | Size |
|---|---|---|---|
| uboot | 16384 | 8192 | 4 MiB |
| trust | 24576 | 8192 | 4 MiB |
| misc | 32768 | 8192 | 4 MiB |
| resource | 40960 | 32768 | 16 MiB |
| kernel | 73728 | 65536 | 32 MiB |
| boot | 139264 | 65536 | 32 MiB |
| recovery | 204800 | 131072 | 64 MiB |
| backup | 335872 | 229376 | 112 MiB |
| security | 565248 | 8192 | 4 MiB |
| cache | 573440 | 786432 | 384 MiB |
| system | 1359872 | 3145728 | 1.5 GiB |
| metadata | 4505600 | 32768 | 16 MiB |
| vendor | 4538368 | 786432 | 384 MiB |
| oem | 5324800 | 524288 | 256 MiB |
| frp | 5849088 | 1024 | 512 KiB |
| smatek | 5850112 | 32768 | 16 MiB |
| keypart | 5882880 | 512 | 256 KiB |
| STSN | 5883392 | 512 | 256 KiB |
| stback | 5883904 | 2097152 | 1 GiB |
| userdata | 7981056 | 7288799 | ~3.5 GiB |

The first 16384 sectors hold the partition table and the Rockchip idbloader.

## Stock boot and update chain

- Non-A/B Android 8.1 layout. The recovery partition installs OTA packages and verifies them against Sonoff's RSA key, so only vendor-signed packages install through recovery.
- The stock launcher refuses OTA checks while its server-side "rooted" flag is set, and the same flag switches ADB on and off (`settings put global adb_enabled 0/1`).
- Network ADB is enabled by the init rule `on property:persist.internet.adb.enable=1` (sets `service.adb.tcp.port 5555`).
