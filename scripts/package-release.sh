#!/usr/bin/env bash
# 将三端构建产物按版本号重命名并集中到 release/vX.Y.Z/
# 用法：
#   1) 在各平台分别执行 `npm run tauri build`（或交叉编译 Windows）
#   2) 在本机（macOS）运行本脚本，自动收集已产出的产物
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CONF="$ROOT/src-tauri/tauri.conf.json"
VER="$(grep -m1 '"version"' "$CONF" | sed -E 's/.*"version": *"([^"]+)".*/\1/')"
OUT="$ROOT/release/v$VER"
mkdir -p "$OUT"

echo ">> 收集 v$VER 的发布产物到 $OUT"

# ---- macOS（在本机或 macOS CI 跑 tauri build 后产出）----
MAC_DMG="$ROOT/src-tauri/target/release/bundle/dmg/ScreenTime Pro_${VER}_aarch64.dmg"
if [ -f "$MAC_DMG" ]; then
  cp "$MAC_DMG" "$OUT/"
  echo "  ✔ macOS dmg"
else
  echo "  ✖ 未找到 macOS dmg（需先在 macOS 上 build）"
fi

# ---- Windows（交叉编译或 windows CI 产出）----
WIN_EXE="$ROOT/src-tauri/target/x86_64-pc-windows-gnu/release/screentime-pro.exe"
if [ -f "$WIN_EXE" ]; then
  cp "$WIN_EXE" "$OUT/screentime-pro_${VER}_x86_64.exe"
  echo "  ✔ Windows exe"
else
  echo "  ✖ 未找到 Windows exe（可 `cargo build --release --target x86_64-pc-windows-gnu`）"
fi

# ---- Linux（linux CI 产出 AppImage / deb）----
LIN_APPIMAGE="$(ls "$ROOT"/src-tauri/target/*/release/bundle/appimage/*.AppImage 2>/dev/null | head -1 || true)"
if [ -n "$LIN_APPIMAGE" ]; then
  cp "$LIN_APPIMAGE" "$OUT/screentime-pro_${VER}_amd64.AppImage"
  echo "  ✔ Linux AppImage"
fi
LIN_DEB="$(ls "$ROOT"/src-tauri/target/*/release/bundle/deb/*.deb 2>/dev/null | head -1 || true)"
if [ -n "$LIN_DEB" ]; then
  cp "$LIN_DEB" "$OUT/screentime-pro_${VER}_amd64.deb"
  echo "  ✔ Linux deb"
fi

echo ">> 完成："
ls -la "$OUT"
