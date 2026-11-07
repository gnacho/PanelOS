#!/usr/bin/env python3
"""Pack the chain loader, the kernel's device tree and an arm64 Linux Image
into one blob for the kernel slot of an Android boot image.

The stock Rockchip U-Boot runs the kernel in place at 0x02080000; mainline
kernels must start on a 2 MiB boundary, so the Image sits at offset 0x180000.
Layout (see tools/chainload/start.S):

  0x000000  chain loader          0x100000  kernel device tree
  0x140000  work area (zeroed)    0x180000  Image, then zeros up to image_size

The trailing zeros reserve the kernel's full image_size (BSS included), so
nothing U-Boot places after the blob (the ramdisk) overlaps the running kernel.

Usage: arm64_chain_pack.py <chain.bin> <kernel.dtb> <Image> <out.bin>
"""
import struct
import sys

DTB_OFFSET, WORK_OFFSET, IMAGE_OFFSET = 0x100000, 0x140000, 0x180000
CODE_MAX = 0xE0000


def main() -> int:
    if len(sys.argv) != 5:
        print(__doc__)
        return 2
    chain, dtb, image = (open(p, "rb").read() for p in sys.argv[1:4])
    if image[56:60] != b"ARM\x64":
        print("not an arm64 Image (missing ARM\\x64 magic)")
        return 1
    text_offset, image_size = struct.unpack_from("<QQ", image, 8)
    if text_offset != 0:
        print(f"unexpected text_offset 0x{text_offset:x}")
        return 1
    if len(chain) > CODE_MAX:
        print(f"chain loader too large: {len(chain)} bytes")
        return 1
    if dtb[:4] != b"\xd0\x0d\xfe\xed" or len(dtb) > WORK_OFFSET - DTB_OFFSET:
        print("not a device tree, or larger than 256 KiB")
        return 1
    image_size = max(image_size, len(image))

    blob = bytearray(IMAGE_OFFSET + image_size)
    blob[0:len(chain)] = chain
    blob[DTB_OFFSET:DTB_OFFSET + len(dtb)] = dtb
    blob[IMAGE_OFFSET:IMAGE_OFFSET + len(image)] = image
    with open(sys.argv[4], "wb") as f:
        f.write(blob)
    print(f"{sys.argv[4]}: loader {len(chain)} bytes, dtb {len(dtb)} bytes, "
          f"Image {len(image)} bytes, image_size {image_size}, blob {len(blob)} bytes")
    return 0


if __name__ == "__main__":
    sys.exit(main())
