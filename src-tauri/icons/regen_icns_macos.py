#!/usr/bin/env python3
"""
重建 macOS 专用图标（icon.icns），修正 macOS 设置面板里图标显示为彩色噪点的问题。

问题（P0，v1.2.2 实测）：macOS「系统设置 → 录屏与系统录音」列表中 ScreenTime Pro 的
图标显示为一团绿/洋红色噪点，无法辨认。Dock / Finder 里的图标则正常。

根因：v1.2.1 引入的 v1 版脚本把 16px / 32px 两个小尺寸写成了
`icp4` / `icp5` 段 + **PNG 载荷**。而 Apple 的 icns 格式里：

  - `icp4` (16×16) / `icp5` (32×32) 是**老式键**，其载荷规范是 24-bit RGB（或
    24-bit RGB + 8-bit 掩码 `s8mk` / `l8mk`），**不是 PNG**；
  - 只有 `ic07`~`ic14` 这类新式键才允许内嵌 PNG / JPEG2000。

Apple 已公开记录该缺陷：macOS 10.15.7 及之后的版本，单独打开 .icns 文件时
`icp4`/`icp5` 里的 PNG 尚可正常显示，但**一旦该 .icns 被用作 App Bundle 的图标**，
系统就会按 24-bit RGB 去解析那段 PNG 压缩流，把压缩字节当成像素值，
于是渲染出一团彩色噪点——正好就是本项目在设置面板里看到的现象。

修法：16px / 32px 改用现代键 `ic04` / `ic05`，其载荷为 **ARGB** 格式
（`ARGB` 魔数 + A/R/G/B 四通道各自 PackBits RLE 压缩）；
大尺寸继续沿用 `ic07`~`ic14` + PNG（本来就是对的，无需改）。
本脚本内实现的 ARGB 编解码器已与 Apple `iconutil` 的产出做过**字节级等价验证**
（同一张图：iconutil 569B ↔ 本脚本 569B、iconutil 1514B ↔ 本脚本 1514B，
逐字节完全一致）。

同时保留 v1.2.1 的留白修复（图形占画布约 824/1024，符合 Big Sur 之后的
macOS 图标规范），因此 Dock「比其他应用大一圈」的问题不会回退。

输出：重写 src-tauri/icons/icon.icns
  - ic04  16×16  ARGB
  - ic05  32×32  ARGB
  - ic11  32×32  PNG（16@2x）
  - ic12  64×64  PNG（32@2x）
  - ic07  128×128 PNG
  - ic13  256×256 PNG（128@2x）
  - ic08  256×256 PNG
  - ic14  512×512 PNG（256@2x）
  - ic09  512×512 PNG
  - ic10  1024×1024 PNG（512@2x）
  - info  310B   Icon Composer 元数据（与 iconutil 产出逐字节一致）

用法（任意平台，无需 iconutil）：
  python regen_icns_macos.py
"""
import base64
import io
import os
import struct

from PIL import Image, ImageDraw

ICON_DIR = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(ICON_DIR, "icon.icns")

ORANGE = (255, 126, 39, 255)   # 实测源图纯色
WHITE = (255, 255, 255, 255)

# 相对画布的几何比例（取自 256px 源图实测，均以「图形边长」为基准）
SQUARE_RATIO = 58 / 256        # 圆角半径
RING_OUTER_RATIO = (203 - 52) / 2 / 256   # 环外径半径 ≈ 75.5/256
RING_WIDTH_RATIO = 9 / 256     # 环宽
DOT_RATIO = 25 / 256           # 点心半径（直径 50/256）

# macOS 规范：图形占画布 824/1024
CONTENT_RATIO = 824 / 1024

SUPERSAMPLE = 4                # 超采样倍数，抗锯齿

# info 段：Icon Composer 写入的元数据（NSKeyedArchiver 归档的 NSKeyedArchiver plist）。
# 与 iconutil 产出逐字节一致，直接内嵌以保证各尺寸表现完全相同。
INFO_B64 = (
    "YnBsaXN0MDDUAQIDBAUGBwpYJHZlcnNpb25ZJGFyY2hpdmVyVCR0b3BYJG9iamVjdHMSAAGGoF8QD05TS2V5"
    "ZWRBcmNoaXZlctEICVRyb290gAGnCwwXGBkaHlUkbnVsbNMNDg8QExZXTlMua2V5c1pOUy5vYmplY3RzViRjbGFz"
    "c6IREoACgAOiFBWABIAFgAZUbmFtZV8QFmFzc2V0Y2F0YWxvZy1yZWZlcmVuY2VUaWNvbtMNDg8bHBagoIAG0"
    "h8gISJaJGNsYXNzbmFtZVgkY2xhc3Nlc1xOU0RpY3Rpb25hcnmiISNYTlNPYmplY3QIERokKTI3SUxTU1t"
    "haHB7goWHiYyOkJKXsLW8vb7AxdDZ5ukAAAAAAAABAQAAAAAAAAAkAAAAAAAAAAAAAAAAAAAA8g=="
)


# --------------------------------------------------------------------------
# 渲染
# --------------------------------------------------------------------------

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


# --------------------------------------------------------------------------
# ARGB 编解码（ic04 / ic05 载荷格式）
# --------------------------------------------------------------------------

def rle_decode(data: bytes, want: int) -> tuple[bytes, int]:
    """PackBits 变体（Apple 专用）：byte&0x80 → 重复 (byte-125) 次；否则字面 (byte+1) 字节。"""
    out = bytearray()
    i = 0
    n = len(data)
    while len(out) < want and i < n:
        c = data[i]
        i += 1
        if c & 0x80:
            out += bytes([data[i]]) * (c - 125)
            i += 1
        else:
            cnt = c + 1
            out += data[i:i + cnt]
            i += cnt
    return bytes(out), i


def rle_encode(band: bytes) -> bytes:
    """rle_decode 的逆：与 iconutil 产出逐字节一致。"""
    out = bytearray()
    i = 0
    n = len(band)
    while i < n:
        run = 1
        while i + run < n and band[i + run] == band[i] and run < 130:
            run += 1
        if run >= 3:
            out.append(run + 125)
            out.append(band[i])
            i += run
        else:
            j = i
            lit = bytearray()
            while j < n and len(lit) < 128:
                r = 1
                while j + r < n and band[j + r] == band[j] and r < 3:
                    r += 1
                if r >= 3:
                    break
                lit.append(band[j])
                j += 1
            out.append(len(lit) - 1)
            out += lit
            i = j
    return bytes(out)


def encode_argb(img: Image.Image) -> bytes:
    """RGBA 图 → ARGB 载荷：'ARGB' + A/R/G/B 四通道各自 RLE 压缩。"""
    r, g, b, a = img.split()
    return (b"ARGB"
            + rle_encode(a.tobytes())
            + rle_encode(r.tobytes())
            + rle_encode(g.tobytes())
            + rle_encode(b.tobytes()))


def decode_argb(payload: bytes, size: int) -> Image.Image:
    """ARGB 载荷 → RGBA 图（供自校验使用）。"""
    if payload[:4] != b"ARGB":
        raise ValueError("ARGB 魔数不匹配")
    body = payload[4:]
    bands = []
    pos = 0
    for _ in range(4):
        band, used = rle_decode(body[pos:], size * size)
        if len(band) != size * size:
            raise ValueError(f"ARGB 通道长度异常：{len(band)} != {size * size}")
        bands.append(band)
        pos += used
    a, r, g, b = bands
    px = bytearray()
    for i in range(size * size):
        px += bytes([r[i], g[i], b[i], a[i]])
    return Image.frombytes("RGBA", (size, size), bytes(px))


# --------------------------------------------------------------------------
# icns 容器
# --------------------------------------------------------------------------

def png_payload(img: Image.Image) -> bytes:
    bio = io.BytesIO()
    img.save(bio, format="PNG", optimize=True)
    return bio.getvalue()


def build_icns(path: str) -> None:
    # (段类型, 像素边长, 载荷格式)
    #   ic04/ic05 必须用 ARGB；ic07~ic14 用 PNG。
    #   故意不产出 icp4/icp5 —— 那两个老式键在 App Bundle 里会被按 24-bit RGB
    #   解析，导致 PNG 压缩流被当像素渲染成彩色噪点。
    chunks = [
        ("ic04", 16, "argb"),   # 16×16  @1x
        ("ic05", 32, "argb"),   # 32×32  @1x
        ("ic11", 32, "png"),    # 16×16  @2x
        ("ic12", 64, "png"),    # 32×32  @2x
        ("ic07", 128, "png"),   # 128×128 @1x
        ("ic13", 256, "png"),   # 128×128 @2x
        ("ic08", 256, "png"),   # 256×256 @1x
        ("ic14", 512, "png"),   # 256×256 @2x
        ("ic09", 512, "png"),   # 512×512 @1x
        ("ic10", 1024, "png"),  # 512×512 @2x
    ]

    body = b""
    for ctype, px, fmt in chunks:
        img = render(px)
        if fmt == "argb":
            payload = encode_argb(img)
            # 自校验：解码回来必须与原图逐像素一致
            back = decode_argb(payload, px)
            if list(back.getdata()) != list(img.getdata()):
                raise AssertionError(f"{ctype} ARGB 编解码往返不一致")
        else:
            payload = png_payload(img)
        body += ctype.encode("latin-1")
        body += struct.pack(">I", len(payload) + 8)
        body += payload
        print(f"  {ctype:5s} {px:4d}px {fmt:4s} {len(payload):7d}B")

    info = base64.b64decode(INFO_B64)
    body += b"info"
    body += struct.pack(">I", len(info) + 8)
    body += info
    print(f"  {'info':5s} {'':4s} {'':4s} {len(info):7d}B")

    total = 8 + len(body)
    with open(path, "wb") as f:
        f.write(b"icns")
        f.write(struct.pack(">I", total))
        f.write(body)
    print(f"✓ {path} ({total}B, {len(chunks) + 1} chunks)")


def verify(path: str) -> None:
    """回读自校验：结构、段格式、像素一致性。"""
    d = open(path, "rb").read()
    assert d[:4] == b"icns", "魔数错误"
    assert struct.unpack(">I", d[4:8])[0] == len(d), "声明长度与实际长度不符"

    off = 8
    seen = {}
    while off < len(d):
        ctype = d[off:off + 4].decode("latin-1")
        length = struct.unpack(">I", d[off + 4:off + 8])[0]
        assert length > 8, f"{ctype} 段长度非法"
        payload = d[off + 8:off + length]
        seen[ctype] = payload
        off += length
    assert off == len(d), "段数据未铺满整个文件"

    # 关键断言：不能再出现 icp4 / icp5
    for legacy in ("icp4", "icp5", "icp6"):
        assert legacy not in seen, f"仍存在会导致噪点的老式段 {legacy}"

    # ic04 / ic05 必须是 ARGB，且解码像素与重新渲染的结果一致
    for ctype, px in (("ic04", 16), ("ic05", 32)):
        p = seen[ctype]
        assert p[:4] == b"ARGB", f"{ctype} 载荷不是 ARGB"
        back = decode_argb(p, px)
        ref = render(px)
        assert list(back.getdata()) == list(ref.getdata()), f"{ctype} 像素不一致"

    # 大尺寸必须是 PNG
    for ctype in ("ic07", "ic08", "ic09", "ic10", "ic11", "ic12", "ic13", "ic14"):
        p = seen[ctype]
        assert p[:8] == b"\x89PNG\r\n\x1a\n", f"{ctype} 载荷不是 PNG"

    print(f"✓ 自校验通过：{len(seen)} 段，无 icp4/icp5，ARGB 与 PNG 载荷均正确")


def main():
    print("重建 macOS 图标：图形占画布 %.1f%%，留白 %.1f%%"
          % (CONTENT_RATIO * 100, (1 - CONTENT_RATIO) * 100))
    build_icns(OUT)
    verify(OUT)


if __name__ == "__main__":
    main()
