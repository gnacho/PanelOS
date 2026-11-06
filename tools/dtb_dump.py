#!/usr/bin/env python3
"""Dump a flattened device tree blob (.dtb) as readable DTS-like text.

No dependencies. Strings are shown as strings, everything else as 32-bit
cells (or bytes when the length is not a multiple of 4). phandle references
are not resolved.

Usage: dtb_dump.py <file.dtb> [output.txt]
"""
import struct
import sys

FDT_MAGIC = 0xD00DFEED
FDT_BEGIN_NODE, FDT_END_NODE, FDT_PROP, FDT_NOP, FDT_END = 1, 2, 3, 4, 9


def is_printable_strings(value: bytes) -> bool:
    if not value or value[-1] != 0:
        return False
    parts = value[:-1].split(b"\0")
    return all(p and all(32 <= c < 127 for c in p) for p in parts)


def format_value(value: bytes) -> str:
    if not value:
        return ""
    if is_printable_strings(value):
        return " = " + ", ".join('"%s"' % p.decode() for p in value[:-1].split(b"\0")) + ";"
    if len(value) % 4 == 0:
        cells = struct.unpack(">%dI" % (len(value) // 4), value)
        return " = <" + " ".join("0x%x" % c for c in cells) + ">;"
    return " = [" + " ".join("%02x" % b for b in value) + "];"


def dump(blob: bytes) -> str:
    magic, _total, off_struct, off_strings = struct.unpack_from(">IIII", blob, 0)
    if magic != FDT_MAGIC:
        raise ValueError("not a DTB (bad magic)")
    out, depth, pos = [], 0, off_struct
    while True:
        (token,) = struct.unpack_from(">I", blob, pos)
        pos += 4
        if token == FDT_BEGIN_NODE:
            end = blob.index(b"\0", pos)
            name = blob[pos:end].decode() or "/"
            pos = (end + 4) & ~3
            out.append("  " * depth + name + " {")
            depth += 1
        elif token == FDT_END_NODE:
            depth -= 1
            out.append("  " * depth + "};")
        elif token == FDT_PROP:
            length, name_off = struct.unpack_from(">II", blob, pos)
            pos += 8
            value = blob[pos:pos + length]
            pos = (pos + length + 3) & ~3
            name_end = blob.index(b"\0", off_strings + name_off)
            name = blob[off_strings + name_off:name_end].decode()
            out.append("  " * depth + name + format_value(value))
        elif token == FDT_NOP:
            continue
        elif token == FDT_END:
            break
        else:
            raise ValueError("unknown token 0x%x at offset %d" % (token, pos - 4))
    return "\n".join(out) + "\n"


def main() -> int:
    if len(sys.argv) not in (2, 3):
        print(__doc__)
        return 2
    with open(sys.argv[1], "rb") as f:
        text = dump(f.read())
    if len(sys.argv) == 3:
        with open(sys.argv[2], "w", encoding="utf-8") as f:
            f.write(text)
    else:
        sys.stdout.write(text)
    return 0


if __name__ == "__main__":
    sys.exit(main())
