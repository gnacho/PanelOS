#!/usr/bin/env bash
# Back up NSPanel Pro eMMC partitions over ADB (stock Android, root via su).
# Binary-safe: each partition is copied on the device with cat, made readable,
# pulled with adb pull and verified with md5 on both sides.
# Do NOT use "adb exec-out ... > file": on Windows it turns LF into CRLF.
#
# Usage: tools/backup_partitions.sh <adb-serial> <output-dir> [partition ...]
# Example: tools/backup_partitions.sh <panel-ip>:5555 backup/2026-11-05
# Set ADB=/path/to/adb if adb is not on PATH.
set -euo pipefail

usage="usage: $0 <adb-serial> <output-dir> [partition ...]"
SERIAL="${1:?$usage}"
OUT="${2:?$usage}"
shift 2
PARTS=("$@")
if [ ${#PARTS[@]} -eq 0 ]; then
  PARTS=(uboot trust misc resource kernel boot recovery security smatek STSN frp keypart backup oem vendor system)
fi

ADB="${ADB:-adb}"
TMP=/data/local/tmp/nspanel_backup.img
export MSYS_NO_PATHCONV=1

mkdir -p "$OUT"
LOG="$OUT/backup_log.txt"
ok=0
bad=0

for p in "${PARTS[@]}"; do
  "$ADB" -s "$SERIAL" shell "su -c 'cat /dev/block/by-name/$p > $TMP; chmod 666 $TMP'"
  dev_md5=$("$ADB" -s "$SERIAL" shell "su -c 'md5sum $TMP'" | cut -d' ' -f1 | tr -d '\r')
  "$ADB" -s "$SERIAL" pull "$TMP" "$OUT/$p.img" >/dev/null
  local_md5=$(md5sum "$OUT/$p.img" | cut -d' ' -f1)
  "$ADB" -s "$SERIAL" shell "su -c 'rm -f $TMP'"
  if [ -n "$dev_md5" ] && [ "$dev_md5" = "$local_md5" ]; then
    status=OK
    ok=$((ok + 1))
  else
    status=MISMATCH
    bad=$((bad + 1))
  fi
  printf '%s %-10s %12s bytes md5=%s %s\n' "$(date +%H:%M:%S)" "$p" "$(wc -c < "$OUT/$p.img")" "$local_md5" "$status" | tee -a "$LOG"
done

echo "done: ok=$ok bad=$bad" | tee -a "$LOG"
[ "$bad" -eq 0 ]
