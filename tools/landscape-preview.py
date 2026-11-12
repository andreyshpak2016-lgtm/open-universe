# -*- coding: utf-8 -*-
"""
Превью рельефа ландшафта из карты высот (без окна UE).

Читает raw-файл, который записал UE (`land_heightmap.bin`: two int32 + float32),
и рисует освещённый рельеф (hillshade) в PNG — визуальная проверка того, что
ландшафт действительно имеет горы/долины, а не плоскость.

Запуск:
  python tools/landscape-preview.py [входной.bin] [выходной.png]
"""

import os
import struct
import sys

import numpy as np
from PIL import Image, ImageDraw, ImageFont

TMP = os.path.join(os.environ.get("TEMP", "."), "opencode")
SRC = sys.argv[1] if len(sys.argv) > 1 else os.path.join(TMP, "land_heightmap.bin")
OUT = sys.argv[2] if len(sys.argv) > 2 else os.path.join(TMP, "land_preview.png")


def main():
    with open(SRC, "rb") as f:
        sx, sy = struct.unpack("<ii", f.read(8))
        data = np.frombuffer(f.read(sx * sy * 4), dtype="<f4").astype(np.float64)
    h = data.reshape(sy, sx)
    print("карта: %dx%d  высоты: %.1f..%.1f см" % (sx, sy, h.min(), h.max()))

    # Нормируем и строим hillshade (свет под 45°).
    hn = (h - h.min()) / max(h.max() - h.min(), 1e-9)
    gy, gx = np.gradient(hn)
    slope = np.pi / 4
    azim = np.pi * 1.25
    shade = np.sin(slope) + np.cos(slope) * (
        np.sin(azim) * -gx * 40 + np.cos(azim) * -gy * 40)
    shade = np.clip(shade, 0, 1)

    # Гипсометрическая раскраска: вода -> низины -> горы -> снег.
    r = np.interp(hn, [0, .3, .55, .8, 1], [40, 60, 120, 150, 255])
    g = np.interp(hn, [0, .3, .55, .8, 1], [80, 140, 160, 150, 255])
    b = np.interp(hn, [0, .3, .55, .8, 1], [140, 90, 80, 130, 255])
    rgb = np.stack([r, g, b], axis=-1) * (0.45 + 0.55 * shade)[..., None]
    img = Image.fromarray(np.clip(rgb, 0, 255).astype(np.uint8))

    # Подписи.
    d = ImageDraw.Draw(img)
    text = "UE Landscape map %dx%d, heights %.0f..%.0f m" % (
        sx, sy, h.min() / 100.0, h.max() / 100.0)
    d.rectangle([0, 0, min(img.width, 700), 26], fill=(0, 0, 0))
    d.text((8, 7), text, fill=(255, 255, 255))

    img.save(OUT)
    print("превью: %s (%dx%d)" % (OUT, img.width, img.height))


if __name__ == "__main__":
    main()
