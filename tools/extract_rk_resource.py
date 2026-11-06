#!/usr/bin/env python3
"""Extract all files from a Rockchip resource image (RSCE format).

The resource partition of Rockchip Android firmware holds the kernel device
tree (usually rk-kernel.dtb) and boot logos.

Usage: extract_rk_resource.py <resource.img> <output-dir>
"""
import os
import struct
import sys

BLOCK = 512


def main() -> int:
    if len(sys.argv) != 3:
        print(__doc__)
        return 2
    image_path, out_dir = sys.argv[1], sys.argv[2]
    with open(image_path, "rb") as f:
        data = f.read()

    if data[:4] != b"RSCE":
        print("not a Rockchip resource image (missing RSCE magic)")
        return 1

    # Header: magic, u16 ptn version, u16 table version, u8 header blocks,
    # u8 table offset (blocks), u8 entry size (blocks), pad, u32 entry count at 12.
    _ptn_ver, _tbl_ver, _hdr_blks, tbl_offset, entry_blks = struct.unpack_from("<HHBBB", data, 4)
    (entry_num,) = struct.unpack_from("<I", data, 12)
    os.makedirs(out_dir, exist_ok=True)

    for i in range(entry_num):
        base = (tbl_offset + i * entry_blks) * BLOCK
        if data[base:base + 4] != b"ENTR":
            print(f"entry {i}: bad tag, stopping")
            break
        name = data[base + 4:base + 260].split(b"\0", 1)[0].decode("utf-8", "replace")
        offset_blks, size = struct.unpack_from("<II", data, base + 260)
        content = data[offset_blks * BLOCK:offset_blks * BLOCK + size]
        safe_name = name.replace("/", "_").replace("\\", "_") or f"entry_{i}"
        with open(os.path.join(out_dir, safe_name), "wb") as out:
            out.write(content)
        print(f"{safe_name}: {size} bytes")
    return 0


if __name__ == "__main__":
    sys.exit(main())
