#!/usr/bin/env python3
"""
从 256x256 源 PNG 重新生成完整的多平台图标集（v0.6.2-beta.7）。

修复：
- .icns 之前只含 ic12（1024 单图）→ 缺 16/32/64/128/256/512 尺寸，macOS Dock 会渲染成
  「大一圈」（找不到合适中间尺寸，回退到 1024 拉满）。现生成完整 iconset 后用 iconutil
  合成多分辨率 .icns，所有尺寸都有。
- .ico 之前只含一个 16x16 PNG (594B) → Windows 任何大于 16px 的位置都模糊。
  现嵌入 16/24/32/48/64/128/256 多分辨率，Windows 资源管理器/Dock/任务栏全清晰。

源：128x128@2x.png (实际是 256x256 RGBA，含圆角版本)
输出：
  - 32x32.png / 128x128.png / 128x128@2x.png (256) / 256x256.png
  - icon.icns (macOS 多分辨率)
  - icon.ico  (Windows 多分辨率)

用法：
  /Users/lkshao/.workbuddy/binaries/python/envs/default/bin/python3 regen_all_icons.py
"""
from PIL import Image
import os
import subprocess
import shutil

ICON_DIR = os.path.dirname(os.path.abspath(__file__))
SOURCE = os.path.join(ICON_DIR, "128x128@2x.png")  # 256x256 RGBA, contains rounded design

# macOS iconset 完整尺寸（含 @2x retina）
ICNS_SIZES = {
    "icon_16x16.png": 16,
    "icon_16x16@2x.png": 32,
    "icon_32x32.png": 32,
    "icon_32x32@2x.png": 64,
    "icon_64x64.png": 64,
    "icon_128x128.png": 128,
    "icon_128x128@2x.png": 256,
    "icon_256x256.png": 256,
    "icon_256x256@2x.png": 512,
    "icon_512x512.png": 512,
    "icon_512x512@2x.png": 1024,
}

# Windows ICO 多分辨率（Tauri/Windows shell 期望的尺寸）
ICO_SIZES = [(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)]


def main():
    if not os.path.exists(SOURCE):
        raise SystemExit(f"找不到源文件 {SOURCE}")

    # 1) 读源（256x256）→ 升到 1024 master
    src = Image.open(SOURCE).convert("RGBA")
    master = src.resize((1024, 1024), Image.LANCZOS)
    print(f"[source] {SOURCE} 256x256 → master 1024x1024")

    # 2) 写出 Tauri 期望的标准 PNG 文件名（tauri.conf.json 里写了）
    for fname, px in [("32x32.png", 32), ("128x128.png", 128),
                      ("128x128@2x.png", 256), ("256x256.png", 256)]:
        out = os.path.join(ICON_DIR, fname)
        master.resize((px, px), Image.LANCZOS).save(out)
        print(f"  ✓ {fname} ({px}x{px}) {os.path.getsize(out)}B")

    # 3) 生成 macOS iconset 临时目录 → iconutil 合成 .icns
    iconset = os.path.join(ICON_DIR, "temp.iconset")
    if os.path.exists(iconset):
        shutil.rmtree(iconset)
    os.makedirs(iconset)
    for name, px in ICNS_SIZES.items():
        path = os.path.join(iconset, name)
        master.resize((px, px), Image.LANCZOS).save(path)
    icns_out = os.path.join(ICON_DIR, "icon.icns")
    if os.path.exists(icns_out):
        os.remove(icns_out)
    subprocess.run(["iconutil", "--convert", "icns", "--output", icns_out, iconset],
                   check=True)
    shutil.rmtree(iconset)
    print(f"  ✓ icon.icns ({os.path.getsize(icns_out)}B) — {len(ICNS_SIZES)} sizes")

    # 4) 生成 Windows 多分辨率 .ico
    ico_out = os.path.join(ICON_DIR, "icon.ico")
    master.save(ico_out, format="ICO", sizes=ICO_SIZES)
    print(f"  ✓ icon.ico ({os.path.getsize(ico_out)}B) — sizes {ICO_SIZES}")


if __name__ == "__main__":
    main()