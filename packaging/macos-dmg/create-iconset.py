#!/usr/bin/env python3
import os
import struct
import sys
import zlib


def write_png(path, size):
    pixels = bytearray()
    cx = (size - 1) / 2.0
    cy = (size - 1) / 2.0
    radius = size * 0.43
    ring_inner = size * 0.31
    core = size * 0.21

    for y in range(size):
        pixels.append(0)
        for x in range(size):
            dx = x - cx
            dy = y - cy
            dist = (dx * dx + dy * dy) ** 0.5
            if dist > radius:
                rgba = (0, 0, 0, 0)
            elif dist >= ring_inner:
                rgba = (45, 120, 255, 255)
            elif dist <= core:
                rgba = (22, 28, 45, 255)
            else:
                rgba = (236, 242, 255, 255)
            pixels.extend(rgba)

    def chunk(kind, data):
        return (
            struct.pack(">I", len(data))
            + kind
            + data
            + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)
        )

    raw = b"\x89PNG\r\n\x1a\n"
    raw += chunk(b"IHDR", struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0))
    raw += chunk(b"IDAT", zlib.compress(bytes(pixels), 9))
    raw += chunk(b"IEND", b"")

    with open(path, "wb") as file:
        file.write(raw)


def main():
    if len(sys.argv) != 2:
        raise SystemExit("usage: create-iconset.py <iconset-dir>")

    iconset = sys.argv[1]
    os.makedirs(iconset, exist_ok=True)
    for size in (16, 32, 128, 256, 512):
        write_png(os.path.join(iconset, f"icon_{size}x{size}.png"), size)
        write_png(os.path.join(iconset, f"icon_{size}x{size}@2x.png"), size * 2)


if __name__ == "__main__":
    main()
