#!/usr/bin/env python3
"""Generate the ModelView app icon (a synthetic mark, no third-party imagery)."""

import os
import struct
import zlib

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
OUT = os.path.join(ROOT, "crates", "modelview-app", "assets", "icon.png")
SIZE = 512

BONE = (0xC9, 0xC4, 0xB8)
SODIUM = (0xFA, 0xCC, 0x15)


def mix(a, b, t):
    return tuple(int(round(a[i] + (b[i] - a[i]) * t)) for i in range(3))


def make_pixels():
    top = (0x0B, 0x0D, 0x0E)
    bottom = (0x1B, 0x20, 0x22)
    px = [[(0, 0, 0, 255) for _ in range(SIZE)] for _ in range(SIZE)]
    for y in range(SIZE):
        color = mix(top, bottom, y / (SIZE - 1))
        for x in range(SIZE):
            px[y][x] = (*color, 255)

    def fill_rect(x0, y0, x1, y1, color):
        for y in range(max(0, y0), min(SIZE, y1)):
            for x in range(max(0, x0), min(SIZE, x1)):
                px[y][x] = (*color, 255)

    inset = 96
    thickness = 7
    # Square keyline.
    fill_rect(inset, inset, SIZE - inset, inset + thickness, BONE)
    fill_rect(inset, SIZE - inset - thickness, SIZE - inset, SIZE - inset, BONE)
    fill_rect(inset, inset, inset + thickness, SIZE - inset, BONE)
    fill_rect(SIZE - inset - thickness, inset, SIZE - inset, SIZE - inset, BONE)
    # Vertical spine, longer than the square.
    fill_rect(SIZE // 2 - thickness // 2, 32, SIZE // 2 + thickness // 2 + 1, SIZE - 32, BONE)
    # Sodium tick off the spine.
    fill_rect(SIZE // 2 + 1, 372, SIZE - 110, 372 + thickness, SODIUM)
    return px


def write_png(path, px):
    raw = bytearray()
    for row in px:
        raw.append(0)
        for r, g, b, a in row:
            raw.extend((r, g, b, a))
    compressed = zlib.compress(bytes(raw), 9)

    def chunk(kind, data):
        return (
            struct.pack(">I", len(data))
            + kind
            + data
            + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)
        )

    png = b"\x89PNG\r\n\x1a\n"
    png += chunk(b"IHDR", struct.pack(">IIBBBBB", SIZE, SIZE, 8, 6, 0, 0, 0))
    png += chunk(b"IDAT", compressed)
    png += chunk(b"IEND", b"")
    with open(path, "wb") as f:
        f.write(png)


if __name__ == "__main__":
    os.makedirs(os.path.dirname(OUT), exist_ok=True)
    write_png(OUT, make_pixels())
    print(f"icon written: {OUT} ({SIZE}x{SIZE})")
