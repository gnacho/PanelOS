#!/usr/bin/env python3
"""Create an Android boot image, header version 0, as the stock firmware uses.

Same layout and SHA-1 "id" field as AOSP mkbootimg, without its dependencies.
Defaults match the stock NSPanel Pro boot/recovery images.

Usage:
  mkbootimg_v0.py --kernel Image --ramdisk initrd.gz --second resource.img \
                  --cmdline "..." -o out.img
"""
import argparse
import hashlib
import struct
import sys


def pad(data: bytes, page: int) -> bytes:
    return data + b"\0" * (-len(data) % page)


def os_version(version: str, patch: str) -> int:
    a, b, c = (int(x) for x in (version.split(".") + ["0", "0"])[:3])
    year, month = (int(x) for x in patch.split("-")[:2])
    return (((a << 14) | (b << 7) | c) << 11) | (((year - 2000) << 4) | month)


def main() -> int:
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--kernel", required=True)
    p.add_argument("--ramdisk")
    p.add_argument("--second")
    p.add_argument("--cmdline", default="")
    p.add_argument("--base", type=lambda x: int(x, 0), default=0x10000000)
    p.add_argument("--kernel-offset", type=lambda x: int(x, 0), default=0x00008000)
    p.add_argument("--ramdisk-offset", type=lambda x: int(x, 0), default=0x01000000)
    p.add_argument("--second-offset", type=lambda x: int(x, 0), default=0x00f00000)
    p.add_argument("--tags-offset", type=lambda x: int(x, 0), default=0x00000100)
    p.add_argument("--pagesize", type=int, default=2048)
    p.add_argument("--os-version", default="8.1.0")
    p.add_argument("--os-patch-level", default="2019-07")
    p.add_argument("-o", "--output", required=True)
    a = p.parse_args()

    def read(path):
        if not path:
            return b""
        with open(path, "rb") as f:
            return f.read()

    kernel, ramdisk, second = read(a.kernel), read(a.ramdisk), read(a.second)
    cmdline = a.cmdline.encode()
    if len(cmdline) > 512 + 1024 - 1:
        raise ValueError("command line too long")
    main_cmd, extra_cmd = cmdline[:512], cmdline[512:]
    if len(main_cmd) == 512:
        main_cmd, extra_cmd = cmdline[:511], cmdline[511:]

    sha = hashlib.sha1()
    for blob in (kernel, ramdisk, second):
        sha.update(blob)
        sha.update(struct.pack("<I", len(blob)))
    img_id = sha.digest() + b"\0" * 12  # 32 bytes

    header = struct.pack(
        "<8s10I16s512s32s1024s",
        b"ANDROID!",
        len(kernel), a.base + a.kernel_offset,
        len(ramdisk), a.base + a.ramdisk_offset,
        len(second), a.base + a.second_offset,
        a.base + a.tags_offset,
        a.pagesize,
        0,  # header version
        os_version(a.os_version, a.os_patch_level),
        b"",
        main_cmd,
        img_id,
        extra_cmd,
    )

    with open(a.output, "wb") as f:
        f.write(pad(header, a.pagesize))
        f.write(pad(kernel, a.pagesize))
        if ramdisk:
            f.write(pad(ramdisk, a.pagesize))
        if second:
            f.write(pad(second, a.pagesize))
    print(f"{a.output}: kernel {len(kernel)}, ramdisk {len(ramdisk)}, second {len(second)}, id {sha.hexdigest()}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
