#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""从各平台构建产物生成 Tauri updater 的 latest.json。

用法：gen_latest_json.py <artifacts_dir> <输出文件>

# 为什么不用 tauri-action 自带的 includeUpdaterJson

它在每个 runner 上只写**自己那一个平台**的条目，而本项目三台 runner（Windows /
Linux / macOS）并行跑同一个 tag，三份 latest.json 互相覆盖 —— 最终线上只剩最后
完成的那台平台，另外两台的用户 `check()` 会拿到 `Err(TargetsNotFound)`
（tauri-plugin-updater 2.12.0 src/updater.rs:637），界面表现为「检查更新失败」
而不是「已是最新」。所以统一由最后一个作业汇总生成。

# 平台键的命名规则（必须与 updater 源码严格一致）

`get_urls()`（updater.rs:608-638）的查找顺序是：
  ① `{os}-{arch}-{installer}`   ② `{os}-{arch}`
其中：
  - `os`     = `updater_os()`：windows / linux / darwin（**注意 mac 是 darwin 不是 macos**）
  - `arch`   = `updater_arch()`：x86_64 / aarch64 / i686 / armv7（**不是** Node 的 x64/arm64）
  - installer = `Installer::name()` 的小写值：nsis / msi / appimage / deb / rpm / app

本项目实际产出的两个键：
  - windows-x86_64-nsis        （NSIS setup.exe）
  - linux-x86_64-appimage      （AppImage.tar.gz）

macOS **故意不写**：本项目是 ad-hoc 签名，签名身份每次构建都变，TCC 授权
（屏幕录制 / 辅助功能）会随升级整体失效。详见 src-tauri/src/app_update.rs 模块头。

# signature 字段

必须是 `.sig` 文件的**文本内容**（minisign 签名串），不是路径、不是 URL。
写成路径的话客户端下载完才验签失败，用户已经白等一次。
"""

from __future__ import annotations

import glob
import json
import os
import re
import sys


# (平台键, 安装包 glob, 说明) —— glob 相对 artifacts 根目录，recursive=True
TARGETS = [
    ("windows-x86_64-nsis", "**/*_x64-setup.exe",
     "Windows NSIS 安装包"),
    ("linux-x86_64-appimage", "**/*_amd64.AppImage.tar.gz",
     "Linux AppImage（tar.gz 包，updater 会解包取 .AppImage）"),
]


def die(msg: str) -> None:
    """输出 GitHub Actions 的 error 注解并退出非 0。

    用注解而不是普通 print：CI 失败时错误会直接显示在作业摘要里，
    不用展开日志去找。"""
    print(f"::error::{msg}", file=sys.stderr)
    sys.exit(1)


def pick_one(pattern: str) -> str:
    """按 glob 找唯一一个文件。

    找不到 → 报错（构建没产出安装包，或产物名跟预期不符 —— 后者往往是
    Tauri 改了命名规则，必须人工确认，不能静默跳过）。
    找到多个 → 也报错。多出来的可能是旧产物残留，选错会把旧包挂到新版本上，
    客户端装完版本号还是旧的，表现为「升级了但没变」。"""
    hits = sorted(glob.glob(pattern, recursive=True))
    if not hits:
        die(f"找不到匹配 {pattern!r} 的产物（构建失败，或产物命名规则变了）")
    if len(hits) > 1:
        die(f"{pattern!r} 匹配到 {len(hits)} 个文件，无法确定该用哪一个：{hits}")
    return hits[0]


def main() -> int:
    if len(sys.argv) != 3:
        print(__doc__)
        return 2
    artifacts_dir, out_path = sys.argv[1], sys.argv[2]

    tag = os.environ.get("TAG", "").strip()
    repo = os.environ.get("REPO", "").strip()
    if not tag or not repo:
        die("缺少环境变量 TAG / REPO")
    # 版本号取 tag 去掉前缀 v。latest.json 的 version 必须是**纯 semver**
    # （updater 用 semver::Version 反序列化，带 v 前缀会直接解析失败）。
    version = tag.lstrip("vV")

    if not os.path.isdir(artifacts_dir):
        die(f"产物目录不存在：{artifacts_dir}")

    platforms: dict[str, dict[str, str]] = {}
    for key, pattern, label in TARGETS:
        pkg = pick_one(os.path.join(artifacts_dir, pattern))
        sig = pkg + ".sig"
        if not os.path.exists(sig):
            # 常见原因：构建时没给 TAURI_SIGNING_PRIVATE_KEY。
            # 这种情况必须让 CI 红掉 —— 静默产出一份没有正确签名的 latest.json，
            # 客户端会一直报验签失败，比构建失败更难排查。
            die(f"{os.path.basename(pkg)} 缺少签名文件 {os.path.basename(sig)}"
                "（检查 TAURI_SIGNING_PRIVATE_KEY secret 是否配置）")
        with open(sig, encoding="utf-8") as f:
            signature = f.read().strip()
        if not signature:
            die(f"签名文件为空：{sig}")

        name = os.path.basename(pkg)
        platforms[key] = {
            "signature": signature,
            # 用固定 tag 的直链而不是 /latest/download/：
            # latest.json 本身会被后续版本覆盖，它里面指向的安装包必须是**本版本**
            # 的固定地址，否则用户点升级会下到下一个版本的包（版本号还对不上）。
            "url": f"https://github.com/{repo}/releases/download/{tag}/{name}",
        }
        print(f"  {key}: {name} ({os.path.getsize(pkg)} bytes) — {label}")

    # ⚠️ pub_date 一旦写了，就必须是**严格 RFC3339**，否则客户端反序列化会直接
    # 报错（updater.rs:1479-1484 用 `OffsetDateTime::parse(..., Rfc3339)` 且把
    # 错误转成 `DeError`）—— 表现是「检查更新失败」，而不是「已是最新」。
    # 所以：
    #   ① 环境变量必须传入真正的 RFC3339（CI 里用 `date -u +%Y-%m-%dT%H:%M:%SZ`）；
    #   ② 拿不到就**整个字段省略**（`None` 是合法的，见同处 `else { None }`）。
    # 绝不能塞 `github.event.repository.updated_at` 这类非 RFC3339 值 —— 它带
    # `+00:00` 有时能过、有时不能，属于「看起来能用」的定时炸弹。
    pub_date = (os.environ.get("PUB_DATE") or "").strip()
    if pub_date:
        # 本地/CI 双重保险：非法格式直接失败，不产出会被客户端拒绝的 JSON。
        if not re.fullmatch(r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?Z", pub_date):
            die(f"PUB_DATE 不是 RFC3339（UTC, Z 结尾）：{pub_date!r}")

    manifest: dict[str, object] = {
        "version": version,
        "notes": f"见 https://github.com/{repo}/releases/tag/{tag}",
        "platforms": platforms,
    }
    if pub_date:
        manifest["pub_date"] = pub_date
    with open(out_path, "w", encoding="utf-8") as f:
        json.dump(manifest, f, ensure_ascii=False, indent=2)
        f.write("\n")

    print(f"已写出 {out_path}（version={version}, {len(platforms)} 个平台）")
    return 0


if __name__ == "__main__":
    sys.exit(main())
