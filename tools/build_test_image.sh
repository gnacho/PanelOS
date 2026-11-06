#!/usr/bin/env bash
# Build the bring-up test image for the recovery partition.
# Runs in WSL/Linux as root. Output: $OUT/recovery-test.img (+ md5), also copied
# to /tmp/nspanel_out so Windows can reach it through \\wsl.localhost.
#
# Environment (defaults):
#   REPO  this repository                  KSRC  mainline kernel source tree
#   OUT   build output directory           JOBS  parallel jobs
set -euo pipefail

REPO="${REPO:-$(cd "$(dirname "$0")/.." && pwd)}"
KSRC="${KSRC:-/root/nspanel/src/linux}"
OUT="${OUT:-/root/nspanel/out}"
JOBS="${JOBS:-$(nproc)}"
DTB_NAME=px30-sonoff-nspanel-pro-86
export ARCH=arm64 CROSS_COMPILE=aarch64-linux-gnu-

mkdir -p "$OUT"
cd "$KSRC"

echo "== patches"
for p in "$REPO"/kernel/patches/*.patch; do
  if git apply --check -R "$p" 2>/dev/null; then
    echo "already applied: $(basename "$p")"
  else
    git apply "$p"
    echo "applied: $(basename "$p")"
  fi
done

echo "== device tree"
cp "$REPO/kernel/dts/$DTB_NAME.dts" arch/arm64/boot/dts/rockchip/
grep -q "$DTB_NAME.dtb" arch/arm64/boot/dts/rockchip/Makefile ||
  echo "dtb-\$(CONFIG_ARCH_ROCKCHIP) += $DTB_NAME.dtb" >> arch/arm64/boot/dts/rockchip/Makefile

echo "== config"
make defconfig >/dev/null
# merge_config.sh cannot handle spaces in paths, so use a copy
cp "$REPO/kernel/configs/nspanel.config" "$OUT/nspanel.config"
scripts/kconfig/merge_config.sh -m .config "$OUT/nspanel.config" >/dev/null
make olddefconfig >/dev/null
missing=0
for o in DRM_ROCKCHIP ROCKCHIP_VOP ROCKCHIP_DW_MIPI_DSI PHY_ROCKCHIP_INNO_DSIDPHY \
         DRM_PANEL_SITRONIX_ST7701 DRM_FBDEV_EMULATION FRAMEBUFFER_CONSOLE BACKLIGHT_PWM \
         PWM_ROCKCHIP I2C_RK3X MFD_RK8XX_I2C REGULATOR_RK808 MMC_DW_ROCKCHIP PSTORE_RAM \
         PSTORE_CONSOLE ROCKCHIP_THERMAL SYSCON_REBOOT_MODE BLK_DEV_INITRD \
         CFG80211 MAC80211 RTW88_8723DS PWRSEQ_SIMPLE EXT4_FS; do
  if ! grep -q "^CONFIG_$o=y" .config; then
    echo "NOT BUILT IN: CONFIG_$o ($(grep "CONFIG_$o[= ]" .config || echo unset))"
    missing=1
  fi
done
[ "$missing" -eq 0 ] && echo "all required options are built in"

echo "== kernel + dtb"
make -j"$JOBS" Image "rockchip/$DTB_NAME.dtb" 2>&1 | grep -E "error|warning: .*$DTB_NAME|Image|DTC" | tail -20
IMAGE=arch/arm64/boot/Image
DTB="arch/arm64/boot/dts/rockchip/$DTB_NAME.dtb"
ls -la "$IMAGE" "$DTB"

echo "== test init + initramfs"
aarch64-linux-gnu-gcc -static -Os -Wall -o "$OUT/init" "$REPO/testinit/init.c"
aarch64-linux-gnu-strip "$OUT/init"
gcc -O2 -o "$OUT/gen_init_cpio" usr/gen_init_cpio.c
cat > "$OUT/initramfs.list" <<EOL
dir /dev 0755 0 0
nod /dev/console 0600 0 0 c 5 1
dir /proc 0755 0 0
dir /sys 0755 0 0
file /init $OUT/init 0755 0 0
EOL
"$OUT/gen_init_cpio" "$OUT/initramfs.list" | gzip -9 > "$OUT/initramfs.cpio.gz"

echo "== resource (DTB) + boot image"
python3 "$REPO/tools/pack_rk_resource.py" "$OUT/resource.img" "rk-kernel.dtb=$DTB"
# The stock U-Boot runs the kernel in place at 0x02080000; mainline needs a
# 2 MiB-aligned start, so put the Image at +0x180000 behind a branch.
python3 "$REPO/tools/arm64_branch_pad.py" "$IMAGE" "$OUT/kernel.bin" --offset 0x180000
IMAGE="$OUT/kernel.bin"
CMDLINE="earlycon=uart8250,mmio32,0xff160000 console=ttyS2,1500000n8 console=tty0 loglevel=8 panic=10 clk_ignore_unused pd_ignore_unused regulator_ignore_unused rdinit=/init"
# Own packer: the Ubuntu mkbootimg package is broken (missing "gki" module).
# Defaults (base, offsets, page size, OS version) match the stock images.
python3 "$REPO/tools/mkbootimg_v0.py" --kernel "$IMAGE" --ramdisk "$OUT/initramfs.cpio.gz" \
  --second "$OUT/resource.img" --cmdline "$CMDLINE" -o "$OUT/recovery-test.img"

size=$(stat -c %s "$OUT/recovery-test.img")
limit=$((131072 * 512))
echo "recovery-test.img: $size bytes (partition: $limit)"
[ "$size" -le "$limit" ] || { echo "ERROR: image does not fit the recovery partition"; exit 1; }
(cd "$OUT" && md5sum recovery-test.img > recovery-test.img.md5 && cat recovery-test.img.md5)

mkdir -p /tmp/nspanel_out
cp "$OUT/recovery-test.img" "$OUT/recovery-test.img.md5" /tmp/nspanel_out/
chmod -R a+rX /tmp/nspanel_out
echo "done: /tmp/nspanel_out/recovery-test.img"
