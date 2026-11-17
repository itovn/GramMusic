#!/usr/bin/env python3
"""Ảnh khởi động cho app (Python thuần, không cần thư viện ảnh) → app/Resources/: nền một màu như nền app.
Biểu tượng vẽ bằng tools/icon/make_icon.py (Pillow).
    python3 tools/make-images.py
"""
import math
import os
import struct
import zlib

OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "app", "Resources")


def png(path, w, h, pixel):
    rows = bytearray()
    for y in range(h):
        rows.append(0)
        for x in range(w):
            rows.extend(pixel(x, y))
    def chunk(t, d):
        return struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d) & 0xFFFFFFFF)
    data = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0))
    data += chunk(b"IDAT", zlib.compress(bytes(rows), 9)) + chunk(b"IEND", b"")
    with open(path, "wb") as f:
        f.write(data)


def mix(a, b, t):
    return tuple(int(round(a[i] * (1 - t) + b[i] * t)) for i in range(3))


def solid(rgb):
    b = bytes(rgb)
    return lambda x, y: b


def main():
    if hasattr(__import__("sys").stdout, "reconfigure"):
        __import__("sys").stdout.reconfigure(encoding="utf-8")
    os.makedirs(OUT, exist_ok=True)
    bg = (226, 232, 238)
    for name, w, h in [("Default.png", 320, 480), ("Default@2x.png", 640, 960), ("Default-568h@2x.png", 640, 1136),
                       ("Default-Portrait~ipad.png", 768, 1004), ("Default-Landscape~ipad.png", 1024, 748)]:
        png(os.path.join(OUT, name), w, h, solid(bg))
    print("đã tạo ảnh trong", os.path.normpath(OUT))


if __name__ == "__main__":
    main()
