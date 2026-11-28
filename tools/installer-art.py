#!/usr/bin/env python3
"""Draws the pictures the Windows installer shows: the panel beside its first and last
pages, and the mark at the top of the pages between. Needs nothing but Python.

    python3 tools/installer-art.py

The mark is the one in app/src/Logo.tsx. Both pictures are drawn at twice the size the
installer asks for, which it scales to fit, so they stay sharp on a high-density screen.
"""
import struct
from pathlib import Path

OUT = Path(__file__).resolve().parent.parent / "app" / "src-tauri" / "windows"
GROUND, WHITE, INK = (0xF1, 0xF1, 0xEE), (0xFF, 0xFF, 0xFF), (0x15, 0x17, 0x1A)
RED, YELLOW, BLUE = (0xF0, 0x47, 0x5B), (0xF2, 0xB7, 0x0A), (0x39, 0xB9, 0xDB)
SAMPLES = 4  # per pixel, each way


def rounded(x, y, w, h, r):
    def inside(px, py):
        if not (x <= px <= x + w and y <= py <= y + h):
            return False
        cx = min(max(px, x + r), x + w - r)
        cy = min(max(py, y + r), y + h - r)
        return (px - cx) ** 2 + (py - cy) ** 2 <= r * r
    return inside


def polygon(points):
    def inside(px, py):
        hit = False
        for (x1, y1), (x2, y2) in zip(points, points[1:] + points[:1]):
            if (y1 > py) != (y2 > py) and px < (x2 - x1) * (py - y1) / (y2 - y1) + x1:
                hit = not hit
        return hit
    return inside


def mark(left, top, size):
    """The Toona mark, drawn on its 16-unit grid."""
    u = size / 16
    at = lambda x, y: (left + x * u, top + y * u)
    box = lambda x, y, w, h, r: rounded(left + x * u, top + y * u, w * u, h * u, r * u)
    return [
        (box(0, 0, 16, 16, 3.6), INK),
        (box(4, 4, 6, 2, 1), RED),
        (box(2, 7, 8.75, 2, 1), YELLOW),
        (box(4, 10, 6, 2, 1), BLUE),
        (polygon([at(11.25, 7.7), at(12.75, 5), at(14.5, 5), at(13.3, 7.7)]), GROUND),
        (polygon([at(11.25, 8.3), at(12.75, 11), at(14.5, 11), at(13.3, 8.3)]), GROUND),
    ]


def draw(name, width, height, ground, shapes):
    rows = []
    for y in range(height):
        row = bytearray()
        for x in range(width):
            total = [0, 0, 0]
            for sy in range(SAMPLES):
                for sx in range(SAMPLES):
                    px, py = x + (sx + 0.5) / SAMPLES, y + (sy + 0.5) / SAMPLES
                    colour = ground
                    for inside, fill in shapes:
                        if inside(px, py):
                            colour = fill
                    for channel in range(3):
                        total[channel] += colour[channel]
            r, g, b = (round(value / SAMPLES**2) for value in total)
            row += bytes((b, g, r))
        row += bytes(-len(row) % 4)
        rows.append(bytes(row))
    pixels = b"".join(reversed(rows))  # a bitmap's rows run bottom to top
    header = struct.pack("<2sIHHI", b"BM", 54 + len(pixels), 0, 0, 54)
    info = struct.pack("<IiiHHIIiiII", 40, width, height, 1, 24, 0, len(pixels), 2835, 2835, 0, 0)
    OUT.mkdir(parents=True, exist_ok=True)
    (OUT / name).write_bytes(header + info + pixels)
    print(f"{name}: {width} x {height}")


# Beside the first and last pages: 164 x 314 as the installer measures it.
bars = [(rounded(84 + i * 56, 548, 48, 12, 6), colour) for i, colour in enumerate((RED, YELLOW, BLUE))]
draw("installer-sidebar.bmp", 328, 628, GROUND, mark(76, 120, 176) + bars)
# At the top right of the pages between: 150 x 57.
draw("installer-header.bmp", 300, 114, WHITE, mark(206, 19, 76))
