#!/usr/bin/env bash
# Prove that a backup can be restored over USB (Rockchip rockusb Loader mode).
#
# 1. Reads uboot and recovery back from the eMMC and compares them with the backup.
# 2. With --write: writes recovery.img back (identical content), reads it again
#    and compares. recovery is the safest partition: Android boots without it.
# 3. Reboots the panel.
#
# Usage (Linux, as root, panel in download mode):
#   tools/restore_test.sh <backup-dir> [--write]
# Sector map: docs/hardware.md (measured on stock firmware 3.0.0).
set -euo pipefail

BACKUP="${1:?usage: $0 <backup-dir> [--write]}"
WRITE="${2:-}"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# name start_sector size_sectors
PARTS=("uboot 16384 8192" "recovery 204800 131072")

echo "== device"
rkdeveloptool ld
if rkdeveloptool ld | grep -qi maskrom; then
  echo "Device is in Maskrom mode: a PX30 loader must be pushed with 'rkdeveloptool db' first. Stopping."
  exit 1
fi
rkdeveloptool rfi || true

# Note: in this rkdeveloptool build (Ubuntu 1.32+pine64) the rl start is in
# 512-byte sectors but the length is in BYTES.
check() {
  local name=$1 start=$2 size=$3 tag=$4
  rkdeveloptool rl "$start" $((size * 512)) "$WORK/$name.$tag.img" >/dev/null
  local a b
  a=$(md5sum < "$WORK/$name.$tag.img" | cut -d' ' -f1)
  b=$(md5sum < "$BACKUP/$name.img" | cut -d' ' -f1)
  if [ "$a" = "$b" ]; then
    echo "$name ($tag): OK  md5=$a"
  else
    echo "$name ($tag): MISMATCH  eMMC=$a backup=$b"
    return 1
  fi
}

echo "== read-back"
for p in "${PARTS[@]}"; do
  # shellcheck disable=SC2086
  check $p readback
done

if [ "$WRITE" = "--write" ]; then
  echo "== write test: recovery (identical content)"
  rkdeveloptool wl 204800 "$BACKUP/recovery.img"
  check recovery 204800 131072 afterwrite
fi

echo "== reboot"
rkdeveloptool rd
echo "done"
