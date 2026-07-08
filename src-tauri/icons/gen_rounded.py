#!/usr/bin/env python3
"""
Generate rounded-corner icons for ScreenTime Pro.

Takes the existing square orange icon (128x128@2x.png = 256px),
applies macOS Big Sur-style superellipse rounding, then generates
all required sizes for Tauri bundling.

Output overwrites src-tauri/icons/ with rounded versions.
"""

from PIL import Image, ImageDraw
import subprocess
import os
import struct

ICONS_DIR = os.path.dirname(os.path.abspath(__file__))
SOURCE = os.path.join(ICONS_DIR, "128x128@2x.png")  # 256px source

# Corner radius as fraction of icon size (macOS Big Sur ~22% continuous corner)
CORNER_RATIO = 0.22


def create_rounded_mask(size: int, radius: int) -> Image.Image:
    """Create an alpha mask with superellipse-rounded corners."""
    mask = Image.new("L", (size, size), 0)
    draw = ImageDraw.Draw(mask)

    # Draw rounded rectangle filled white
    r = max(1, radius)
    draw.rounded_rectangle(
        [0, 0, size - 1, size - 1],
        radius=r,
        fill=255,
    )
    return mask


def make_rounded(source_path: str, output_size: int) -> Image.Image:
    """Load source, apply rounded mask, resize to target."""
    img = Image.open(source_path).convert("RGBA")
    # Apply mask at source resolution first for quality
    src_size = img.size[0]
    radius = int(src_size * CORNER_RATIO)
    mask = create_rounded_mask(src_size, radius)
    img.putalpha(mask)
    # Resize
    if output_size != src_size:
        img = img.resize((output_size, output_size), Image.LANCZOS)
    return img


def main():
    print(f"Source: {SOURCE}")
    src = Image.open(SOURCE).convert("RGBA")
    print(f"Source size: {src.size}")

    # Generate PNGs at all required sizes
    for size_name, px in [("32x32", 32), ("128x128", 128), ("128x128@2x", 256)]:
        out = make_rounded(SOURCE, px)
        path = os.path.join(ICONS_DIR, f"{size_name}.png")
        out.save(path, "PNG")
        print(f"  ✓ {size_name}.png ({px}x{px})")

    # Build .iconset for macOS icns conversion
    iconset_dir = os.path.join(ICONS_DIR, "AppIcon.iconset")
    os.makedirs(iconset_dir, exist_ok=True)

    # iconutil requires these exact filenames with @2x suffixes
    iconset_map = {
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

    for filename, px in iconset_map.items():
        rounded = make_rounded(SOURCE, px)
        rounded.save(os.path.join(iconset_dir, filename), "PNG")

    # Convert .iconset → .icns via macOS iconutil
    icns_out = os.path.join(ICONS_DIR, "icon.icns")
    subprocess.run(
        ["iconutil", "-c", "icns", iconset_dir, "-o", icns_out],
        check=True,
    )
    print(f"  ✓ icon.icns")

    # Clean up temp iconset
    import shutil
    shutil.rmtree(iconset_dir)

    # Generate .ico (Windows): embeds multiple sizes in one file
    ico_sizes = [16, 24, 32, 48, 64, 128, 256]
    ico_images = []
    for s in ico_sizes:
        ico_images.append(make_rounded(SOURCE, s))
    ico_out = os.path.join(ICONS_DIR, "icon.ico")
    ico_images[0].save(
        ico_out,
        format="ICO",
        sizes=[(s, s) for s in ico_sizes],
        append_images=ico_images[1:],
    )
    print(f"  ✓ icon.ico ({len(ico_sizes)} sizes)")

    print("\nDone! All icons now have rounded corners.")


if __name__ == "__main__":
    main()
