#!/usr/bin/env bash
# Write an image to the recovery partition from the running stock Android,
# over (Wi-Fi) ADB, with md5 checks before and after. Optionally boot it once.
#
# Usage: tools/flash_recovery_adb.sh <adb-serial> <image> [--boot]
# Example: tools/flash_recovery_adb.sh <panel-ip>:5555 recovery-test.img --boot
# Restore the stock recovery: tools/flash_recovery_adb.sh <serial> backup/<dir>/recovery.img
# Set ADB=/path/to/adb if adb is not on PATH. Works in Linux and Git Bash.
set -euo pipefail

usage="usage: $0 <adb-serial> <image> [--boot]"
SERIAL="${1:?$usage}"
IMAGE="${2:?$usage}"
BOOT="${3:-}"
ADB="${ADB:-adb}"
TMP=/data/local/tmp/recovery-new.img
LIMIT=$((131072 * 512))
export MSYS_NO_PATHCONV=1

size=$(wc -c < "$IMAGE")
[ "$size" -le "$LIMIT" ] || { echo "image is larger than the recovery partition"; exit 1; }
local_md5=$(md5sum < "$IMAGE" | cut -d' ' -f1)
echo "image: $size bytes, md5 $local_md5"

"$ADB" connect "$SERIAL" >/dev/null 2>&1 || true
"$ADB" -s "$SERIAL" push "$IMAGE" "$TMP"
dev_md5=$("$ADB" -s "$SERIAL" shell "md5sum $TMP" | cut -d' ' -f1 | tr -d '\r')
[ "$dev_md5" = "$local_md5" ] || { echo "push corrupted: device md5 $dev_md5"; exit 1; }

"$ADB" -s "$SERIAL" shell "su -c 'cat $TMP > /dev/block/by-name/recovery && sync'"
written_md5=$("$ADB" -s "$SERIAL" shell "su -c 'head -c $size /dev/block/by-name/recovery | md5sum'" | cut -d' ' -f1 | tr -d '\r')
"$ADB" -s "$SERIAL" shell "rm -f $TMP"
if [ "$written_md5" != "$local_md5" ]; then
  echo "VERIFY FAILED: recovery partition md5 $written_md5"
  exit 1
fi
echo "recovery partition written and verified"

if [ "$BOOT" = "--boot" ]; then
  echo "booting into recovery once (a normal reboot returns to stock Android)"
  "$ADB" -s "$SERIAL" reboot recovery
fi
