#!/usr/bin/env bash
# Takes the stock Android off a panel that runs PanelOS, over SSH, one step at
# a time. Nothing here needs the panel opened as long as PanelOS starts.
#
#   tools/remove_android.sh <host> check   what is on the panel now; changes nothing
#   tools/remove_android.sh <host> copy    writes the PanelOS image that runs now
#                                          (recovery partition) to the boot partition
#                                          too: Android cannot start any more, and a
#                                          normal boot starts this second copy
#   tools/remove_android.sh <host> test    boots once from the boot partition, checks
#                                          that PanelOS came up, makes the recovery
#                                          partition the default again
#   tools/remove_android.sh <host> data    formats the old Android userdata partition
#                                          (about 3.5 GiB) as ext4, label panelos-data;
#                                          PanelOS mounts it at /mnt/data
#   tools/remove_android.sh <host> restore <backup folder>
#                                          the way back: writes the stock boot and
#                                          recovery partitions from this panel's
#                                          backup, clears userdata, starts the stock
#                                          firmware (docs/restore.md, case D)
#
# The other Android partitions (system, vendor, oem, ...) stay as they are,
# unused.
# Env: NSPANEL_SSH_KEY (default /root/.ssh/nspanel_dev), KNOWN_HOSTS, YES=1 (no questions)
set -euo pipefail

usage="usage: $0 <host> check|copy|test|data|restore <backup folder>"
HOST="${1:?$usage}"
STEP="${2:?$usage}"
KEY="${NSPANEL_SSH_KEY:-/root/.ssh/nspanel_dev}"
SSH=(ssh -n -i "$KEY" -o ConnectTimeout=8 -o StrictHostKeyChecking=accept-new
  -o UserKnownHostsFile="${KNOWN_HOSTS:-$HOME/.ssh/panelos_known_hosts}" -o LogLevel=ERROR "root@$HOST")
on_panel() { "${SSH[@]}" "$@"; }
# The same without -n, for sending a file on stdin.
push() { ssh -i "$KEY" -o ConnectTimeout=8 -o StrictHostKeyChecking=accept-new   -o UserKnownHostsFile="${KNOWN_HOSTS:-$HOME/.ssh/panelos_known_hosts}" -o LogLevel=ERROR "root@$HOST" "$@"; }

part() {
  on_panel "for d in /sys/class/block/mmcblk0p*; do grep -qx PARTNAME=$1 \$d/uevent && basename \$d; done; true"
}

# Length of the Android boot image at the start of a partition (0: none).
image_len() {
  local dev=$1 magic words
  magic=$(on_panel "head -c 8 /dev/$dev" | tr -d '\0')
  [ "$magic" = "ANDROID!" ] || { echo 0; return; }
  read -r -a words <<<"$(on_panel "head -c 40 /dev/$dev | od -A n -t u4 -v" | tr '\n' ' ')"
  # words: magic magic kernel_size kernel_addr ramdisk_size ramdisk_addr second_size second_addr tags page
  local ks=${words[2]} rs=${words[4]} ss=${words[6]} pg=${words[9]}
  up() { echo $(( ($1 + pg - 1) / pg * pg )); }
  echo $(( pg + $(up "$ks") + $(up "$rs") + $(up "$ss") ))
}

image_md5() { on_panel "head -c $2 /dev/$1 | md5sum" | cut -d' ' -f1; }

confirm() {
  [ "${YES:-}" = 1 ] && return 0
  echo "$1"
  read -r -p "Type yes to go on: " answer </dev/tty
  [ "$answer" = yes ] || { echo "stopped, nothing changed"; exit 1; }
}

REC=$(part recovery)
BOOT=$(part boot)
DATA=$(part userdata)
[ -n "$REC" ] && [ -n "$BOOT" ] && [ -n "$DATA" ] || { echo "recovery, boot or userdata partition not found"; exit 1; }
rec_len=$(image_len "$REC")
[ "$rec_len" -gt 0 ] || { echo "no PanelOS image in the recovery partition"; exit 1; }
boot_size=$(( $(on_panel "cat /sys/class/block/$BOOT/size") * 512 ))

copy_done() {
  [ "$(image_len "$BOOT")" = "$rec_len" ] && [ "$(image_md5 "$BOOT" "$rec_len")" = "$(image_md5 "$REC" "$rec_len")" ]
}

case "$STEP" in
check)
  echo "recovery /dev/$REC: PanelOS image, $rec_len bytes, md5 $(image_md5 "$REC" "$rec_len")"
  if copy_done; then
    echo "boot /dev/$BOOT: the same PanelOS image (Android removed)"
  else
    boot_len=$(image_len "$BOOT")
    echo "boot /dev/$BOOT ($boot_size bytes): another image ($boot_len bytes; the stock Android while it is there)"
    [ "$rec_len" -le "$boot_size" ] || echo "WARNING: the PanelOS image is larger than the boot partition"
  fi
  on_panel 'nspanel-bootmode status'
  echo "userdata /dev/$DATA: $(on_panel "blkid /dev/$DATA" || echo 'no file system PanelOS knows')"
  on_panel 'mountpoint -q /mnt/data && df -h /mnt/data | tail -1 || echo "/mnt/data: not mounted"'
  on_panel 'echo "up $(cut -d. -f1 /proc/uptime) s, app $(pidof nspanel-app || echo "not running")"'
  ;;
copy)
  copy_done && { echo "the boot partition already holds this PanelOS image"; exit 0; }
  [ "$rec_len" -le "$boot_size" ] || { echo "the PanelOS image ($rec_len bytes) does not fit the boot partition ($boot_size bytes)"; exit 1; }
  on_panel 'nspanel-bootmode status' | grep -q "open system" || { echo "the default boot must be PanelOS (nspanel-bootmode open) first"; exit 1; }
  confirm "This writes the running PanelOS image over the boot partition (/dev/$BOOT). The stock Android will not start any more; the way back is the backup (docs/restore.md)."
  on_panel "dd if=/dev/$REC of=/dev/$BOOT bs=2048 count=$((rec_len / 2048)) 2>/dev/null && sync"
  copy_done || { echo "VERIFY FAILED: the boot partition does not match the recovery image"; exit 1; }
  echo "boot partition written and verified: PanelOS is on both partitions"
  ;;
test)
  copy_done || { echo "run the copy step first"; exit 1; }
  confirm "The panel restarts once from the boot partition (normal boot), then the recovery partition becomes the default again. If it does not come back within 5 minutes, power it off and on once; docs/restore.md has the rest."
  old_boot=$(on_panel 'cat /proc/sys/kernel/random/boot_id')
  # A plain reboot writes the "normal" reboot mode, so with the misc boot
  # command cleared the bootloader starts the boot partition. Detached, or the
  # restart dies with the SSH session.
  on_panel 'nspanel-bootmode android >/dev/null && setsid sh -c "sleep 1; reboot" </dev/null >/dev/null 2>&1 &'
  echo "restarting from the boot partition..."
  sleep 15
  for _ in $(seq 1 57); do
    if boot=$(on_panel 'cat /proc/sys/kernel/random/boot_id' 2>/dev/null) && [ -n "$boot" ] && [ "$boot" != "$old_boot" ]; then
      on_panel 'nspanel-bootmode status; echo "up $(cut -d. -f1 /proc/uptime) s, app $(pidof nspanel-app || echo "not running")"'
      on_panel 'nspanel-bootmode open'
      echo "PanelOS started from the boot partition; the default boot is the recovery partition again"
      exit 0
    fi
    sleep 5
  done
  echo "the panel did not come back within 5 minutes: power it off and on once; if it still does not start, see docs/restore.md"
  exit 1
  ;;
data)
  copy_done || { echo "the userdata partition is Android's until the copy step has run"; exit 1; }
  on_panel 'command -v mkfs.ext4 >/dev/null' || { echo "this PanelOS has no mkfs.ext4 (e2fsprogs); it comes with the next image"; exit 1; }
  on_panel "blkid /dev/$DATA" | grep -q 'LABEL="panelos-data"' && { echo "already formatted for PanelOS"; exit 0; }
  confirm "This formats the old Android userdata partition (/dev/$DATA, Android's apps and settings) for PanelOS."
  on_panel "mkfs.ext4 -q -F -L panelos-data -m 0 /dev/$DATA && mkdir -p /mnt/data && mount -t ext4 -o rw,noatime /dev/$DATA /mnt/data && df -h /mnt/data | tail -1"
  echo "userdata formatted and mounted at /mnt/data"
  ;;
restore)
  BK="${3:?$usage}"
  for p in boot recovery; do
    f="$BK/$p.img"
    [ -f "$f" ] || { echo "$f not found"; exit 1; }
    dev=$(part "$p")
    want=$(( $(on_panel "cat /sys/class/block/$dev/size") * 512 ))
    [ "$(stat -c %s "$f")" = "$want" ] || { echo "$f is not the size of this panel's $p partition"; exit 1; }
  done
  confirm "This writes the stock boot and recovery partitions back from $BK (a backup of THIS panel only), clears the old userdata partition (what PanelOS kept in /mnt/data is lost) and restarts into the stock firmware."
  for p in boot recovery; do
    f="$BK/$p.img"
    dev=$(part "$p")
    md5=$(md5sum < "$f" | cut -d' ' -f1)
    push "cat > /tmp/stock-$p.img" < "$f"
    got=$(on_panel "dd if=/tmp/stock-$p.img of=/dev/$dev bs=1M 2>/dev/null && sync && rm -f /tmp/stock-$p.img && md5sum < /dev/$dev" | cut -d' ' -f1)
    [ "$got" = "$md5" ] || { echo "VERIFY FAILED on the $p partition; PanelOS still runs, do not restart, run this step again"; exit 1; }
    echo "$p partition written and verified"
  done
  on_panel "mountpoint -q /mnt/data && umount /mnt/data; dd if=/dev/zero of=/dev/$DATA bs=1M count=4 2>/dev/null; sync"
  on_panel 'nspanel-bootmode android && setsid sh -c "sleep 1; reboot" </dev/null >/dev/null 2>&1 &'
  echo "restarting into the stock firmware"
  ;;
*)
  echo "$usage" >&2
  exit 2
  ;;
esac
