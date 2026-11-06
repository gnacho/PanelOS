#!/usr/bin/env python3
"""Build a Rockchip resource image (RSCE format), e.g. to carry the kernel DTB.

The vendor U-Boot reads the device tree ("rk-kernel.dtb") from the resource
image stored as the "second" stage of an Android boot image.

Usage: pack_rk_resource.py <output.img> <name>=<file> [<name>=<file> ...]
Example: pack_rk_resource.py resource.img rk-kernel.dtb=board.dtb
"""
import struct
import sys

BLOCK = 512


def main() -> int:
    if len(sys.argv) < 3:
        print(__doc__)
        return 2
    out_path = sys.argv[1]
    entries = []
    for arg in sys.argv[2:]:
        name, path = arg.split("=", 1)
        with open(path, "rb") as f:
            entries.append((name, f.read()))

    header_blocks = 1
    table_offset = 1  # blocks
    entry_blocks = 1
    data_block = table_offset + len(entries) * entry_blocks

    header = bytearray(BLOCK)
    header[0:4] = b"RSCE"
    struct.pack_into("<HHBBB", header, 4, 0, 0, header_blocks, table_offset, entry_blocks)
    struct.pack_into("<I", header, 12, len(entries))

    table = bytearray()
    payload = bytearray()
    for name, content in entries:
        entry = bytearray(BLOCK)
        entry[0:4] = b"ENTR"
        encoded = name.encode()
        if len(encoded) > 255:
            raise ValueError("entry name too long: " + name)
        entry[4:4 + len(encoded)] = encoded
        struct.pack_into("<II", entry, 260, data_block, len(content))
        table += entry
        padded = content + b"\0" * (-len(content) % BLOCK)
        payload += padded
        data_block += len(padded) // BLOCK

    with open(out_path, "wb") as f:
        f.write(header + table + payload)
    print(f"{out_path}: {len(entries)} entries, {len(header) + len(table) + len(payload)} bytes")
    return 0


if __name__ == "__main__":
    sys.exit(main())
