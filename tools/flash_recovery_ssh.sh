#!/usr/bin/env bash
# Write an image to the recovery partition from the running development system
# over SSH, verify it, and optionally boot it once ("reboot-to recovery").
# The stock Android on the boot partition is not touched.
#
# Usage: tools/flash_recovery_ssh.sh <host> <image> [--boot]
# Env:   NSPANEL_SSH_KEY (default /root/.ssh/nspanel_dev)
#        REBOOT_TO       local reboot-to binary, pushed if the target lacks it
set -euo pipefail

usage="usage: $0 <host> <image> [--boot]"
HOST="${1:?$usage}"
IMAGE="${2:?$usage}"
BOOT="${3:-}"
KEY="${NSPANEL_SSH_KEY:-/root/.ssh/nspanel_dev}"
REBOOT_TO="${REBOOT_TO:-/root/nspanel/br2-external/board/nspanel/rootfs_overlay/usr/sbin/reboot-to}"
# The panel keeps its host key (S49sshhostkey): accept it the first time, check it after.
SSH=(ssh -i "$KEY" -o StrictHostKeyChecking=accept-new -o UserKnownHostsFile="${KNOWN_HOSTS:-$HOME/.ssh/panelos_known_hosts}" -o LogLevel=ERROR "root@$HOST")

size=$(stat -c %s "$IMAGE")
limit=$((131072 * 512))
[ "$size" -le "$limit" ] || { echo "image is larger than the recovery partition"; exit 1; }
md5=$(md5sum < "$IMAGE" | cut -d' ' -f1)
echo "image: $size bytes, md5 $md5"

part=$("${SSH[@]}" 'for d in /sys/class/block/mmcblk0p*; do grep -qx "PARTNAME=recovery" "$d/uevent" && basename "$d"; done; true')
[ -n "$part" ] || { echo "recovery partition not found on the target"; exit 1; }
echo "target recovery partition: /dev/$part"

"${SSH[@]}" "cat > /tmp/recovery-new.img" < "$IMAGE"
dev_md5=$("${SSH[@]}" "md5sum /tmp/recovery-new.img" | cut -d' ' -f1)
[ "$dev_md5" = "$md5" ] || { echo "transfer corrupted: $dev_md5"; exit 1; }

written=$("${SSH[@]}" "dd if=/tmp/recovery-new.img of=/dev/$part bs=1M 2>/dev/null && sync && rm -f /tmp/recovery-new.img && head -c $size /dev/$part | md5sum" | cut -d' ' -f1)
[ "$written" = "$md5" ] || { echo "VERIFY FAILED: partition md5 $written"; exit 1; }
echo "recovery partition written and verified"

if [ "$BOOT" = "--boot" ]; then
  if ! "${SSH[@]}" 'command -v reboot-to >/dev/null'; then
    "${SSH[@]}" "cat > /tmp/reboot-to && chmod 755 /tmp/reboot-to" < "$REBOOT_TO"
    cmd=/tmp/reboot-to
  else
    cmd=reboot-to
  fi
  echo "rebooting into recovery"
  "${SSH[@]}" "$cmd recovery" || true
fi
