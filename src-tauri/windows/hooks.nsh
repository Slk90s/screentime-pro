; windows/hooks.nsh
; ScreenTime Pro — NSIS 安装器钩子（v0.7.8 新增，2026-09-11）
;
; 修改历史：
;   - 2026-09-11 @v0.7.8: 新增 - 安装/卸载前静默结束运行中的托盘进程（避免「写入错误」）
;   - 2026-09-20 @v0.9.2: 新增 - 卸载前把 logs 备份到「文档\ScreenTimePro-Logs」，
;                          避免用户勾选「删除应用数据」时日志一并被删、事后无法追溯
;   - 2026-09-21 @v0.9.5: 重写 SCREENTIME_KILL_APP（轮询等待真退出 + nsExec 强杀兜底），
;                          新增 SCREENTIME_ENSURE_EXE_GONE（卸载段尾部校验 exe 已删除，
;                          失败时 SetErrorLevel 1 + Quit —— 把「静默失败」变成「显式 rc=1」，
;                          让升级安装器走「用户取消」豁免路径静默返回，不再弹「无法卸载!」）
;
; ─────────────────────────────────────────────────────────────
; 为什么需要它（问题现场）
; ─────────────────────────────────────────────────────────────
; 应用在**首次运行时会自动开启开机自启**（见 src-tauri/src/lib.rs：
;   if db.get_setting("autostart").is_none() { app.autolaunch().enable(); ... }），
; 且是常驻托盘程序（主窗口默认不显示）。也就是说用户点开安装包时，
; `screentime-pro.exe` **几乎必然正在运行**，它占用了 $INSTDIR 下的自身映像文件。
;
; Tauri 默认的 installMode=currentUser 安装器虽然带 `CheckIfAppIsRunning`
; （nsis_tauri_utils::FindProcessCurrentUser + KillProcessCurrentUser），
; 但它存在两个缺口，会导致 NSIS 在 `File "${MAINBINARYSRCPATH}"` 一步写不进去，
; 弹出内建的**「写入错误 / Error writing」**对话框：
;   1. 它只做一次 `KillProcess` + `Sleep 500`。在文件 I/O 被安全软件挂钩的机器上
;      （本机装有奇安信天擎 EDR；实测会拦截 Python 写文件、PowerShell Add-Type 等），
;      500ms 常常**不够**内核释放 exe 映像句柄；
;   2. 交互模式下它会先弹「应用正在运行，点击确定以终止运行」——对一个**没有可见窗口**
;      的托盘程序来说提示莫名其妙，用户一旦点取消/关闭就直接中止安装。
;
; 本钩子在 Tauri 内置检查**之前**执行，静默结束进程并**轮询等待其真正退出**，
; 使后面的文件写入必然成功；同时避免弹出那个令人困惑的确认框。
;
; ─────────────────────────────────────────────────────────────
; 旧约定失效
; ─────────────────────────────────────────────────────────────
; ❌「Tauri 安装器自带应用运行检测，无需自己处理」→ 对常驻托盘 + 有 EDR 的机器不成立。
;
; ─────────────────────────────────────────────────────────────
; 可用符号说明
; ─────────────────────────────────────────────────────────────
; - `nsis_tauri_utils` 是 tauri-bundler 随包提供并已被官方模板使用的插件
;   （见 crates/tauri-bundler/.../nsis/utils.nsh 的 CheckIfAppIsRunning），
;   此处复用同一插件，不引入新的依赖。
; - `${MAINBINARYNAME}` / `${INSTALLMODE}` 由 Tauri 在生成的脚本顶部 `!define`，
;   在本文件被 `!include` 展开时均已可用。
; - `Sleep` 是 NSIS 核心指令（毫秒），`${For}` / `${If}` 来自 LogicLib。

; ── 结束正在运行的 ScreenTime Pro，并等待其真正退出 ──────────────
;
; v0.9.5 策略（重写）：
;   1. 检测到进程在跑 → 轮询「结束 → 等 500ms → 复查」，最多 10 轮（累计 ~5s+），
;      进程一消失立即 ${ExitFor}，不空等；
;   2. 轮询耗尽仍在跑 → nsExec::Exec `taskkill /F /T /IM` 强杀兜底（含子进程树），
;      再等 1s 复查；nsExec 不弹窗、不依赖控制台；
;   3. 最终确认：仍在跑只 DetailPrint 警告（交由 Tauri 内置 CheckIfAppIsRunning 处理）。
;
; v0.7.8 旧版缺陷（本次修复的根因）：
;   旧版固定 3 轮 kill + 各 500ms，**不复查进程是否真的退出**就继续往下走。
;   在 EDR 挂钩 / 内核延迟释放映像句柄的机器上，1.5s 窗口内进程可能还活着，
;   卸载段 `Delete "$INSTDIR\screentime-pro.exe"` 因映像占用**静默失败**
;   （NSIS Delete 不置错误也不中断），卸载器 rc=0 正常返回；
;   升级安装器随后 `${FileExists} exe` 命中 → 弹「无法卸载!」。
;   （2026-09-21 v0.9.4 升级实测复现：两次尝试后 exe/注册表/快捷方式全部原样保留。）
;
; 只使用 tauri-bundler 随包提供、官方模板已在用的 `nsis_tauri_utils` 插件 +
; NSIS 自带 nsExec 插件（tauri NSIS 发行版 Plugins/x86-unicode/nsExec.dll 实存），
; LogicLib 的 ${For}/${ExitFor} 在本机 tauri NSIS Include/LogicLib.nsh 实存（L603）。
!macro SCREENTIME_KILL_APP
  !if "${INSTALLMODE}" == "currentUser"
    nsis_tauri_utils::FindProcessCurrentUser "${MAINBINARYNAME}.exe"
  !else
    nsis_tauri_utils::FindProcess "${MAINBINARYNAME}.exe"
  !endif
  Pop $R9

  ${If} $R9 = 0
    DetailPrint "检测到 ${PRODUCTNAME} 正在运行，准备结束进程..."
    ; 轮询最多 10 轮：kill → 500ms → 复查，进程消失立即退出循环
    ${For} $R8 1 10
      !if "${INSTALLMODE}" == "currentUser"
        nsis_tauri_utils::KillProcessCurrentUser "${MAINBINARYNAME}.exe"
      !else
        nsis_tauri_utils::KillProcess "${MAINBINARYNAME}.exe"
      !endif
      Pop $R9
      Sleep 500

      !if "${INSTALLMODE}" == "currentUser"
        nsis_tauri_utils::FindProcessCurrentUser "${MAINBINARYNAME}.exe"
      !else
        nsis_tauri_utils::FindProcess "${MAINBINARYNAME}.exe"
      !endif
      Pop $R9
      ${If} $R9 <> 0
        ${ExitFor}   ; 进程已消失（Find 返回非 0 = 未找到）
      ${EndIf}
    ${Next}

    ; 轮询耗尽仍在跑 → taskkill /F /T 强杀兜底（含子进程树），再等 1s
    ${If} $R9 = 0
      DetailPrint "常规结束超时，尝试强制结束进程树..."
      nsExec::Exec `taskkill /F /T /IM "${MAINBINARYNAME}.exe"`
      Pop $R9
      Sleep 1000
      !if "${INSTALLMODE}" == "currentUser"
        nsis_tauri_utils::FindProcessCurrentUser "${MAINBINARYNAME}.exe"
      !else
        nsis_tauri_utils::FindProcess "${MAINBINARYNAME}.exe"
      !endif
      Pop $R9
    ${EndIf}

    ${If} $R9 = 0
      ; 仍未能结束：留日志，交由 Tauri 内置的 CheckIfAppIsRunning 提示用户手动关闭，
      ; 比静默继续然后抛「写入错误」更容易被用户理解。
      DetailPrint "警告：未能结束 ${MAINBINARYNAME}.exe，安装可能提示应用正在运行。"
    ${EndIf}
  ${EndIf}
!macroend

; ── 安装前（Tauri 内置 CheckIfAppIsRunning 之前执行）─────────────
!macro NSIS_HOOK_PREINSTALL
  !insertmacro SCREENTIME_KILL_APP
!macroend

; ── 卸载前：把日志备份到「文档」（v0.9.2 新增）─────────────────────
;
; 背景：Tauri 默认卸载器在用户勾选「删除应用数据」时执行
;   RmDir /r "$LOCALAPPDATA\${BUNDLEID}"    ← 日志就在其下的 logs\ 子目录里
; （见 tauri-bundler 的 installer.nsi；用户不勾选则保留）。
; 本宏在 PREUNINSTALL 执行（早于上述删除），先把日志复制到
;   $DOCUMENTS\ScreenTimePro-Logs
; 于是无论用户是否勾选「删除应用数据」，卸载后都留有一份可用于追溯的日志。
;
; 说明：
; - 日志目录与 Rust 侧 `app_log_dir()` 一致：%LOCALAPPDATA%\com.screentime.pro\logs
; - 目录不存在 / 创建目标失败 / 复制失败：一律静默跳过，绝不阻断卸载。
; - CopyFiles 不复制子目录；logs 目录是平铺的（app.YYYY-MM-DD.log / audit.YYYY-MM-DD.log），无影响。
; - 只用到 $R0/$R1 两个临时寄存器（卸载段开头本就没有跨寄存器状态）。
!macro SCREENTIME_BACKUP_LOGS
  StrCpy $R0 "$LOCALAPPDATA\com.screentime.pro\logs"
  IfFileExists "$R0\*.*" 0 screentime_backup_logs_done
    StrCpy $R1 "$DOCUMENTS\ScreenTimePro-Logs"
    ClearErrors
    CreateDirectory "$R1"
    IfErrors screentime_backup_logs_done
    CopyFiles /SILENT "$R0\*.*" "$R1"
    DetailPrint "${PRODUCTNAME}：日志已备份到 $R1"
  screentime_backup_logs_done:
!macroend

; ── 卸载前（卸载器要删除 $INSTDIR\screentime-pro.exe，同样怕被占用）──
; 顺序要紧：先结束进程，再把日志复制到一个不会被占用/不会被删的目录。
!macro NSIS_HOOK_PREUNINSTALL
  !insertmacro SCREENTIME_KILL_APP
  !insertmacro SCREENTIME_BACKUP_LOGS
!macroend

; ── 卸载段尾部：校验主程序 exe 确实已删除（v0.9.5 新增）──────────
;
; 背景（2026-09-21 v0.9.4 升级「无法卸载!」根因）：
;   NSIS 的 `Delete` 失败是**静默的**（不置错误标志、不中断），卸载器照常 rc=0 返回。
;   升级安装器 PageLeaveReinstall 在 ExecWait 卸载器之后检查
;   `${If} $0 <> 0 ${OrIf} ${FileExists} "$INSTDIR\${MAINBINARYNAME}.exe"`，
;   命中即弹「无法卸载!」并中止升级。实测 NSIS 探针：`Abort` 退出码=2，
;   模板只豁免 rc=1（用户取消）——所以任何 Abort 都会弹「无法卸载!」。
;
; 本宏在 POSTUNINSTALL（所有文件删除之后）执行：
;   - exe 已删干净 → 正常返回（rc=0），升级安装器继续；
;   - exe 仍在（映像占用/EDR 拦截等）→ SetErrorLevel 1 + Quit：
;     把「静默失败」变成**显式 rc=1**，升级安装器将其视为「用户取消」
;     走豁免路径静默返回重装选择页（不弹「无法卸载!」），用户可重试。
;   注意：用 Quit 而非 Abort —— Abort 退出码是 2（本机 makensis v3.08 实测），
;   2 不在豁免列表内，会再次触发「无法卸载!」。
!macro SCREENTIME_ENSURE_EXE_GONE
  ${If} ${FileExists} "$INSTDIR\${MAINBINARYNAME}.exe"
    DetailPrint "警告：${MAINBINARYNAME}.exe 未能删除（可能被占用），中止卸载以便重试。"
    SetErrorLevel 1
    Quit
  ${EndIf}
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  !insertmacro SCREENTIME_ENSURE_EXE_GONE
!macroend
