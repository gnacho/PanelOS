#!/usr/bin/env bash
# Make the device tree for the stock U-Boot from the panel's stock resource
# partition (a backup of it).
#
# The stock U-Boot drives its own drivers (display, eMMC) with the device tree
# of the boot image. With the stock device tree it shows the boot logo and
# loads the image in about 2 s; with the mainline one it showed nothing and
# took about 40 s. The chain loader (tools/chainload) then hands the kernel its
# own device tree. Changes to the stock device tree:
#   - /chosen/bootargs removed (stock Android arguments; U-Boot would put
#     them in front of the PanelOS command line),
#   - the backlight starts at the brightness of the boot animation.
#
# Usage: make_uboot_dtb.sh <resource.img> <dtc> <brightness 0-255> <out.dtb>
set -euo pipefail

usage="usage: $0 <resource.img> <dtc> <brightness 0-255> <out.dtb>"
RESOURCE="${1:?$usage}"
DTC="${2:?$usage}"
BRIGHTNESS="${3:?$usage}"
OUTDTB="${4:?$usage}"
HERE="$(cd "$(dirname "$0")" && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

python3 "$HERE/extract_rk_resource.py" "$RESOURCE" "$WORK/res" >/dev/null
[ -f "$WORK/res/rk-kernel.dtb" ] || { echo "no rk-kernel.dtb in $RESOURCE" >&2; exit 1; }
"$DTC" -I dtb -O dts -q -o "$WORK/stock.dts" "$WORK/res/rk-kernel.dtb"

awk -v level="$BRIGHTNESS" '
  /^\tchosen \{/    { node = "chosen" }
  /^\tbacklight \{/ { node = "backlight" }
  /^\t\};/          { node = "" }
  node == "chosen" && /^\t\tbootargs = / { next }
  node == "backlight" && /^\t\tdefault-brightness-level = / {
    printf "\t\tdefault-brightness-level = <%d>;\n", level; found = 1; next
  }
  { print }
  END { if (!found) { print "no backlight default-brightness-level found" > "/dev/stderr"; exit 1 } }
' "$WORK/stock.dts" > "$WORK/uboot.dts"

"$DTC" -I dts -O dtb -q -o "$OUTDTB" "$WORK/uboot.dts"
echo "$OUTDTB: stock device tree without bootargs, backlight $BRIGHTNESS ($(stat -c %s "$OUTDTB") bytes)"
