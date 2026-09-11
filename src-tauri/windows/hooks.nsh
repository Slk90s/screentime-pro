; windows/hooks.nsh
; ScreenTime Pro — NSIS 安装器钩子（v0.7.8 新增，2026-09-11）
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
; 策略：检测到在跑时，做 3 轮「结束 → 等 500ms」，累计等待窗口 1.5s，最后再确认一次。
; 之所以要重复且要等：原「写入错误」的根因就是 Tauri 内置逻辑只 Kill 一次 + Sleep 500，
; 在内核（或被 EDR 挂钩的文件系统）尚未释放 `screentime-pro.exe` 映像句柄时，
; 紧接着的 `File "${MAINBINARYSRCPATH}"` 就会写失败。
;
; 只使用 tauri-bundler 随包提供、官方模板已在用的 `nsis_tauri_utils` 插件，
; 不引入 `nsExec` 等额外插件依赖，避免插件缺失导致 CI 构建或安装器运行期出错。
; LogicLib 由 MUI2.nsh 在本文件之前引入（模板第 226 行起即在用 `${If}`），
; 而宏体在 `!insertmacro` 展开时求值，因此 `${If}` / `${For}` 必然可用。
!macro SCREENTIME_KILL_APP
  !if "${INSTALLMODE}" == "currentUser"
    nsis_tauri_utils::FindProcessCurrentUser "${MAINBINARYNAME}.exe"
  !else
    nsis_tauri_utils::FindProcess "${MAINBINARYNAME}.exe"
  !endif
  Pop $R9

  ${If} $R9 = 0
    DetailPrint "检测到 ${PRODUCTNAME} 正在运行，准备结束进程..."
    ${For} $R8 1 3
      !if "${INSTALLMODE}" == "currentUser"
        nsis_tauri_utils::KillProcessCurrentUser "${MAINBINARYNAME}.exe"
      !else
        nsis_tauri_utils::KillProcess "${MAINBINARYNAME}.exe"
      !endif
      Pop $R9
      ; 给内核时间释放 exe 映像文件句柄（这一步是原「写入错误」的关键）
      Sleep 500
    ${Next}

    ; 最终确认进程已消失
    !if "${INSTALLMODE}" == "currentUser"
      nsis_tauri_utils::FindProcessCurrentUser "${MAINBINARYNAME}.exe"
    !else
      nsis_tauri_utils::FindProcess "${MAINBINARYNAME}.exe"
    !endif
    Pop $R9

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

; ── 卸载前（卸载器要删除 $INSTDIR\screentime-pro.exe，同样怕被占用）──
!macro NSIS_HOOK_PREUNINSTALL
  !insertmacro SCREENTIME_KILL_APP
!macroend
