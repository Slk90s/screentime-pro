# v0.4.1 + v0.4.2 双版本发布指南（2026-07-10）

> 本地已 commit：v0.4.1 (`1d334c2`) + v0.4.2 (`244f622`)。
> 远端最新仍是 v0.4.0 (`a667352`)，v0.3.x 之前。
> 本指南按用户要求「分开发」：v0.4.1（修死锁）→ v0.4.2（加日志）。

---

## 📋 现状速查

| 项 | 状态 |
|---|------|
| 本地 v0.4.1 commit | ✅ `1d334c2`（已 commit，**未 push**） |
| 本地 v0.4.2 commit | ✅ `244f622`（已 commit，**未 push**） |
| 本地 v0.4.1 tag | ❌ 没有（需创建 `v0.4.1`） |
| 本地 v0.4.2 tag | ❌ 没有（需创建 `v0.4.2`） |
| macOS v0.4.1 dmg | ✅ `release/v0.4.1/ScreenTime Pro_0.4.1_aarch64.dmg`（6.3MB） |
| macOS v0.4.2 dmg | ✅ `release/v0.4.2/ScreenTime Pro_0.4.2_aarch64.dmg`（6.8MB） |
| Windows v0.4.1/0.4.2 .exe | ❌ 必须 CI 构建 |
| Linux v0.4.1/00.4.2 .deb/.AppImage | ❌ 必须 CI 构建 |
| 网络（github.com:443 git 协议） | ❌ 阻塞（SSL_ERROR_SYSCALL） |
| 网络（api.github.com） | ✅ 通（`gh api` 工作） |

---

## 🎯 总体流程（4 步）

```
1. push main + 双 tag 到 GitHub
2. CI 跑 Windows + Linux（自动）
3. 手动下载 CI artifacts
4. gh release create v0.4.1 + v0.4.2
```

---

## 🚀 步骤 1：网络恢复后立即 push

```bash
cd "/Users/lkshao/Library/Mobile Documents/com~apple~CloudDocs/Aicodeproject/screentime-pro"

# 1.1 创建本地 tag
git tag v0.4.1 1d334c2
git tag v0.4.2 244f622

# 1.2 推 main + 双 tag（网络恢复后）
git push origin main
git push origin v0.4.1
git push origin v0.4.2
```

> ⚠️ 如果 push 仍然 `SSL_ERROR_SYSCALL`，说明墙没恢复。**可选项**：
> - 等 5-10 分钟重试
> - 用代理：`git config http.proxy http://127.0.0.1:7890`（需要本地有代理）
> - 切 SSH：参考 `docs/RELEASE.md §6` 配置 SSH key

---

## 🤖 步骤 2：等 CI 跑完

推送 tag 后，CI 自动触发（`on: push: tags: ['v*']`）。

- v0.4.1 tag push → CI 跑 v0.4.1（Windows + Linux）
- v0.4.2 tag push → CI 跑 v0.4.2（Windows + Linux）

**手动监控：**

```bash
# 列出最近 4 次 CI 跑（v0.4.1 win+linux, v0.4.2 win+linux）
gh run list --limit 6 --json databaseId,status,name,conclusion \
  --jq '.[] | "\(.databaseId)  \(.status)  \(.conclusion)  \(.name)"'

# 看具体某次跑
gh run view <RUN_ID>

# 等某次跑完（阻塞等待）
gh run watch <RUN_ID> --exit-status
```

---

## 📥 步骤 3：下载 CI artifacts

每个 tag 跑完都会产生 2 个 artifacts：`windows-build` + `linux-build`。

**3.1 找到对应 v0.4.1 / v0.4.2 的 run IDs：**

```bash
# 查看 v0.4.1 的所有 run
gh api repos/Slk90s/screentime-pro/actions/runs \
  --paginate -q '.workflow_runs[] | select(.head_branch=="v0.4.1") | .id' | head -2
# 输出: <win-run-id>  <linux-run-id>

# 查看 v0.4.2 的所有 run
gh api repos/Slk90s/screentime-pro/actions/runs \
  --paginate -q '.workflow_runs[] | select(.head_branch=="v0.4.2") | .id' | head -2
```

**3.2 下载 Windows .exe 到本地：**

```bash
# v0.4.1
gh run download <win-run-id-v0.4.1> -n windows-build
# 把下载的 .exe 移到 release/v0.4.1/
mv screentime-pro_*_x86_64-setup.exe release/v0.4.1/

# v0.4.2
gh run download <win-run-id-v0.4.2> -n windows-build
mv screentime-pro_*_x86_64-setup.exe release/v0.4.2/
```

**3.3 下载 Linux .deb/.AppImage：**

```bash
# v0.4.1
gh run download <linux-run-id-v0.4.1> -n linux-build
mv screentime-pro_*_amd64.deb release/v0.4.1/ 2>/dev/null
mv screentime-pro_*_amd64.AppImage release/v0.4.1/ 2>/dev/null

# v0.4.2
gh run download <linux-run-id-v0.4.2> -n linux-build
mv screentime-pro_*_amd64.deb release/v0.4.2/ 2>/dev/null
mv screentime-pro_*_amd64.AppImage release/v0.4.2/ 2>/dev/null
```

**3.4 验证：**

```bash
ls -la release/v0.4.1/ release/v0.4.2/
# v0.4.1/ 应有: NOTES.md + macOS dmg + Windows exe + Linux deb/AppImage
# v0.4.2/ 应有: NOTES.md + macOS dmg + Windows exe + Linux deb/AppImage
```

---

## 📦 步骤 4：创建 GitHub Release

```bash
cd "/Users/lkshao/Library/Mobile Documents/com~apple~CloudDocs/Aicodeproject/screentime-pro"

# v0.4.1（带 ⚠️ 警告，让用户知道有更优的 v0.4.2）
gh release create v0.4.1 \
  --title "v0.4.1 ⚠️ 已知小问题，建议升级 v0.4.2" \
  --notes-file release/v0.4.1/NOTES.md \
  release/v0.4.1/ScreenTime\ Pro_0.4.1_aarch64.dmg \
  release/v0.4.1/screentime-pro_0.4.1_x86_64-setup.exe \
  release/v0.4.1/screentime-pro_0.4.1_amd64.deb \
  release/v0.4.1/screentime-pro_0.4.1_amd64.AppImage

# v0.4.2（Latest · 推荐）
gh release create v0.4.2 \
  --title "v0.4.2 ⭐ Latest · 推荐" \
  --notes-file release/v0.4.2/NOTES.md \
  release/v0.4.2/ScreenTime\ Pro_0.4.2_aarch64.dmg \
  release/v0.4.2/screentime-pro_0.4.2_x86_64-setup.exe \
  release/v0.4.2/screentime-pro_0.4.2_amd64.deb \
  release/v0.4.2/screentime-pro_0.4.2_amd64.AppImage
```

**注意**：
- `gh release create` 自动给 v0.4.2 打 **Latest** 徽章（v0.4.1 之前没 Latest，v0.4.0 是）
- v0.4.1 标题加 ⚠️ 警告，让用户优先选 v0.4.2
- 如某平台文件名有出入（CI 输出格式偶尔变），用通配符：
  ```bash
  release/v0.4.1/screentime-pro_*_x86_64-setup.exe
  ```

---

## ✅ 完成验证

```bash
# 列出所有 release（应见 v0.4.0 / v0.4.1 / v0.4.2 三个）
gh release list

# 看 v0.4.2 release 详情
gh release view v0.4.2

# Latest 徽章应指向 v0.4.2
gh release view --json isLatest --jq '.isLatest'
# 期望: true
```

---

## 🆘 异常处理

| 问题 | 解决 |
|------|------|
| `git push` 持续 `SSL_ERROR_SYSCALL` | 等 5-10 分钟重试；或配代理；或换网络（手机热点） |
| CI 跑失败 | `gh run view <id> --log-failed` 看具体错误，常见是 webkit2gtk / WebView2 系统包缺失（CI runner 已配，理论上不会） |
| 某个 artifacts 文件名不符合预期 | `ls release/v0.4.2/` 确认实际文件名，调整 `gh release create` 命令 |
| 想撤回 release | `gh release delete v0.4.1 --yes`（软删，tag 还在） |
| 想改 v0.4.1 标题 | `gh release edit v0.4.1 --title "..."` |
| 想把 Latest 改回 v0.4.1 | `gh release edit v0.4.1 --latest`（再 `gh release edit v0.4.2 --latest=false`） |

---

## 🎯 一键脚本（可选）

如果网络恢复且想自动化，参考 `scripts/release-github.sh`（已为单版本设计）。
v0.4.x 双版本连发需要扩展，可后续补充 `scripts/release-pair.sh`。
