#!/usr/bin/env python3
"""把源图切成 Android 各密度 mipmap。"""
import os
import sys

from PIL import Image

src = sys.argv[1]
out = sys.argv[2]

# 密度目录 -> 边长
SIZES = {
    "mipmap-mdpi": 48,
    "mipmap-hdpi": 72,
    "mipmap-xhdpi": 96,
    "mipmap-xxhdpi": 144,
    "mipmap-xxxhdpi": 192,
}

im = Image.open(src).convert("RGBA")
print("源图:", im.size)

for folder, size in SIZES.items():
    d = os.path.join(out, folder)
    os.makedirs(d, exist_ok=True)
    # 用 LANCZOS 保证缩小后边缘干净
    im.resize((size, size), Image.LANCZOS).save(
        os.path.join(d, "ic_launcher.png"), "PNG", optimize=True
    )
    print("  %-18s %dx%d" % (folder, size, size))

print("完成:", out)
