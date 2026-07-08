#!/usr/bin/env python3
"""从已有的圆角 PNG 重建完整 macOS icon.icns（含高清 512@2x）。"""
from PIL import Image
import os, subprocess, shutil

ICON_DIR = os.path.dirname(os.path.abspath(__file__))

# 基础母版：用 128x128@2x.png（256px 圆角）作为源，放大到 1024 作为母版
master = Image.open(os.path.join(ICON_DIR, "128x128@2x.png")).convert("RGBA")
master = master.resize((1024, 1024), Image.LANCZOS)

iconset = os.path.join(ICON_DIR, "temp.iconset")
os.makedirs(iconset, exist_ok=True)

sizes = {
    "icon_16x16.png": 16,
    "icon_16x16@2x.png": 32,
    "icon_32x32.png": 32,
    "icon_32x32@2x.png": 64,
    "icon_128x128.png": 128,
    "icon_128x128@2x.png": 256,
    "icon_256x256.png": 256,
    "icon_256x256@2x.png": 512,
    "icon_512x512.png": 512,
    "icon_512x512@2x.png": 1024,
}
for name, px in sizes.items():
    img = master.resize((px, px), Image.LANCZOS)
    img.save(os.path.join(iconset, name))

# 同时更新各尺寸 PNG（保持圆角）
for px in (32, 128, 256):
    master.resize((px, px), Image.LANCZOS).save(os.path.join(ICON_DIR, f"{px}x{px}.png"))
master.resize((256, 256), Image.LANCZOS).save(os.path.join(ICON_DIR, "128x128@2x.png"))

# 生成 icns
icns_path = os.path.join(ICON_DIR, "icon.icns")
if os.path.exists(icns_path):
    os.remove(icns_path)
subprocess.run(["iconutil", "--convert", "icns", "--output", icns_path, iconset], check=True)
shutil.rmtree(iconset)
print("icns regenerated:", os.path.getsize(icns_path), "bytes")
