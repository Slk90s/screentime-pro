#!/usr/bin/env bash
# 发布 release/v${ver}/ 下的所有产物到 GitHub Releases
# 用法：
#   ./scripts/release-github.sh                     # 自动读 tauri.conf.json 的 version + 同名 NOTES.md
#   ./scripts/release-github.sh --draft             # 创建草稿（手动在网页审核再公开）
#   ./scripts/release-github.sh --notes ./NOTES.md  # 自定义 notes 文件路径
# 前置：
#   - 已构建产物并复制到 release/v${ver}/
#   - 已写好 release/v${ver}/NOTES.md
#   - 已登录 gh CLI（gh auth status 正常）
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CONF="$ROOT/src-tauri/tauri.conf.json"
DRAFT_FLAG=""
NOTES_FILE=""

# 解析参数
while [ $# -gt 0 ]; do
  case "$1" in
    --draft)        DRAFT_FLAG="--draft" ;;
    --notes)        NOTES_FILE="$2"; shift ;;
    --notes=*)      NOTES_FILE="${1#--notes=}" ;;
    -h|--help)
      echo "用法：$0 [--draft] [--notes <file>]"
      echo ""
      echo "默认 notes: release/v\${ver}/NOTES.md"
      exit 0
      ;;
    *) echo "未知参数：$1" >&2; exit 1 ;;
  esac
  shift
done

# 读版本号
if [ ! -f "$CONF" ]; then
  echo "❌ 未找到 $CONF" >&2
  exit 1
fi
VER="$(grep -m1 '"version"' "$CONF" | sed -E 's/.*"version": *"([^"]+)".*/\1/')"
if [ -z "$VER" ]; then
  echo "❌ 解析 version 失败，请检查 $CONF" >&2
  exit 1
fi
OUT="$ROOT/release/v$VER"
[ -z "$NOTES_FILE" ] && NOTES_FILE="$OUT/NOTES.md"

echo "==============================================="
echo "  ScreenTime Pro · GitHub Release 助手"
echo "==============================================="
echo "  版本     : v$VER"
echo "  产物目录 : $OUT"
echo "  Notes    : $NOTES_FILE"
[ -n "$DRAFT_FLAG" ] && echo "  模式     : DRAFT（草稿）"
echo "==============================================="

# 校验产物目录
if [ ! -d "$OUT" ] || [ -z "$(ls -A "$OUT" 2>/dev/null | grep -v '^\.')" ]; then
  echo "❌ $OUT 不存在或为空" >&2
  echo "   请先构建并把产物复制到此目录（可用 ./scripts/package-release.sh）" >&2
  exit 1
fi

# 校验 notes
if [ ! -f "$NOTES_FILE" ]; then
  echo "⚠️  $NOTES_FILE 不存在 —— GitHub 会用默认空 notes" >&2
  echo "   建议先按 docs/RELEASE.md §4 模板写好再发布" >&2
  read -r "?仍然继续吗？(y/N) " ans
  [[ "$ans" =~ ^[Yy]$ ]] || { echo "已取消"; exit 1; }
fi

# 校验 gh 登录
if ! gh auth status >/dev/null 2>&1; then
  echo "❌ gh CLI 未登录，先跑：gh auth login" >&2
  exit 1
fi

# 校验 tag 不重复
if git rev-parse "v$VER" >/dev/null 2>&1; then
  echo "❌ tag v$VER 已存在：$(git rev-parse --short v$VER 2>/dev/null)" >&2
  echo "   如需重发：git tag -d v$VER && git push origin :refs/tags/v$VER" >&2
  exit 1
fi

# 列出要上传的产物
echo ""
echo "将上传产物："
ARTIFACTS=()
while IFS= read -r f; do
  ARTIFACTS+=("$f")
  echo "  📦 $(basename "$f") ($(du -h "$f" | cut -f1))"
done < <(find "$OUT" -maxdepth 1 -type f \( -name '*.dmg' -o -name '*.exe' -o -name '*.deb' -o -name '*.AppImage' \) | sort)

if [ "${#ARTIFACTS[@]}" -eq 0 ]; then
  echo "❌ 未在 $OUT 找到可发布产物（需 .dmg / .exe / .deb / .AppImage）" >&2
  exit 1
fi

echo ""
read -r "?确认发布 v$VER 吗？(y/N) " ans
[[ "$ans" =~ ^[Yy]$ ]] || { echo "已取消"; exit 0; }

# 创建 tag + push
echo ""
echo ">> 创建本地 tag v$VER"
git tag "v$VER"

echo ">> 推送 tag 到 origin"
git push origin "v$VER"

# 构造 gh release create
CMD=(gh release create "v$VER")
CMD+=(--title "v$VER")
[ -n "$DRAFT_FLAG" ] && CMD+=("$DRAFT_FLAG")
if [ -f "$NOTES_FILE" ]; then
  CMD+=(--notes-file "$NOTES_FILE")
fi
CMD+=("${ARTIFACTS[@]}")

echo ""
echo ">> 执行：${CMD[*]}"
"${CMD[@]}"

# 等 CI 上传完（如有 Linux/Windows 产物还没下载）
REPO="$(gh repo view --json nameWithOwner -q .nameWithOwner)"
RELEASE_URL="https://github.com/$REPO/releases/tag/v$VER"

echo ""
echo "==============================================="
echo "  ✅ v$VER Release 已创建"
echo "==============================================="
echo "  URL : $RELEASE_URL"
echo ""
echo "  👉 新 Release 会自动获 'Latest' 标记"
echo "     (gh release create 默认行为，无需 --latest flag)"
echo ""
echo "  后续可选："
echo "    - 等 CI 完成 → 下载 windows-build / linux-build artifacts"
echo "    - 手动追加：gh release upload v$VER ~/Downloads/*.exe ~/Downloads/*.deb"
echo "    - 加 tag 推送触发 CI：git push origin v$VER （如 tag 没推成功，重推）"
echo "==============================================="
