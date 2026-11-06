#!/usr/bin/env bash
# Build the wake word benchmark for the panel (development tool, run as root
# in Linux/WSL) and collect a self-contained bundle: the binary, the shared
# libraries it needs that the panel image does not have, and the
# microWakeWord models.
#
# Needs a Buildroot output with tensorflow-lite (BR_BENCH, built from the
# panel defconfig plus BR2_PACKAGE_TENSORFLOW_LITE=y) and the kissfft source
# (KISSFFT, the v130 release, used by the TFLite micro frontend).
#
# Usage: tools/wakeword_bench/build.sh            -> $OUT/wakeword_bench.tar.gz
# Deploy (the panel's busybox tar has no -z):
#   gzip -dc bundle | ssh root@<panel> 'mkdir -p /tmp/mww && tar xf - -C /tmp/mww'
# Run:    LD_LIBRARY_PATH=/tmp/mww/lib /tmp/mww/wakeword_bench synth 2000 /tmp/mww/models/*.tflite
set -euo pipefail
export PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin

HERE="$(cd "$(dirname "$0")" && pwd)"
BR_BENCH="${BR_BENCH:-/root/nspanel/br-bench}"
KISSFFT="${KISSFFT:-/root/nspanel/kissfft/kissfft-130}"
OUT="${OUT:-/root/nspanel/out/wakeword_bench}"
MODELS="${MODELS:-okay_nabu hey_jarvis vad}"
MODEL_URL=https://raw.githubusercontent.com/esphome/micro-wake-word-models/main/models/v2

TFSRC=$(ls -d "$BR_BENCH"/build/tensorflow-lite-*/ | head -n 1)
SYSROOT="$BR_BENCH/host/aarch64-buildroot-linux-gnu/sysroot"
CXX=$(ls "$BR_BENCH"/host/bin/aarch64-*g++ | grep -v br_real | head -n 1)
CC=${CXX%g++}gcc
FE="$TFSRC/tensorflow/lite/experimental/microfrontend/lib"

rm -rf "$OUT"
mkdir -p "$OUT/obj" "$OUT/bundle/lib" "$OUT/bundle/models"

CFLAGS=(-O2 -mcpu=cortex-a35 --sysroot="$SYSROOT" -I"$TFSRC" -I"$KISSFFT" -I"$KISSFFT/tools" -DFIXED_POINT=16)
for src in "$FE"/*.c "$FE"/*.cc "$KISSFFT/kiss_fft.c" "$KISSFFT/tools/kiss_fftr.c"; do
  case "$src" in *_test.cc | *_io.c | *_io.cc | *main* | *generator*) continue ;; esac
  obj="$OUT/obj/$(basename "$src").o"
  case "$src" in
  *.c) "$CC" "${CFLAGS[@]}" -c "$src" -o "$obj" ;;
  *.cc) "$CXX" "${CFLAGS[@]}" -std=c++17 -c "$src" -o "$obj" ;;
  esac
done

"$CXX" -O2 -mcpu=cortex-a35 -std=c++17 --sysroot="$SYSROOT" -I"$TFSRC" -I"$SYSROOT/usr/include" \
  "$HERE/wakeword_bench.cc" "$OUT"/obj/*.o \
  -L"$SYSROOT/usr/lib" -Wl,-rpath-link="$SYSROOT/usr/lib" -ltensorflow-lite -lpthread -ldl \
  -o "$OUT/bundle/wakeword_bench"

# Shared libraries: everything the binary needs, resolved recursively from the
# Buildroot target tree (the panel image has glibc but not the TFLite stack).
need() { "${CXX%g++}readelf" -d "$1" 2>/dev/null | sed -n 's/.*(NEEDED).*\[\(.*\)\]/\1/p'; }
declare -A seen=()
queue=("$OUT/bundle/wakeword_bench")
while [ ${#queue[@]} -gt 0 ]; do
  f=${queue[0]}
  queue=("${queue[@]:1}")
  for lib in $(need "$f"); do
    [ -n "${seen[$lib]:-}" ] && continue
    seen[$lib]=1
    case "$lib" in libc.so.* | libm.so.* | libpthread.so.* | libdl.so.* | librt.so.* | ld-linux-*) continue ;; esac
    path=""
    for dir in "$BR_BENCH"/target/usr/lib "$BR_BENCH"/target/lib; do
      if [ -e "$dir/$lib" ]; then path="$dir/$lib"; break; fi
    done
    [ -n "$path" ] || { echo "library not found: $lib"; exit 1; }
    cp -L "$path" "$OUT/bundle/lib/"
    queue+=("$path")
  done
done

for m in $MODELS; do
  curl -fsSL -o "$OUT/bundle/models/$m.tflite" "$MODEL_URL/$m.tflite"
  curl -fsSL -o "$OUT/bundle/models/$m.json" "$MODEL_URL/$m.json"
done

tar czf "$OUT/wakeword_bench.tar.gz" -C "$OUT/bundle" .
ls -la "$OUT/bundle" "$OUT/bundle/lib" "$OUT/bundle/models"
du -sh "$OUT/bundle"
echo "bundle: $OUT/wakeword_bench.tar.gz"
