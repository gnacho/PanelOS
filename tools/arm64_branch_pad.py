#!/usr/bin/env python3
"""Wrap an arm64 Linux Image so it runs from a non-2 MiB-aligned load address.

The stock Rockchip U-Boot executes an uncompressed Android-image kernel in
place at kernel_addr_r = 0x02080000, which suited the old vendor kernel
(text_offset 0x80000). Mainline kernels have text_offset 0 and must start on a
2 MiB boundary. This tool writes:

  [B +offset][zero padding up to offset][Image][zero padding up to image_size]

so the Image itself lands on the next 2 MiB boundary (offset 0x180000 for a
load address of 0x...80000) and the first instruction branches to it. x0..x3
(the FDT pointer from U-Boot) are left untouched. The trailing padding reserves
the kernel's full image_size (BSS included) so nothing U-Boot places after the
blob (the ramdisk) can overlap the running kernel.

Usage: arm64_branch_pad.py <Image> <out.bin> [--offset 0x180000]
"""
import argparse
import struct
import sys


def main() -> int:
    p = argparse.ArgumentParser()
    p.add_argument("image")
    p.add_argument("output")
    p.add_argument("--offset", type=lambda x: int(x, 0), default=0x180000)
    a = p.parse_args()

    with open(a.image, "rb") as f:
        image = f.read()
    if image[56:60] != b"ARM\x64":
        print("not an arm64 Image (missing ARM\\x64 magic)")
        return 1
    text_offset, image_size = struct.unpack_from("<QQ", image, 8)
    if text_offset != 0:
        print(f"unexpected text_offset 0x{text_offset:x}")
        return 1
    if a.offset % 4 or a.offset >= (1 << 27):
        print("offset must be a multiple of 4 and below 128 MiB")
        return 1
    image_size = max(image_size, len(image))

    branch = 0x14000000 | ((a.offset // 4) & 0x03FFFFFF)  # B #offset
    blob = struct.pack("<I", branch) + b"\0" * (a.offset - 4) + image
    blob += b"\0" * (a.offset + image_size - len(blob))

    with open(a.output, "wb") as f:
        f.write(blob)
    print(f"{a.output}: branch +0x{a.offset:x}, Image {len(image)} bytes, "
          f"image_size {image_size}, blob {len(blob)} bytes")
    return 0


if __name__ == "__main__":
    sys.exit(main())
