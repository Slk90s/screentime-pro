#!/usr/bin/env python3
"""
重建 macOS 专用图标（icon.icns），修正 Dock 里「比其他应用大一圈」。

问题：v1.2.1 及更早的 icon.icns 中，图形内容 100% 铺满 1024 画布、四周零留白；
而 macOS Big Sur 之后的图标规范要求图形只占画布约 824/1024（80.5%），
四周留出约 100px 透明边距。铺满时 Dock/Finder 中的视觉尺寸比系统图标大约 24%，
表现为「大一圈」。

方案：不整体缩放（会破坏 Windows/Linux 与托盘共用的 PNG），只重建 icon.icns。
用 4x 超采样重绘原图形（纯色圆角方块 + 白色圆环 + 白色圆点），几何参数取自
256px 源图实测：圆角半径 ≈ 58/256（22.7%），环外径 ≈ 152/256，环宽 ≈ 9/256，
点心直径 ≈ 50/256，纯色 #FF7E27。

输出：重写 src-tauri/icons/icon.icns，覆盖 16..1024 全尺寸（icp4/icp5/ic07/
ic08/ic09/ic10/ic11/ic12/ic13/ic14），保持 .icns 结构合法。

用法（任意平台，无需 iconutil）：
  python regen_icns_macos.py
"""
import os
import struct
import math

from PIL import Image, ImageDraw

ICON_DIR = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(ICON_DIR, "icon.icns")

ORANGE = (255, 126, 39, 255)   # 实测源图纯色
WHITE = (255, 255, 255, 255)

# 相对画布的几何比例（取自 256px 源图实测，均以「图形边长」为基准）
# 源图中图形铺满 256，故这些即 256 下的像素 ÷ 256
SQUARE_RATIO = 58 / 256        # 圆角半径
RING_OUTER_RATIO = (203 - 52) / 2 / 256   # 环外径半径 ≈ 75.5/256
RING_WIDTH_RATIO = 9 / 256     # 环宽
DOT_RATIO = 25 / 256           # 点心半径（直径 50/256）

# macOS 规范：图形占画布 824/1024
CONTENT_RATIO = 824 / 1024

SUPERSAMPLE = 4                # 超采样倍数，抗锯齿

# .icns 尺寸表：chunk 类型 -> 像素边长
ICNS_CHUNKS = [
    ("icp4", 16),
    ("icp5", 32),
    ("ic11", 32),    # 16@2x
    ("ic12", 64),    # 32@2x
    ("ic07", 128),
    ("ic13", 256),   # 128@2x
    ("ic08", 256),
    ("ic14", 512),   # 256@2x
    ("ic09", 512),
    ("ic10", 1024),
]


def render(size: int) -> Image.Image:
    """渲染单张图标：图形占画布 CONTENT_RATIO，透明背景。"""
    s = size * SUPERSAMPLE
    canvas = Image.new("RGBA", (s, s), (0, 0, 0, 0))
    d = ImageDraw.Draw(canvas)

    # 图形（圆角方块）边长与左上角
    side = s * CONTENT_RATIO
    off = (s - side) / 2
    radius = side * SQUARE_RATIO

    d.rounded_rectangle(
        [off, off, off + side - 1, off + side - 1],
        radius=radius,
        fill=ORANGE,
    )

    cx = cy = s / 2

    # 白色圆环 = 外圆填充白 + 内圆填橙色
    r_out = side * RING_OUTER_RATIO
    r_in = r_out - side * RING_WIDTH_RATIO
    d.ellipse([cx - r_out, cy - r_out, cx + r_out, cy + r_out], fill=WHITE)
    d.ellipse([cx - r_in, cy - r_in, cx + r_in, cy + r_in], fill=ORANGE)

    # 白色圆点
    r_dot = side * DOT_RATIO
    d.ellipse([cx - r_dot, cy - r_dot, cx + r_dot, cy + r_dot], fill=WHITE)

    return canvas.resize((size, size), Image.LANCZOS)


def build_icns(path: str) -> None:
    chunks = []
    for ctype, px in ICNS_CHUNKS:
        img = render(px)
        buf = img.tobytes()  # RGBA
        # icns 的 PNG 容器：直接用 PNG 编码更稳（macOS 支持 PNG 压缩的 ic* 块）
        import io
        bio = io.BytesIO()
        img.save(bio, format="PNG", optimize=True)
        payload = bio.getvalue()
        chunks.append((ctype, payload))
        print(f"  {ctype} {px}x{px}: {len(payload)}B")

    body = b""
    for ctype, payload in chunks:
        body += ctype.encode("latin-1")
        body += struct.pack(">I", len(payload) + 8)
        body += payload

    total = 8 + len(body)
    with open(path, "wb") as f:
        f.write(b"icns")
        f.write(struct.pack(">I", total))
        f.write(body)
    print(f"✓ {path} ({total}B, {len(chunks)} chunks)")


def main():
    print("重建 macOS 图标：图形占画布 %.1f%%，留白 %.1f%%"
          % (CONTENT_RATIO * 100, (1 - CONTENT_RATIO) * 100))
    build_icns(OUT)


if __name__ == "__main__":
    main()
