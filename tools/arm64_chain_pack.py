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

With --lz4 the Image is packed instead (lz4 command, best ratio): a 32-byte
header ("PNLZ", image_size, packed size) and one LZ4 frame at 0x180000, no
padding. The loader unpacks it to the start of the first DRAM bank. This keeps
a boot image small enough for the 32 MiB boot partition.

Usage: arm64_chain_pack.py <chain.bin> <kernel.dtb> <Image> <out.bin> [--lz4]
"""
import struct
import subprocess
import sys

DTB_OFFSET, WORK_OFFSET, IMAGE_OFFSET = 0x100000, 0x140000, 0x180000
CODE_MAX = 0xE0000


def main() -> int:
    args = [a for a in sys.argv[1:] if a != "--lz4"]
    packed = "--lz4" in sys.argv[1:]
    if len(args) != 4:
        print(__doc__)
        return 2
    chain, dtb, image = (open(p, "rb").read() for p in args[:3])
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

    if packed:
        lz4 = subprocess.run(["lz4", "-12", "-BD", "-c", args[2]], capture_output=True, check=True).stdout
        payload = struct.pack("<IIQQ", 0x5A4C4E50, 0, image_size, len(lz4)) + lz4
        blob = bytearray(IMAGE_OFFSET + len(payload))
        blob[IMAGE_OFFSET:] = payload
    else:
        blob = bytearray(IMAGE_OFFSET + image_size)
        blob[IMAGE_OFFSET:IMAGE_OFFSET + len(image)] = image
    blob[0:len(chain)] = chain
    blob[DTB_OFFSET:DTB_OFFSET + len(dtb)] = dtb
    with open(args[3], "wb") as f:
        f.write(blob)
    print(f"{args[3]}: loader {len(chain)} bytes, dtb {len(dtb)} bytes, "
          f"Image {len(image)} bytes{' packed to ' + str(len(blob) - IMAGE_OFFSET) if packed else ''}, "
          f"image_size {image_size}, blob {len(blob)} bytes")
    return 0


if __name__ == "__main__":
    sys.exit(main())
