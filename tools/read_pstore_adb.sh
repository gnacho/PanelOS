#!/usr/bin/env bash
# After a test boot that ended with a warm reboot, read the previous boot's
# console log that the stock Android exposes in /sys/fs/pstore.
#
# Usage: tools/read_pstore_adb.sh <adb-serial> <output-file>
set -euo pipefail

usage="usage: $0 <adb-serial> <output-file>"
SERIAL="${1:?$usage}"
OUTFILE="${2:?$usage}"
ADB="${ADB:-adb}"
export MSYS_NO_PATHCONV=1

"$ADB" connect "$SERIAL" >/dev/null 2>&1 || true
"$ADB" -s "$SERIAL" shell "su -c 'ls -la /sys/fs/pstore/'"
"$ADB" -s "$SERIAL" shell "su -c 'cat /sys/fs/pstore/console-ramoops* 2>/dev/null; cat /sys/fs/pstore/dmesg-ramoops* 2>/dev/null'" | tr -d '\r' > "$OUTFILE"
echo "saved $(wc -l < "$OUTFILE") lines to $OUTFILE"
