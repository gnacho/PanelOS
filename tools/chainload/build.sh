#!/usr/bin/env bash
# Build the chain loader (tools/chainload) into a flat binary.
# Usage: build.sh <kernel source tree> <output.bin>
# libfdt comes from the kernel tree (scripts/dtc/libfdt).
set -euo pipefail

KSRC="${1:?usage: build.sh <kernel source tree> <output.bin>}"
OUTBIN="${2:?usage: build.sh <kernel source tree> <output.bin>}"
HERE="$(cd "$(dirname "$0")" && pwd)"
LIBFDT="$KSRC/scripts/dtc/libfdt"
CC=aarch64-linux-gnu-gcc
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

CFLAGS="-Os -ffreestanding -fno-builtin -fno-tree-loop-distribute-patterns \
  -fno-stack-protector -fno-pic -fno-common -mstrict-align -mgeneral-regs-only \
  -mcmodel=small -ffunction-sections -fdata-sections -Wall -I$LIBFDT"

objs=()
for src in "$HERE/start.S" "$HERE/chain.c" "$LIBFDT/fdt.c" "$LIBFDT/fdt_ro.c" \
           "$LIBFDT/fdt_rw.c" "$LIBFDT/fdt_wip.c"; do
  obj="$WORK/$(basename "$src").o"
  $CC $CFLAGS -c "$src" -o "$obj"
  objs+=("$obj")
done

# Absolute data pointers would break when U-Boot loads us elsewhere; the code
# itself only uses PC-relative addressing (-mcmodel=small).
if aarch64-linux-gnu-objdump -r "${objs[@]}" | grep -q "R_AARCH64_ABS64"; then
  echo "absolute relocations found; the loader must stay position independent" >&2
  aarch64-linux-gnu-objdump -r "${objs[@]}" | grep "R_AARCH64_ABS64" >&2
  exit 1
fi

$CC -nostdlib -static -Wl,--gc-sections -Wl,-T,"$HERE/chain.ld" -o "$WORK/chain.elf" "${objs[@]}"
aarch64-linux-gnu-objcopy -O binary "$WORK/chain.elf" "$OUTBIN"
echo "$OUTBIN: $(stat -c %s "$OUTBIN") bytes"
