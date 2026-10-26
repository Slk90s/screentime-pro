<template>
  <!-- 设置页（v0.6.2-beta.19 重构）
       视觉：每功能一卡片，head 区域 圆角色块图标 + 标题/副标题，body 区域放交互控件。
       功能不裁：原 Settings 全部功能（设备名/空闲阈值/保留天数/语言/自启/备份导入/日志/危险区/桌宠/皮肤/编辑器/检查更新/关于）全部保留。 -->
  <div class="settings">
    <!-- ============ 顶部品牌色横条 + 标题 ============ -->
    <div class="settings-header">
      <div class="header-bar" />
      <h2>设置</h2>
    </div>

    <!-- ============ 语言 ============ -->
    <div class="setting-card">
      <div class="card-head">
        <div class="head-icon icon-orange">
          <AppIcon name="language" :size="20" />
        </div>
        <div class="head-text">
          <h3>{{ t("settings.languageTitle") }}</h3>
          <p>{{ t("settings.languageDesc") }}</p>
        </div>
      </div>
      <div class="card-body">
        <div class="radio-pills">
          <label class="radio-pill" :class="{ active: i18n.global.locale.value === 'zh-CN' }">
            <input
              type="radio"
              name="lang"
              value="zh-CN"
              :checked="i18n.global.locale.value === 'zh-CN'"
              @change="onLangChange"
            />
            <span class="radio-dot" />
            <span>简体中文</span>
          </label>
          <label class="radio-pill" :class="{ active: i18n.global.locale.value === 'en-US' }">
            <input
              type="radio"
              name="lang"
              value="en-US"
              :checked="i18n.global.locale.value === 'en-US'"
              @change="onLangChange"
            />
            <span class="radio-dot" />
            <span>English</span>
          </label>
        </div>
      </div>
    </div>

    <!-- ============ 通用（设备名 / 空闲阈值 / 保留天数 / 开机自启）============ -->
    <div class="setting-card">
      <div class="card-head">
        <div class="head-icon icon-blue">
          <AppIcon name="settings" :size="20" />
        </div>
        <div class="head-text">
          <h3>{{ t("settings.generalTitle") }}</h3>
          <p>{{ t("settings.generalDesc") }}</p>
        </div>
      </div>
      <div class="card-body">
        <div class="form-row">
          <label>{{ t("settings.deviceName") }}</label>
          <input
            v-model="deviceName"
            type="text"
            class="text-input"
            :placeholder="t('settings.deviceNamePh')"
          />
          <p class="field-hint">{{ t("settings.deviceNameHint") }}</p>
        </div>

        <div class="form-row">
          <label>{{ t("settings.idleThreshold") }}</label>
          <input
            v-model.number="idleMin"
            type="number"
            min="1"
            max="60"
            class="text-input narrow"
          />
          <p class="field-hint">{{ t("settings.idleHint") }}</p>
        </div>

        <div class="form-row">
          <label>{{ t("settings.retention") }}</label>
          <input
            v-model.number="retention"
            type="number"
            min="30"
            max="3650"
            class="text-input narrow"
          />
          <p class="field-hint">{{ t("settings.retentionHint") }}</p>
        </div>

        <div class="form-row row">
          <label>{{ t("settings.autostart") }}</label>
          <label class="toggle-switch" :class="{ on: autostart }">
            <input
              type="checkbox"
              :checked="autostart"
              @change="onAutostart($event)"
            />
            <span class="toggle-slider" />
            <span class="toggle-state">
              {{ autostart ? t("settings.autostartOn") : t("settings.autostartOff") }}
            </span>
          </label>
        </div>

        <div class="card-actions">
          <button class="primary-btn" @click="onSave">
            <AppIcon name="save" :size="14" /> {{ t("settings.save") }}
          </button>
        </div>
      </div>
    </div>

    <!-- ============ 状态栏（v0.7.6：总开关 + 3 个子项复选框）============ -->
    <div class="setting-card">
      <div class="card-head">
        <div class="head-icon icon-orange">
          <AppIcon name="tool" :size="20" />
        </div>
        <div class="head-text">
          <h3>{{ t("settings.statusBarTitle") }}</h3>
          <p>{{ t("settings.statusBarDesc") }}</p>
        </div>
      </div>
      <div class="card-body">
        <!-- 总开关：启用状态栏 -->
        <div class="form-row row between">
          <label>{{ t("settings.statusBarEnabled") }}</label>
          <label class="toggle-switch" :class="{ on: statusBarConfig.enabled }">
            <input
              type="checkbox"
              :checked="statusBarConfig.enabled"
              @change="onStatusBarEnabled($event)"
            />
            <span class="toggle-slider" />
            <span class="toggle-state">
              {{ statusBarConfig.enabled ? t("settings.statusBarOn") : t("settings.statusBarOff") }}
            </span>
          </label>
        </div>

        <!-- 3 个子项复选框（仅启用时可交互） -->
        <div class="sub-zone" :class="{ disabled: !statusBarConfig.enabled }">
          <div class="form-row row between">
            <label>{{ t("settings.statusBarShowCpu") }}</label>
            <input
              type="checkbox"
              class="check-input"
              :checked="statusBarConfig.show_cpu"
              :disabled="statusBarChildrenDisabled"
              @change="onStatusBarItem($event, 'show_cpu')"
            />
          </div>
          <div class="form-row row between">
            <label>{{ t("settings.statusBarShowMem") }}</label>
            <input
              type="checkbox"
              class="check-input"
              :checked="statusBarConfig.show_mem"
              :disabled="statusBarChildrenDisabled"
              @change="onStatusBarItem($event, 'show_mem')"
            />
          </div>
          <!-- v0.7.7（2026-09-10）：磁盘占用（取系统盘；三平台均已支持采样，默认关） -->
          <div class="form-row row between">
            <label>{{ t("settings.statusBarShowDisk") }}</label>
            <input
              type="checkbox"
              class="check-input"
              :checked="statusBarConfig.show_disk"
              :disabled="statusBarChildrenDisabled"
              @change="onStatusBarItem($event, 'show_disk')"
            />
          </div>
          <div class="form-row row between">
            <label>{{ t("settings.statusBarShowNet") }}</label>
            <input
              type="checkbox"
              class="check-input"
              :checked="statusBarConfig.show_net"
              :disabled="statusBarChildrenDisabled"
              @change="onStatusBarItem($event, 'show_net')"
            />
          </div>
          <p class="field-hint">{{ t("settings.statusBarHint") }}</p>

          <!-- v0.7.6：悬浮指标条（2026-09-10 起受「启用状态栏」总开关统管，随子项一并置灰；
               可拖拽，全屏自动隐藏。旧设计「独立于总开关」作废——死开关体验，见 CHANGELOG） -->
          <div class="form-row row between">
            <label>{{ t("settings.statusBarFloat") }}</label>
            <input
              type="checkbox"
              class="check-input"
              :checked="statusBarConfig.float_enabled"
              :disabled="statusBarChildrenDisabled"
              @change="onStatusBarItem($event, 'float_enabled')"
            />
          </div>
          <p class="field-hint platform-note">{{ t("settings.statusBarFloatHint") }}</p>

          <!-- v1.2.2（2026-10-21）：液态玻璃外观。
               受三重门控：① 总开关启用；② 浮窗本身开启（玻璃只作用于浮窗）；
               ③ `supported`（老系统 / Linux 无系统材质 → 置灰并给出原因）。
               开关值随正式配置一起提交，Rust 侧 set_status_bar_config 检测到翻转后
               会立刻重新应用/清除材质并同步窗口高度，无需额外 IPC。 -->
          <div class="form-row row between">
            <label>{{ t("settings.statusBarGlass") }}</label>
            <input
              type="checkbox"
              class="check-input"
              :checked="statusBarConfig.float_glass"
              :disabled="!statusBarConfig.enabled || !statusBarConfig.float_enabled || !glassSupported"
              :title="glassReason"
              @change="onStatusBarItem($event, 'float_glass')"
            />
          </div>
          <p class="field-hint platform-note">
            {{ glassHint }}
          </p>
        </div>
      </div>
    </div>

    <!-- ============ 屏幕截图（v0.8.0）============ -->
    <div class="setting-card">
      <div class="card-head">
        <div class="head-icon icon-pink">
          <AppIcon name="crop" :size="20" />
        </div>
        <div class="head-text">
          <h3>{{ t("settings.shotTitle") }}</h3>
          <p>{{ t("settings.shotDesc") }}</p>
        </div>
      </div>
      <div class="card-body">
        <div class="form-row row between">
          <label>{{ t("settings.shotEnabled") }}</label>
          <label class="toggle-switch" :class="{ on: shotConfig.enabled }">
            <input type="checkbox" :checked="shotConfig.enabled" @change="onShotEnabled($event)" />
            <span class="toggle-slider" />
            <span class="toggle-state">
              {{ shotConfig.enabled ? t("settings.statusBarOn") : t("settings.statusBarOff") }}
            </span>
          </label>
        </div>

        <div class="sub-zone" :class="{ disabled: !shotConfig.enabled }">
          <!-- 快捷键：点击后**直接按下组合键**即录入。
               v0.8.0 初版是个纯文本框，用户按组合键时修饰键不产生字符 → 看起来"设置不了"；
               而且当时 Rust 无论注册成败都返回成功，填了被占用的组合也显示"已设置"，
               实际快捷键是死的 → 这就是「快捷键无法设置」的两条根因。 -->
          <div class="form-row row between">
            <label>{{ t("settings.shotShortcut") }}</label>
            <div class="hotkey-row">
              <button
                ref="hotkeyBtnEl"
                class="hotkey-btn"
                :class="{ recording: hotkeyRecording, invalid: !!hotkeyError }"
                :disabled="!shotConfig.enabled"
                @click="startHotkeyRecording"
                @blur="stopHotkeyRecording"
                @keydown="onHotkeyKeydown"
              >
                <template v-if="hotkeyRecording">{{ hotkeyPreview || t("settings.shotShortcutRecording") }}</template>
                <template v-else>{{ formatHotkey(shotConfig.shortcut) }}</template>
              </button>
              <button class="ghost-btn" :disabled="!shotConfig.enabled" @click="resetHotkey">
                {{ t("settings.shotShortcutReset") }}
              </button>
            </div>
          </div>
          <p v-if="hotkeyError" class="field-hint field-hint--error">{{ hotkeyError }}</p>
          <p class="field-hint">{{ t("settings.shotShortcutHint") }}</p>

          <div class="form-row row between">
            <label>{{ t("settings.shotAutoSave") }}</label>
            <input
              type="checkbox"
              class="check-input"
              :checked="shotConfig.auto_save"
              :disabled="!shotConfig.enabled"
              @change="onShotToggle($event, 'auto_save')"
            />
          </div>
          <p class="field-hint">{{ t("settings.shotAutoSaveHint") }}</p>

          <!-- v0.9.0：取字引擎切换（标准 = 系统内置 / 增强 = PaddleOCR-ONNX 本地模型）。
               「标准」在 Windows = WinRT、macOS = 系统 Vision；Linux 暂无，此时置灰。
               「增强」只在**模型缺失**（= 安装不完整）时置灰；运行库缺失不是禁用理由
               —— macOS / Linux 的运行库本就不随包，得靠选中「增强」才触发后台下载。 -->
          <p class="zone-title">{{ t("settings.shotOcrEngine") }}</p>
          <div class="engine-picker">
            <label
              class="engine-opt"
              :class="{ on: shotConfig.ocr_engine === 'system', off: ocrInfo && !ocrInfo.system_available }"
            >
              <input
                type="radio"
                name="ocr-engine"
                value="system"
                :checked="shotConfig.ocr_engine === 'system'"
                :disabled="!shotConfig.enabled || (!!ocrInfo && !ocrInfo.system_available)"
                @change="onOcrEngine('system')"
              />
              <b>{{ t("settings.shotOcrEngineSystem") }}</b>
              <span>{{ systemEngineDesc }}</span>
            </label>
            <label
              class="engine-opt"
              :class="{ on: shotConfig.ocr_engine === 'enhanced', off: ocrInfo && !ocrInfo.enhanced_models_ready }"
            >
              <input
                type="radio"
                name="ocr-engine"
                value="enhanced"
                :checked="shotConfig.ocr_engine === 'enhanced'"
                :disabled="!shotConfig.enabled || !enhancedSelectable"
                @change="onOcrEngine('enhanced')"
              />
              <b>{{ t("settings.shotOcrEngineEnhanced") }}</b>
              <span>{{ t("settings.shotOcrEngineEnhancedDesc") }}</span>
            </label>
          </div>
          <p class="field-hint">{{ ocrEngineHint }}</p>

          <p class="zone-title">{{ t("settings.shotExportStyle") }}</p>
          <div class="form-row row between">
            <label>{{ t("settings.shotRadius") }} · {{ shotConfig.corner_radius }}px</label>
            <input
              v-model.number="shotConfig.corner_radius"
              class="range-input"
              type="range"
              min="0"
              max="40"
              step="2"
              :disabled="!shotConfig.enabled"
              @change="persistShotConfig"
            />
          </div>
          <!-- v1.2.2：投影开关已移除（功能整体下线），此处只留圆角 -->
          <p class="field-hint">{{ t("settings.shotRadiusHint") }}</p>

          <div class="form-row row between">
            <label>{{ t("settings.shotMaxCount") }}</label>
            <input
              v-model.number="shotConfig.max_count"
              class="text-input narrow"
              type="number"
              min="10"
              max="5000"
              :disabled="!shotConfig.enabled"
              @change="persistShotConfig"
            />
          </div>
          <p class="field-hint">{{ t("settings.shotMaxCountHint") }}</p>

          <div class="form-row row between">
            <label>{{ t("settings.shotDir") }}</label>
            <div class="dir-row">
              <span class="path-text" :title="shotDir">{{ shotDir || "—" }}</span>
              <button class="ghost-btn" @click="openShotDir">{{ t("settings.shotOpenDir") }}</button>
            </div>
          </div>

          <!-- v1.2.2：移除「立即截图」按钮 —— 触发入口保留在浮窗胶囊与全局快捷键，
               设置页不该成为第三个讲不清"按了会发生什么"的入口。 -->
          <!-- v1.1.0：延时截图（借鉴 Snapzy）——倒计时后进入正常截图，可先摆好界面 -->
          <div class="form-row row between">
            <label :title="t('settings.shotDelayHint')">{{ t("settings.shotDelay") }}</label>
            <div class="btn-row">
              <button
                v-for="sec in [3, 5, 10]"
                :key="sec"
                class="ghost-btn"
                :disabled="!shotConfig.enabled"
                @click="onShotDelayed(sec)"
              >
                {{ sec }}s
              </button>
            </div>
          </div>
        </div>

        <!-- 截图历史（缩略图按需拉取，避免一次把 200 张全读进内存） -->
        <p class="zone-title">{{ t("settings.shotHistory") }}</p>
        <p v-if="shotHistory.length === 0" class="field-hint">{{ t("settings.shotHistoryEmpty") }}</p>
        <template v-else>
          <div class="shot-toolbar">
            <button class="ghost-btn" @click="toggleShotSelecting">
              {{ shotSelecting ? t("settings.shotSelectDone") : t("settings.shotSelect") }}
            </button>
            <template v-if="shotSelecting">
              <button class="ghost-btn" @click="selectAllShots">{{ t("settings.shotSelectAll") }}</button>
              <button class="ghost-btn" @click="clearShotSelection">{{ t("settings.shotSelectNone") }}</button>
              <span class="shot-sel-count">{{ t("settings.shotSelected", { n: selectedShotIds.length }) }}</span>
              <button
                class="danger-btn"
                :disabled="selectedShotIds.length === 0"
                @click="deleteSelectedShots"
              >
                {{ t("settings.shotDeleteSelected") }} ({{ selectedShotIds.length }})
              </button>
            </template>
          </div>
          <div class="shot-grid">
            <figure
              v-for="s in shotHistory"
              :key="s.id"
              class="shot-item"
              :class="{ selected: shotSelecting && selectedShotIds.includes(s.id), selectable: shotSelecting }"
              @click="shotSelecting && toggleShotSelect(s.id)"
            >
              <label v-if="shotSelecting" class="shot-check" @click.prevent="toggleShotSelect(s.id)">
                <input type="checkbox" :checked="selectedShotIds.includes(s.id)" />
              </label>
              <img v-if="shotThumbs[s.id]" class="shot-img" :src="shotThumbs[s.id]" :alt="s.file_name" />
              <div v-else class="shot-img shot-img--empty"></div>
              <figcaption class="shot-cap">
                <span class="shot-meta">{{ s.width }}×{{ s.height }} · {{ formatBytes(s.bytes) }}</span>
                <span class="shot-meta shot-date">{{ s.created_at.slice(0, 16).replace("T", " ") }}</span>
                <span class="shot-actions">
                  <button class="ghost-btn" @click="revealShot(s.id)">{{ t("settings.shotReveal") }}</button>
                  <button class="danger-btn" @click="deleteShot(s.id)">{{ t("settings.shotDelete") }}</button>
                </span>
              </figcaption>
            </figure>
          </div>
          <!-- v1.1.1：加载更多（分页追加），与 max_count 解耦 -->
          <div v-if="shotHasMore" class="shot-more">
            <button class="ghost-btn" :disabled="shotLoadingMore" @click="loadMoreShots">
              {{ shotLoadingMore ? t("settings.shotLoading") : t("settings.shotLoadMore") }}
            </button>
          </div>
        </template>
      </div>
    </div>

    <!-- ============ v1.2.10：权限中心 ============
         为什么放在截图卡片之后：本应用在 macOS 上真正需要的唯一授权就是「屏幕录制」，
         且它只服务截图取字与窗口标题 —— 与截图功能语义相邻，用户排查截图问题时
         视线不用跳。
         为什么只读展示 + 手动刷新、不做自动轮询：Rust 的 check_permissions 在未放行时
         会跑 codesign / xattr 两个子进程做诊断（见 commands/platform/macos.rs 的警告），
         接进轮询等于每 2 秒 fork 两个进程。用户在系统设置里改完授权后点一下「刷新」即可。 -->
    <div class="setting-card">
      <div class="card-head">
        <div class="head-icon icon-green">
          <AppIcon name="lock" :size="20" />
        </div>
        <div class="head-text">
          <h3>{{ t("settings.permissionCenterTitle") }}</h3>
          <p>{{ t("settings.permissionCenterDesc") }}</p>
        </div>
        <button v-if="IS_MAC" class="ghost-btn perm-refresh" @click="loadPermissions">
          <AppIcon name="refresh" :size="14" />
          {{ t("settings.permRefresh") }}
        </button>
      </div>
      <div class="card-body">
        <!-- 非 macOS 抓屏不需要 TCC 授权，卡片只留一句结论，不摆永远显示「已授权」的假开关 -->
        <p v-if="!IS_MAC" class="field-hint perm-hint-nomac">{{ t("settings.permNotRequired") }}</p>
        <template v-else>
        <!-- 屏幕录制 -->
        <div class="perm-row">
          <div class="perm-main">
            <span class="perm-label">{{ t("settings.permScreenCapture") }}</span>
            <span
              v-if="permissions"
              class="perm-badge"
              :class="permissions.screen_capture ? 'ok' : 'bad'"
            >
              <i class="perm-dot" />{{
                permissions.screen_capture ? t("settings.permGranted") : t("settings.permNotGranted")
              }}
            </span>
            <span v-else class="perm-badge pending">…</span>
            <div class="perm-actions">
              <button
                v-if="!permissions || !permissions.screen_capture"
                class="ghost-btn"
                @click="onPermGrantScreen"
              >
                {{ t("settings.permGrant") }}
              </button>
              <button class="ghost-btn" @click="onPermResetRestart">
                {{ t("settings.permResetRestart") }}
              </button>
            </div>
          </div>
          <p class="field-hint">{{ t("settings.permScreenCaptureDesc") }}</p>
        </div>
        <!-- 辅助功能 -->
        <div class="perm-row">
          <div class="perm-main">
            <span class="perm-label">{{ t("settings.permAccessibility") }}</span>
            <span
              v-if="permissions"
              class="perm-badge"
              :class="permissions.accessibility ? 'ok' : 'bad'"
            >
              <i class="perm-dot" />{{
                permissions.accessibility ? t("settings.permGranted") : t("settings.permNotGranted")
              }}
            </span>
            <span v-else class="perm-badge pending">…</span>
            <div class="perm-actions">
              <button class="ghost-btn" @click="onPermOpenAccessibility">
                {{ t("settings.permOpenSettings") }}
              </button>
            </div>
          </div>
          <p class="field-hint">{{ t("settings.permAccessibilityDesc") }}</p>
        </div>
        <!-- 系统设置里开关开着、这里仍显示未授权 —— 这是 ad-hoc 签名的典型症状，
             不解释用户只会反复点「去授权」而不理解为什么没用。 -->
        <p v-if="permissions && !permissions.screen_capture" class="field-hint perm-hint-warn">
          {{ t("settings.permRevokedHint") }}
        </p>
        </template>
      </div>
    </div>

    <!-- ============ 设备 ID ============ -->
    <div class="setting-card">
      <div class="card-head">
        <div class="head-icon icon-blue">
          <AppIcon name="keyRound" :size="20" />
        </div>
        <div class="head-text">
          <h3>{{ t("settings.deviceIdTitle") }}</h3>
          <p>{{ t("settings.deviceIdDesc") }}</p>
        </div>
      </div>
      <div class="card-body">
        <div class="id-bar">
          <code class="id-mono">{{ settings.device_id || "—" }}</code>
          <div class="id-actions">
            <button class="ghost-btn" @click="copy(settings.device_id || '')">
              <AppIcon name="copy" :size="14" /> {{ t("common.copy") }}
            </button>
          </div>
        </div>
      </div>
    </div>

    <!-- ============ 备份与多设备合并 ============ -->
    <div class="setting-card">
      <div class="card-head">
        <div class="head-icon icon-green">
          <AppIcon name="database" :size="20" />
        </div>
        <div class="head-text">
          <h3>{{ t("settings.backupMerge") }}</h3>
          <p>{{ t("settings.backupHint") }}</p>
        </div>
      </div>
      <div class="card-body">
        <div class="btn-row">
          <button class="ghost-btn" @click="onExport">
            <AppIcon name="download" :size="14" /> {{ t("settings.export") }}
          </button>
          <button class="ghost-btn" @click="pickImport">
            <AppIcon name="upload" :size="14" /> {{ t("settings.import") }}
          </button>
          <input
            ref="fileInput"
            type="file"
            accept="application/json,.json"
            hidden
            @change="onImport"
          />
        </div>

        <!-- v0.7.2 本地自动备份（参考微信桌面版：本地落盘 JSON，用户自行拷到云盘即"云备份"） -->
        <div class="sub-zone">
          <h4><AppIcon name="clock" :size="14" /> {{ t("settings.autoBackup") }}</h4>
          <p class="field-hint" v-html="t('settings.autoBackupDesc')" />
          <div class="form-row row">
            <label>{{ t("settings.autoBackupOn") }}</label>
            <label class="toggle-switch" :class="{ on: backupConfig.enabled }">
              <input
                type="checkbox"
                :checked="backupConfig.enabled"
                @change="onToggleAutoBackup"
              />
              <span class="toggle-slider" />
              <span class="toggle-state">
                {{ backupConfig.enabled ? t("settings.autostartOn") : t("settings.autostartOff") }}
              </span>
            </label>
          </div>

          <div class="form-row">
            <label>{{ t("settings.backupPath") }}</label>
            <div class="path-row">
              <input
                v-model="backupConfig.path"
                type="text"
                class="text-input"
                :placeholder="t('settings.backupPathPh')"
                readonly
              />
              <button class="ghost-btn" @click="chooseBackupFolder">
                <AppIcon name="folder" :size="14" /> {{ t("settings.chooseFolder") }}
              </button>
            </div>
            <p class="field-hint" v-html="t('settings.backupPathHint')" />
          </div>

          <div class="form-row">
            <label>{{ t("settings.backupKeep") }}</label>
            <input
              v-model.number="backupConfig.keep_days"
              type="number"
              min="1"
              max="3650"
              class="text-input narrow"
              @change="persistBackupConfig"
            />
            <p class="field-hint" v-html="t('settings.backupKeepHint')" />
          </div>

          <div class="form-row row between">
            <span class="last-backup">
              {{ t("settings.backupLast") }}
              <b>{{ backupConfig.last_date || t("settings.backupNever") }}</b>
            </span>
            <button class="primary-btn" :disabled="backupBusy" @click="onBackupNow">
              <AppIcon name="download" :size="14" />
              {{ backupBusy ? t("settings.backingUp") : t("settings.backupNow") }}
            </button>
          </div>
        </div>

        <div class="sub-zone">
          <h4><AppIcon name="tool" :size="14" /> {{ t("settings.diag") }}</h4>
          <p class="field-hint" v-html="t('settings.diagHint')" />
          <p class="field-hint" v-if="logSize !== null" v-html="t('settings.logSize', { size: formatBytes(logSize) })" />
          <div class="btn-row">
            <button class="ghost-btn" @click="exportLogs">
              <AppIcon name="clipboard" :size="14" /> {{ t("settings.exportLogs") }}
            </button>
            <button class="ghost-btn" @click="revealLogDir">
              <AppIcon name="folder" :size="14" /> {{ t("settings.openLogDir") }}
            </button>
            <button class="ghost-btn" @click="refreshLogSize">
              <AppIcon name="refresh" :size="14" /> {{ t("settings.refresh") }}
            </button>
          </div>
        </div>
      </div>
    </div>

    <!-- ============ 桌宠（开关 + 操作 + 皮肤 + 编辑器）============ -->
    <div class="setting-card pet-card">
      <div class="card-head">
        <div class="head-icon icon-pink">
          <AppIcon name="paw" :size="20" />
        </div>
        <div class="head-text">
          <h3>{{ t("pet.settings.title") }}</h3>
          <p>{{ t("pet.settings.enabledDesc") }}</p>
        </div>
      </div>
      <div class="card-body">
        <div class="form-row row">
          <label>{{ t("pet.settings.enabled") }}</label>
          <label class="toggle-switch" :class="{ on: petStore.enabled }">
            <input
              type="checkbox"
              :checked="petStore.enabled"
              @change="onTogglePet(($event.target as HTMLInputElement).checked)"
            />
            <span class="toggle-slider" />
            <span class="toggle-state">
              {{ petStore.enabled ? t("pet.settings.on") : t("pet.settings.off") }}
            </span>
          </label>
        </div>

        <div class="btn-row" v-if="petStore.enabled">
          <button class="ghost-btn" @click="onShowPet">
            <AppIcon name="eye" :size="14" /> {{ t("pet.settings.open") }}
          </button>
          <button class="ghost-btn" @click="onHidePet">
            <AppIcon name="eyeOff" :size="14" /> {{ t("pet.settings.close") }}
          </button>
          <button class="ghost-btn" @click="onResetPetPos">
            <AppIcon name="rotateCcw" :size="14" /> {{ t("pet.settings.resetPos") }}
          </button>
        </div>

        <!-- 皮肤 -->
        <div class="sub-zone">
          <h4><AppIcon name="sparkles" :size="14" /> {{ t("pet.settings.skinTitle") }}</h4>
          <p class="field-hint">{{ t("pet.settings.skinDesc") }}</p>
          <div class="pet-skin-grid">
            <button
              v-for="s in skinList"
              :key="s.id"
              type="button"
              class="pet-skin-tile"
              :class="{ 'is-active': s.id === activeSkinId }"
              :aria-pressed="s.id === activeSkinId"
              @click="pickSkin(s.id)"
            >
              <div class="pet-skin-head">
                <!-- v1.2.2: emoji（🎁/🐾）统一替换为 AppIcon 矢量图标；描述文案走 i18n（pet.skins.<id>.desc），英文模式不再显示中文 -->
                <AppIcon
                  :name="skinIcon(s.id)"
                  :size="18"
                  class="pet-skin-icon"
                  :aria-hidden="true"
                />
                <span class="pet-skin-name">{{ s.name }}</span>
              </div>
              <div class="pet-skin-desc">{{ skinDesc(s) }}</div>
            </button>
          </div>
        </div>

        <div class="card-actions" v-if="petStore.enabled">
          <button class="ghost-btn" @click="showEditor = true">
            <AppIcon name="penTool" :size="14" />
            {{ t("pet.settings.editor") }}
          </button>
        </div>
      </div>
    </div>

    <!-- 桌宠编辑器（Teleport 到 body，避免被 card 裁剪） -->
    <Teleport to="body">
      <PetSpriteEditor :visible="showEditor" @close="showEditor = false" />
    </Teleport>

    <!-- ============ 检查更新 ============ -->
    <div class="setting-card">
      <div class="card-head">
        <div class="head-icon icon-blue">
          <AppIcon name="refresh" :size="20" />
        </div>
        <div class="head-text">
          <h3>{{ t("settings.updateTitle") }}</h3>
          <p>{{ t("settings.updateDesc") }}</p>
        </div>
      </div>
      <div class="card-body">
        <div class="btn-row btn-row-end">
          <button class="primary-btn outline" :disabled="checking" @click="onCheckUpdate">
            <AppIcon name="refresh" :size="14" />
            {{ checking ? t("settings.checking") : t("settings.checkUpdate") }}
          </button>
        </div>
        <p v-if="updateResult" class="field-hint" :class="{ outdated: updateResult.has_update }">
          <template v-if="updateResult.has_update">
            {{ t("settings.foundNew") }} <b>v{{ updateResult.latest }}</b>（当前 v{{ updateResult.current }}）
            <button class="link-btn" @click="goDownload(updateResult.url)">
              {{ t("settings.goDownload") }}
            </button>
          </template>
          <template v-else>
            {{ t("settings.upToDate", { current: updateResult.current }) }}
          </template>
        </p>
      </div>
    </div>

    <!-- ============ 关于 ============ -->
    <div class="setting-card">
      <div class="card-head">
        <div class="head-icon icon-blue">
          <AppIcon name="info" :size="20" />
        </div>
        <div class="head-text">
          <h3>{{ t("settings.about") }}</h3>
          <p>{{ t("settings.aboutDesc") }}</p>
        </div>
      </div>
      <div class="card-body about-list">
        <div class="meta-row">
          <span class="meta-label">{{ t("settings.appVersion") }}</span>
          <span class="meta-value mono">{{ version }}</span>
        </div>
        <div class="meta-row">
          <span class="meta-label">{{ t("settings.deviceId") }}</span>
          <span class="meta-value mono">{{ settings.device_id || "—" }}</span>
        </div>
        <div class="meta-row">
          <span class="meta-label">{{ t("settings.storageLabel") }}</span>
          <span class="meta-value">{{ t("settings.storage") }}</span>
        </div>
      </div>
    </div>

    <!-- ============ 危险区 ============ -->
    <div class="setting-card danger-card">
      <div class="card-head">
        <div class="head-icon icon-red">
          <AppIcon name="warning" :size="20" />
        </div>
        <div class="head-text">
          <h3>{{ t("settings.danger") }}</h3>
          <p>{{ t("settings.dangerZoneDesc") }}</p>
        </div>
      </div>
      <div class="card-body">
        <div class="btn-row">
          <button class="danger-btn" @click="confirmCleanAll">
            <AppIcon name="trash" :size="14" />
            {{ t("settings.cleanOld", { days: retention }) }}
          </button>
          <button class="danger-btn" @click="openDevicePrune">
            <AppIcon name="trash" :size="14" /> {{ t("settings.pruneByDevice") }}
          </button>
        </div>
        <p class="danger-hint">{{ t("settings.dangerHint") }}</p>
      </div>
    </div>

    <!-- ============ 通用反馈/确认弹窗 ============ -->
    <Modal
      v-model="alertOpen"
      :type="alertType"
      :title="alertTitle"
      :message="alertMsg"
      :confirm-text="alertConfirmText || t('common.confirm')"
      :cancel-text="alertType === 'info' ? '' : t('common.cancel')"
      width="420px"
      @confirm="onAlertConfirm"
    >
      <!--
        截图缺「屏幕录制」权限时，footer 多一个「重启应用」。
        原因：macOS 的 TCC 授权**只对新启动的进程生效** —— 用户在系统设置里把开关打开后，
        当前进程依然拿不到授权，现象就是「系统设置里明明是开着的，应用却一直说没权限」。
        这里给一键重启，用户不必自己 Cmd+Q 再打开（多数人不会这么做，于是卡成死循环）。

        ⚠️ 覆盖 footer 后，Modal 自带的按钮不再渲染 —— 下面 else 分支必须复刻它原本的
        行为（confirm/warn 显示「取消 + 确定」，info 只显示「确定」），否则弹窗会没有按钮。
      -->
      <template #footer>
        <template v-if="alertPermRestart">
          <button class="modal-btn cancel" @click="alertOpen = false">{{ t("common.cancel") }}</button>
          <button class="modal-btn cancel" @click="onAlertRestart">{{ t("settings.shotPermFix") }}</button>
          <button class="modal-btn primary" @click="onAlertConfirm">
            {{ alertConfirmText || t("common.confirm") }}
          </button>
        </template>
        <template v-else>
          <button v-if="alertType !== 'info'" class="modal-btn cancel" @click="alertOpen = false">
            {{ t("common.cancel") }}
          </button>
          <button class="modal-btn primary" @click="onAlertConfirm">
            {{ alertConfirmText || t("common.confirm") }}
          </button>
        </template>
      </template>
    </Modal>

    <!-- ============ 导出成功后的弹窗 ============ -->
    <Modal
      v-model="exportDialogOpen"
      type="info"
      :title="t('settings.exportedBackup')"
      :message="t('settings.exportedMsg', { path: exportPath })"
      :confirm-text="t('common.confirm')"
      cancel-text=""
      width="520px"
    >
      <template #footer>
        <button class="modal-btn cancel" @click="reveal(exportPath)">{{ t("common.revealInFM") }}</button>
        <button class="modal-btn cancel" @click="copy(exportPath)">{{ t("common.copyPath") }}</button>
        <button class="modal-btn primary" @click="exportDialogOpen = false">{{ t("common.close") }}</button>
      </template>
    </Modal>

    <!-- ============ 日志导出成功后的弹窗 ============ -->
    <Modal
      v-model="logExportDialogOpen"
      type="info"
      :title="t('settings.logExported')"
      :message="t('settings.logExportedMsg', { path: logExportPath })"
      :confirm-text="t('common.confirm')"
      cancel-text=""
      width="520px"
    >
      <template #footer>
        <button class="modal-btn cancel" @click="reveal(logExportPath)">{{ t("common.revealInFM") }}</button>
        <button class="modal-btn cancel" @click="copy(logExportPath)">{{ t("common.copyPath") }}</button>
        <button class="modal-btn primary" @click="logExportDialogOpen = false">{{ t("common.close") }}</button>
      </template>
    </Modal>

    <!-- ============ 按设备清理弹窗 ============ -->
    <Modal
      v-model="pruneDialogOpen"
      type="warn"
      :title="t('settings.pruneTitle')"
      :message="t('settings.pruneMsg')"
      :confirm-text="selectedDeviceIds.length === 0 ? t('settings.pruneAllConfirm') : t('settings.pruneNConfirm', { n: selectedDeviceIds.length })"
      :cancel-text="t('common.cancel')"
      width="640px"
      @confirm="onConfirmPruneByDevice"
    >
      <div class="device-list">
        <div v-if="deviceStats.length === 0" class="empty">
          {{ t("settings.loading") }}
        </div>
        <div v-else>
          <label
            v-for="d in deviceStats"
            :key="d.device_id"
            class="device-row"
            :class="{ checked: selectedDeviceIds.includes(d.device_id) }"
          >
            <input
              type="checkbox"
              :value="d.device_id"
              v-model="selectedDeviceIds"
            />
            <div class="device-info">
              <div class="device-name">
                {{ d.device_name || d.device_id }}
                <span v-if="d.device_id === settings.device_id" class="self-tag">{{ t("settings.selfTag") }}</span>
                <span
                  v-else-if="!d.device_name || d.device_name === d.device_id"
                  class="default-tag"
                  :title="t('settings.unnamedTip', '该设备没有设置名称（可能是从旧版备份导入的数据）')"
                >{{ t("settings.unnamed") }}</span>
              </div>
              <div class="device-meta">
                <span class="mono">{{ d.device_id }}</span>
                <span>·</span>
                <span>{{ formatSeconds(d.total_seconds) }}</span>
                <span>·</span>
                <span>{{ t("settings.sessions", { n: d.session_count }) }}</span>
                <span v-if="d.earliest_date">·</span>
                <span v-if="d.earliest_date">{{ d.earliest_date }} → {{ d.latest_date }}</span>
              </div>
            </div>
          </label>
          <p class="field-hint" v-html="t('settings.pruneHint', { days: retention })" />
        </div>
      </div>
    </Modal>
  </div>
</template>

<script setup lang="ts">
// 设置页（v0.6.2-beta.19 重构）
// - 视觉：按"图标 + 标题/描述 + 控件"卡片化重组（原 card 列表 → 多功能分卡）
// - 功能：100% 保留，script 段内方法、状态、副作用与原实现一致
// - 关键变更：
//   1. 语言 radio 改成"胶囊式"组件（截图风格），仍走 i18n.setLocale
//   2. 设备 ID 抽出独立卡片，仅"复制"按钮（截图里的"重置"按钮非原功能，移除避免调用未注册的 reset_device_id）
//   3. 备份与诊断合并到一张卡（"数据库"图标）
//   4. 桌宠卡含 开关 / 操作按钮 / 皮肤 / 编辑器入口
//   5. 危险区改为独立卡片，警示色
//
// ── 修改历史 ────────────────────────────────────────────────
// 2026-10-26 @v1.2.10：新增「权限中心」卡片（截图卡片之后、设备 ID 之前）。
//   · 背景：设置页 grep checkPermissions 零命中 —— 用户想主动确认「到底授权了没有」
//     在界面上无路可走，只能靠顶部横幅出现/消失猜；横幅还同时承担通知与操作两个职责。
//   · 本版把「状态展示 + 全部操作按钮」收口到这张卡，横幅此后只承担通知。
//   · 为什么只 onMounted 查一次 + 手动刷新，不做轮询：后端 check_permissions 在未放行时
//     会跑 codesign / xattr 两个子进程做诊断（commands/platform/macos.rs 有明确警告），
//     接进轮询等于每 2 秒 fork 两个进程。用户改完系统设置点「刷新状态」即可。
//   · 非 macOS：抓屏不需要 TCC 授权，卡片只留一句「本平台无需授权」，不摆假开关。
// 2026-10-23 @v1.2.7：修「进设置页立刻点开关，改动会被晚到的加载覆盖」
//   · 症状：进设置页后马上点「显示网速」，光标闪一下就弹回原值，用户以为点不动。
//   · 真机复现（CDP + debug 构建，进入页面 25–46ms 内点击）：
//       +3ms checked=false（用户点成功）→ +858ms checked=true（被冲回）
//       且点击瞬间 disabled=true，点击被浏览器直接吞掉，连 change 都不发。
//   · 根因两条，都在「加载晚于交互」这一个窗口里：
//     ① 加载方向：`ref.value = await load()` 整体替换，冲掉窗口期内用户写值。
//     ② 提交方向：提交时发整个本地对象，而本地初值是**默认值**（enabled:false），
//        于是把后端原本 true 的字段一并写坏 —— 这是**数据损坏**，不只是显示问题。
//   · 修法：改用「权威基线 + 用户差量」模型（statusBarBaseline/statusBarDirty、
//     shotConfigBaseline/shotConfigDirty）：
//       加载返回 → { ...loaded, ...delta }，只让改过的键覆盖权威值
//       提交     → { ...baseline, ...delta }，不把未加载的默认值当真值发出去
//   · ⚠️ 踩过的坑：第一版写成 `{ ...loaded, ...statusBarConfig.value }`（整对象
//     冒充差量），实测把 show_disk / float_enabled / float_glass / enabled 全打成
//     false 并写回 DB。修 lost update 必须有**真正的差量来源**，否则就是反向丢更新。
//   · ⚠️ 注意：这是「加载晚于交互」的通用坑。以后新增 `ref.value = await load()`
//     式初始化，若该 ref 也能被用户即时编辑，必须沿用同一保护。
// 2026-10-22 @v1.2.5：液态玻璃（v1.2.2）之后无改动，仅体积优化涉及构建产物。

import { ref, onMounted, onBeforeUnmount, computed, nextTick } from "vue";
import { useI18n } from "vue-i18n";
import { getVersion } from "@tauri-apps/api/app";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import Modal from "../components/Modal.vue";
import AppIcon from "../components/AppIcon.vue";
import { tracker } from "../api/tracker";
import { screenshot, DEFAULT_SCREENSHOT_CONFIG } from "../api/screenshot";
import { i18n, setLocale, type Locale } from "../i18n";
import type {
  BackupConfig,
  DeviceStats,
  SettingsOut,
  StatusBarConfig,
  UpdateInfo,
  PermissionStatus,
  ScreenshotConfig,
  ScreenshotOut,
  OcrEngineInfo,
  OcrEngineKind,
  FloatGlassTier,
} from "../types";
import { formatDuration } from "../utils/format";
import { petStore } from "../pet/stores/petStore";
import PetSpriteEditor from "../pet/components/PetSpriteEditor.vue";
import { skinRegistry } from "../pet/skins/registry";
import "../pet/skins";
import { log } from "@/lib/logger";

const { t } = useI18n();

// 桌宠编辑器显示状态
const showEditor = ref(false);

// v0.6.2-beta.2：皮肤选择器
const skinList = computed(() => skinRegistry.list());
const activeSkinId = computed(() => skinRegistry.active().id);
function pickSkin(id: string): void {
  skinRegistry.setActive(id);
}

/**
 * v1.2.2：皮肤卡片图标（替代 emoji 🎁/🐾）。
 * manifest 未声明 icon 字段，这里按 id 映射；新皮肤默认给 paw。
 */
function skinIcon(id: string): string {
  return id === 'popmart-3d' ? 'gift' : 'paw';
}

/**
 * v1.2.2：皮肤描述本地化 —— manifest.description 是硬编码中文，
 * 英文模式下改读 i18n（pet.skins.<id>.desc），缺 key 时回退 manifest 原文。
 * 右键菜单（PetContextMenu）的 tooltip 用同一套 key，两处文案保持一致。
 */
function skinDesc(s: { id: string; description?: string; name: string }): string {
  return t(`pet.skins.${s.id}.desc`, s.description ?? s.name);
}

// v0.6.2-beta.3：跨窗口皮肤同步
let unlistenSkin: (() => void) | null = null;
let unlistenPetEnabled: (() => void) | null = null;
// v0.7.6（2026-09-10）：托盘右键快捷开关 ↔ 设置页双向同步
let unlistenStatusBarCfg: (() => void) | null = null;
// v0.9.2：截图完成 → 刷新历史列表（注销函数需保存，防监听器泄漏）
let unlistenShotDone: (() => void) | null = null;
// 单一 onMounted 汇聚全部初始化（v1.1.1 合并，原分散在 4 处）：
// ① 跨窗口事件监听（桌宠皮肤/开关、托盘状态栏勾选）
// ② 截图配置 + 取字引擎 + 截图历史 + screenshot-done 监听
// ③ 版本 + 通用设置 + 备份配置 + 状态栏配置
// ④ 日志大小
onMounted(async () => {
  try {
    unlistenSkin = await listen("pet-skin-changed", () => {
      skinRegistry.reloadActive();
    });
  } catch (e) {
    log.error("[Settings] 监听 pet-skin-changed 失败", e);
  }
  try {
    unlistenPetEnabled = await listen("pet-enabled-changed", () => {
      petStore.reload();
    });
  } catch (e) {
    log.error("[Settings] 监听 pet-enabled-changed 失败", e);
  }
  // 托盘菜单勾选后后端 emit 此事件，设置页卡片即时反映（避免两处状态不一致）
  // v1.2.7：仍要保留「用户刚改、尚在提交中」的差量 —— 托盘事件可能在用户点击
  // 之后、IPC 返回之前到达，此时整体覆盖会把乐观更新的值冲掉（同上 lost update）。
  try {
    unlistenStatusBarCfg = await listen<StatusBarConfig>(
      "status-bar-config-changed",
      (e) => {
        statusBarBaseline = e.payload;
        statusBarConfig.value =
          statusBarDirty.size > 0
            ? { ...e.payload, ...statusBarDelta() }
            : e.payload;
      },
    );
  } catch (e) {
    log.error("[Settings] 监听 status-bar-config-changed 失败", e);
  }
  void initScreenshot();
  void initSettings();
  void refreshLogSize();
  // v1.2.10：权限状态只查这一次（+ 卡片上的手动刷新），不轮询 —— 见 loadPermissions 说明
  void loadPermissions();
});
onBeforeUnmount(() => {
  if (unlistenSkin) unlistenSkin();
  if (unlistenPetEnabled) unlistenPetEnabled();
  if (unlistenStatusBarCfg) unlistenStatusBarCfg();
  if (unlistenShotDone) unlistenShotDone();
});

// 语言切换
function onLangChange(e: Event) {
  setLocale((e.target as HTMLInputElement).value as Locale);
}

// 桌宠控制
async function onTogglePet(checked: boolean) {
  petStore.setEnabled(checked);
  if (checked) {
    invoke("create_pet_window")
      .then(() => invoke("show_pet_window"))
      .then(() => {
        const pos = petStore.position;
        return invoke("move_pet_window", { x: pos.x, y: pos.y });
      })
      .catch((err) => {
        log.error("[pet] 切换桌宠失败", err);
        showAlert("warn", t("common.error"), String(err));
      });
  } else {
    invoke("hide_pet_window").catch((err) => {
      log.error("[pet] 隐藏桌宠失败", err);
    });
  }
}
async function onShowPet() {
  try {
    await invoke("create_pet_window");
    await invoke("show_pet_window");
    const pos = petStore.position;
    await invoke("move_pet_window", { x: pos.x, y: pos.y });
  } catch (err) {
    log.error("[pet] 显示桌宠失败", err);
    showAlert("warn", t("common.error"), String(err));
  }
}
async function onHidePet() {
  // v0.9.0：与右键菜单「隐藏桌宠」语义统一——不仅隐藏窗口，同时关闭桌宠开关
  // （翻转 petStore.enabled 并广播 pet-enabled-changed），确保菜单 / 设置页 / 桌宠三处开关状态一致。
  petStore.setEnabled(false);
  try {
    await invoke("hide_pet_window");
  } catch (err) {
    log.error("[pet] 隐藏桌宠失败", err);
  }
}
function onResetPetPos() {
  const screenW = window.screen.width;
  const screenH = window.screen.height;
  petStore.setPosition(screenW - 200, screenH - 240);
  const pos = petStore.position;
  invoke("move_pet_window", { x: pos.x, y: pos.y }).catch(() => {});
}

const settings = ref<SettingsOut>({
  device_id: "",
  device_name: "",
  idle_threshold: 300,
  data_retention_days: 365,
  sample_interval: 2,
  autostart: false,
});

const version = ref("");
const checking = ref(false);
const updateResult = ref<UpdateInfo | null>(null);

const deviceName = ref("");
const idleMin = ref(5);
const retention = ref(365);
const autostart = ref(false);

const fileInput = ref<HTMLInputElement>();

// ============ v0.7.2 本地自动备份（微信桌面版式）============
const backupConfig = ref<BackupConfig>({
  enabled: false,
  path: "",
  keep_days: 30,
  last_date: "",
});
const backupBusy = ref(false);

// v1.2.7：与状态栏/截图配置同源 —— `loadBackupConfig` 是整体替换，而备份开关
// 是**即时保存**（onToggleAutoBackup 立刻写后端）。若加载晚于点击，UI 会被
// 回滚成旧值，而后端已是新值 → 两边不一致，重进页面又「自己变回去」。
// 故同样记「权威基线 + 用户差量」，只让改过的键覆盖加载值。
let backupBaseline: BackupConfig | null = null;
const backupDirty = new Set<keyof BackupConfig>();

async function loadBackupConfig() {
  try {
    const loaded = await tracker.getBackupConfig();
    backupBaseline = loaded;
    if (backupDirty.size > 0) {
      const delta: Record<string, unknown> = {};
      for (const k of backupDirty) delta[k as string] = backupConfig.value[k];
      backupConfig.value = { ...loaded, ...delta } as BackupConfig;
      log.info(
        `[Settings] 备份配置加载晚于用户操作，已保留改动键：${[...backupDirty].join(",")}`,
      );
    } else {
      backupConfig.value = loaded;
    }
  } catch {
    /* 浏览器预览模式忽略 */
  }
}

async function onToggleAutoBackup(e: Event) {
  const next = (e.target as HTMLInputElement).checked;
  backupConfig.value.enabled = next; // 乐观更新
  backupDirty.add("enabled");
  try {
    await persistBackupConfig();
    if (backupBaseline) backupBaseline = { ...backupConfig.value };
    backupDirty.delete("enabled");
  } catch {
    // persistBackupConfig 内部已提示；dirty 保留，等下一次加载以用户意图为准
  }
}

async function chooseBackupFolder() {
  try {
    const selected = await open({ directory: true, multiple: false });
    if (typeof selected === "string" && selected) {
      backupConfig.value.path = selected;
      backupDirty.add("path"); // v1.2.7：同 onToggleAutoBackup，防被晚到的加载覆盖
      await persistBackupConfig();
      if (backupBaseline) backupBaseline = { ...backupConfig.value };
      backupDirty.delete("path");
    }
  } catch (err) {
    showAlert("warn", t("settings.openFailed"), t("settings.openFailedMsg", { err: err instanceof Error ? err.message : String(err) }));
  }
}

async function persistBackupConfig() {
  try {
    await tracker.saveBackupConfig({
      enabled: backupConfig.value.enabled,
      path: backupConfig.value.path,
      keepDays: backupConfig.value.keep_days,
    });
  } catch (err) {
    showAlert("warn", t("settings.saveFailed"), t("settings.saveFailedMsg", { err: err instanceof Error ? err.message : String(err) }));
  }
}

async function onBackupNow() {
  backupBusy.value = true;
  try {
    const res = await tracker.runBackupNow();
    exportPath.value = res.path;
    exportDialogOpen.value = true;
  } catch (err) {
    showAlert("warn", t("settings.exportFailed"), t("settings.exportFailedMsg", { err: err instanceof Error ? err.message : String(err) }));
  } finally {
    backupBusy.value = false;
  }
}

// ============ 通用弹窗 ============
const alertOpen = ref(false);
const alertType = ref<"info" | "confirm" | "warn">("info");
const alertTitle = ref("");
const alertMsg = ref("");
/**
 * 自定义确认按钮文案（留空则用 i18n 默认「确定」）。
 * v0.9.1：截图权限缺失时按钮要显示「打开系统设置」——比「确定」明确得多，
 * 用户知道点下去会发生什么（跳系统设置），而不是以为点完问题就解决了。
 */
const alertConfirmText = ref("");
/**
 * 弹窗 footer 是否多给一个「重启应用」按钮。
 * 只有「截图缺屏幕录制权限」这一种场景需要：TCC 新授权必须重启进程才生效。
 */
const alertPermRestart = ref(false);
let pendingConfirm: (() => void | Promise<void>) | null = null;
function showAlert(
  type: "info" | "confirm" | "warn",
  title: string,
  msg: string,
  onConfirm?: () => void | Promise<void>,
  confirmText?: string,
  /**
   * 是否在 footer 追加「重启应用」（默认否，显式传 true 才出现）。
   * 默认重置为 false，避免上一次带重启按钮的弹窗把状态残留给后续普通提示。
   */
  showRestart = false
) {
  alertType.value = type;
  alertTitle.value = title;
  alertMsg.value = msg;
  alertConfirmText.value = confirmText ?? "";
  alertPermRestart.value = showRestart;
  pendingConfirm = onConfirm ?? null;
  alertOpen.value = true;
}

/**
 * 一键「重置授权 + 重启」（macOS）。
 *
 * 为什么是两步而不是单纯重启：系统里那条屏幕录制授权记录可能指向**旧版本的代码签名指纹**
 * （未签名的包每次升级都会这样），这种脏记录在系统设置界面里清不掉 —— 用户把开关关了再开、
 * 把条目按「−」删了再加，写进去的往往还是旧身份。所以先 `tccutil reset` 清干净，再重启进程。
 * 两步缺一不可：只清不重启，新授权对当前进程无效；只重启，脏记录还在。
 *
 * 成功时进程会直接退出，restartApp 的 await 不会返回；catch 兜的是 IPC 本身失败。
 */
async function onAlertRestart() {
  alertOpen.value = false;
  try {
    await tracker.resetScreenCapturePermission();
  } catch (err) {
    // 重置失败不阻断重启：tccutil 在个别系统上需要 sudo，此时至少让「重启」这条路还能走
    log.warn("[shot] 重置屏幕录制授权失败:", err);
  }
  try {
    await tracker.restartApp();
  } catch (err) {
    showAlert("warn", t("settings.shotTitle"), err instanceof Error ? err.message : String(err));
  }
}
function onAlertConfirm() {
  // v0.9.5 修复：footer slot 覆盖了 Modal 自带按钮后，自带按钮的 close() 行为也一并丢失
  //（v0.9.4 起「确定」点了没反应，只能 X / 遮罩 / Esc 关闭）。这里补上关闭动作。
  // 注意顺序：先关再跑回调 —— 回调里常会再 showAlert 弹「结果弹窗」
  //（如确认清理 → 弹「已清理」），若先跑回调，后执行的 alertOpen=false 会把结果弹窗误关。
  alertOpen.value = false;
  if (pendingConfirm) {
    const cb = pendingConfirm;
    pendingConfirm = null;
    void cb();
  }
}

// ============ 导出/按设备清理 ============
const exportDialogOpen = ref(false);

// ============ v0.7.6：状态栏配置（总开关 + 子项）============
// v0.7.7（2026-09-10）：内存与磁盘采样已补齐三平台，不再按平台禁用子项
//（旧版用于禁用内存项的 isMac 判据与后端 MEMORY_SUPPORTED 编译期常量一并作废）。
// v0.7.11（2026-09-13）：三端统一只用「悬浮指标条」——macOS 原菜单栏文字指标已从
// Rust 侧整体移除，故不再需要 isMac 判据来切换平台说明文案（原 isMac 常量已删除）。
const statusBarConfig = ref<StatusBarConfig>({
  enabled: false,
  show_cpu: true,
  show_mem: true,
  show_disk: false,
  show_net: true,
  float_enabled: false,
  // v0.8.2：截图总开关透传（只读视图，权威源在 screenshot 模块；此处仅占位满足类型）
  screenshot_enabled: true,
  // v1.2.2：液态玻璃开关（默认关，首次 loadStatusBarConfig 由后端权威值覆盖）
  float_glass: false,
});

// ===== v1.2.2（2026-10-21）：液态玻璃能力探测 =====
// `supported=false` 时开关置灰，并换用「不支持」文案（避免"开了没效果"的死开关）。
// 探测在 loadStatusBarConfig 里一并发起（同一卡片、同一次进入设置页）。
// 默认 false：探测回来前先按不可用渲染，宁可短暂置灰也不让用户点到一个假开关。
const glassSupported = ref(false);
const glassReason = ref("");
// Windows 的 tier 也是 "acrylic"，但实测系统材质对无边框窗口不透光，
// 实际效果是「高透光但无模糊」——文案必须如实说明，不能让用户以为有模糊。
const glassTier = ref<FloatGlassTier>("none");
async function loadGlassSupport() {
  try {
    const s = await tracker.getFloatGlassSupport();
    glassSupported.value = s.supported;
    glassTier.value = s.tier;
    // 直接把后端返回的能力说明当 tooltip 用（英文技术描述，不另做 i18n，
    // 它是给排障看的，不是给普通用户读的）
    glassReason.value = s.reason;
    // 能力不支持但配置里还开着（典型场景：从 macOS 26 机器把配置同步到了旧系统）
    // → 前端不静默关掉用户配置（用户可能只是临时在别的机器上看），
    //   仅置灰展示；Rust 侧 apply 失败会记 warn，浮窗自动退化 CSS 兜底。
  } catch (err) {
    log.warn("[Settings] 液态玻璃能力探测失败", err);
    glassSupported.value = false;
    glassTier.value = "none";
    glassReason.value = "";
  }
}

// 说明文案三态：不支持 → 不支持；Windows(acrylic) → 明说无模糊；其余（macOS）→ 完整描述
const glassHint = computed(() => {
  if (!glassSupported.value) return t("settings.statusBarGlassUnsupported");
  if (glassTier.value === "acrylic") return t("settings.statusBarGlassWindows");
  return t("settings.statusBarGlassHint");
});

// ── v1.2.7：修「进设置页立刻点状态栏开关，会被晚到的加载覆盖」──
//
// 缺陷复现（真机 CDP，debug 构建，进入设置页 25–46ms 内就点「显示网速」）：
//   +3ms    checked=False   ← 用户这一下点成功了（乐观更新已生效）
//   +858ms  checked=True    ← 被 loadStatusBarConfig 的**整体覆盖**冲回去了
//   +2209ms checked=True    ← 稳定在旧值，用户视角就是「点了没反应」
//
// 根因：`initSettings()` 里 `await getSettings() → await loadBackupConfig()
//   → await loadStatusBarConfig()` 是串行 await，debug 构建下整条链要 2–4s
//   （其中 loadStatusBarConfig 还要等 loadGlassSupport 并发完成）。
//   而 `statusBarConfig.value = await tracker.getStatusBarConfig()` 是一次
//   **整对象替换**，会把用户在这段窗口里写进去的字段一并冲掉 —— 典型
//   lost update。release 下窗口只有几十毫秒，所以很难撞上，但并非不可能。
//
// ── 修法（v1.2.7 最终版）：权威基线 + 用户差量 ────────────────────
//
// ⚠️ 第一版修法错了，记录在此避免重犯：
//   初版写成 `statusBarConfig.value = { ...loaded, ...statusBarConfig.value }`，
//   想法是「用户值优先」。但右边是**完整对象**（含全部本地默认值），不是差量，
//   于是 spread 把 loaded 里用户**没碰过**的字段也一起盖回本地默认值 ——
//   为了修丢更新，引入了反向丢更新。真机实测（probe_merge_regression.py）：
//     只点「显示网速」→ show_disk / float_enabled / float_glass / enabled
//     全被静默打成 false，且经 set_status_bar_config **写回 DB 持久化**。
//   教训：合并「用户值优先」必须有**差量来源**，不能用整对象冒充差量。
//
// 正确模型：
//   · `statusBarBaseline` —— 后端权威值的快照（加载成功时更新；未加载时为 null）
//   · `statusBarDirty`    —— 用户**实际改过**的键集合（不是布尔「碰过没有」）
//   两个用途：
//   ① 加载返回时合并：`{ ...loaded, ...pick(dirty) }` —— 只让改过的键覆盖权威值
//   ② 提交时合成：      dirty 非空时，以 baseline 打底再叠 dirty，避免把
//                        「未加载的本地默认值」当作真实值提交给后端
//
// 为什么 ② 必需：`statusBarConfig` 初值是 `enabled:false`，若用户在加载完成前
//   （debug 下实测 25–46ms 就能点到）点了「显示网速」，旧代码提交的是整个本地
//   对象，会把后端本来是 true 的总开关/磁盘/浮窗一并写成 false —— 探测脚本实测
//   复现了这个数据损坏。只加「用户值优先合并」是修不了这条的，因为损坏发生在
//   提交方向，不在加载方向。
let statusBarBaseline: StatusBarConfig | null = null;
const statusBarDirty = new Set<keyof StatusBarConfig>();
let statusBarTouched = false;
const statusBarLoaded = ref(false);

/** 取 dirty 键构成的差量对象（只有用户真正改过的字段） */
function statusBarDelta(): Partial<StatusBarConfig> {
  const delta: Record<string, unknown> = {};
  for (const k of statusBarDirty) delta[k as string] = statusBarConfig.value[k];
  return delta as Partial<StatusBarConfig>;
}

/** 合成要提交给后端的完整配置：权威值打底 + 用户差量 */
function statusBarPayload(): StatusBarConfig {
  const base = statusBarBaseline ?? statusBarConfig.value;
  return { ...base, ...statusBarDelta() };
}

/**
 * v1.2.7：**提交前确保有权威基线**。
 *
 * ⚠️ 这里踩过一次坑，务必保留：`statusBarBaseline ?? statusBarConfig.value` 在
 *   基线还没到手时退化成「本地默认值」，等于给污染开了后门。实测（probe）：
 *   用户在加载完成前点「显示网速」，提交出去的 payload 里 enabled/float_glass/
 *   show_disk/float_enabled 全是本地默认的 false，把后端全写坏了 —— 加不加
 *   dirty 集合都拦不住，因为保护的是「合并」而不是「打底来源」。
 *
 * 因此：只要基线为空（首次加载还没回来 / 上次加载失败），提交前先同步拉一次
 * 后端真值当基线。这一次 IPC 很便宜（读内存里的 config，非 IO），且只在
 * 「加载还没完成」这个极短窗口里才会发生。
 */
async function ensureStatusBarBaseline() {
  if (statusBarBaseline) return;
  try {
    statusBarBaseline = await tracker.getStatusBarConfig();
    statusBarLoaded.value = true;
  } catch (err) {
    log.warn("[Settings] 提交前获取状态栏权威配置失败", err);
  }
}

// v1.2.7：子项是否置灰 —— 语义是「总开关关着 → 子项不可点」。
// ⚠️ 必须区分「还没加载到」与「后端确实是 false」：未加载时 statusBarConfig
// 里是本地默认 `enabled:false`，若直接据此置灰，用户在加载窗口内点击会被浏览器
// 吞掉（实测 27–36ms 时 disabled=True，连 change 都不触发）。所以未加载时不置灰，
// 让乐观更新先顶上。
// statusBarTouched 为真时可点：用户已把总开关打开（乐观更新领先于加载），
// 若仍按后端旧值置灰，会看到自己刚打开的开关又把子项锁死。
const statusBarChildrenDisabled = computed(
  () => !statusBarTouched && statusBarLoaded.value && !statusBarConfig.value.enabled,
);

async function loadStatusBarConfig() {
  // 两件事互不依赖，并发发起；任一失败只影响自己的 UI 分支
  await Promise.all([loadGlassSupport(), (async () => {
    try {
      const loaded = await tracker.getStatusBarConfig();
      statusBarBaseline = loaded;
      if (statusBarDirty.size > 0) {
        // 只让「用户改过的键」覆盖权威值，其余一律以后端为准
        statusBarConfig.value = { ...loaded, ...statusBarDelta() };
        log.info(
          `[Settings] 状态栏配置加载晚于用户操作，已保留改动键：${[...statusBarDirty].join(",")}`,
        );
      } else {
        statusBarConfig.value = loaded;
      }
    } catch (err) {
      log.warn("[Settings] 加载状态栏配置失败", err);
    } finally {
      statusBarLoaded.value = true;
    }
  })()]);
}

/** 统一提交入口：合成「权威值 + 差量」后提交，避免把未加载的默认值写坏后端 */
async function commitStatusBarConfig() {
  await ensureStatusBarBaseline();
  await tracker.setStatusBarConfig(statusBarPayload());
}

// 乐观更新：先翻 ref 再 await IPC，失败回滚（与 onAutostart 模式一致）
async function onStatusBarEnabled(e: Event) {
  const next = (e.target as HTMLInputElement).checked;
  const prev = statusBarConfig.value.enabled;
  statusBarTouched = true;
  statusBarDirty.add("enabled");
  statusBarConfig.value = { ...statusBarConfig.value, enabled: next };
  try {
    await commitStatusBarConfig();
    // 提交成功：后端已接受该值，把它并入基线并清 dirty（避免下次重复叠加）
    statusBarBaseline = statusBarPayload();
    statusBarDirty.delete("enabled");
  } catch (err) {
    statusBarConfig.value = { ...statusBarConfig.value, enabled: prev };
    statusBarDirty.delete("enabled");
    showAlert(
      "warn",
      t("settings.statusBarOff"),
      err instanceof Error ? err.message : String(err),
    );
  }
}
async function onStatusBarItem(
  e: Event,
  // v1.2.2：新增 float_glass。它在 Rust 侧 set_status_bar_config 里触发材质重应用，
  // 与其它纯布尔项走同一条路径（乐观更新 + 失败回滚），无需单独的提交函数。
  key: "show_cpu" | "show_mem" | "show_disk" | "show_net" | "float_enabled" | "float_glass",
) {
  const next = (e.target as HTMLInputElement).checked;
  const prev = statusBarConfig.value[key];
  statusBarTouched = true;
  statusBarDirty.add(key);
  statusBarConfig.value = { ...statusBarConfig.value, [key]: next };
  try {
    await commitStatusBarConfig();
    statusBarBaseline = statusBarPayload();
    statusBarDirty.delete(key);
  } catch (err) {
    statusBarConfig.value = { ...statusBarConfig.value, [key]: prev };
    statusBarDirty.delete(key);
    showAlert(
      "warn",
      t("settings.statusBarOff"),
      err instanceof Error ? err.message : String(err),
    );
  }
}
const exportPath = ref("");

// ============ v0.8.0：屏幕截图（配置 + 历史）============
// 设计：所有改动即时持久化（与状态栏卡片同款「乐观更新 + 失败回滚」），
// 不设「保存」按钮——截图是轻量高频操作，用户不希望改个圆角还要点保存。
const shotConfig = ref<ScreenshotConfig>({ ...DEFAULT_SCREENSHOT_CONFIG });
const shotDir = ref("");
const shotHistory = ref<ScreenshotOut[]>([]);
/** id → 缩略图 data URL（按需拉取；文件缺失的条目不会进这张表） */
const shotThumbs = ref<Record<number, string>>({});
// v1.1.1：截图历史分页 —— 与 max_count（FIFO 上限）解耦，按「加载更多」翻页
const SHOT_PAGE_SIZE = 60;
/** 已加载到第几张（= shotHistory.length），作为下次加载的 offset */
const shotHasMore = ref(true);
/** 加载更多按钮是否忙碌（防连点） */
const shotLoadingMore = ref(false);

// v1.2.7：与状态栏配置同源的 lost update —— 用户刚改的截图配置可能被晚到的
// `getConfig()` 整体覆盖。模型与状态栏完全一致：权威基线 + 差量键集合
// （为什么不能用「整对象 + 布尔标记」，见 loadStatusBarConfig 顶部的踩坑记录）。
let shotConfigBaseline: ScreenshotConfig | null = null;
const shotConfigDirty = new Set<keyof ScreenshotConfig>();

/** 取 dirty 键构成的差量对象（只有用户真正改过的字段） */
function shotConfigDelta(): Partial<ScreenshotConfig> {
  const delta: Record<string, unknown> = {};
  for (const k of shotConfigDirty) delta[k as string] = shotConfig.value[k];
  return delta as Partial<ScreenshotConfig>;
}

/** 合成要提交给后端的完整配置：权威值打底 + 用户差量 */
function shotPayload(): ScreenshotConfig {
  const base = shotConfigBaseline ?? shotConfig.value;
  return { ...base, ...shotConfigDelta() };
}

/**
 * v1.2.7：提交前确保有权威基线 —— 与 ensureStatusBarBaseline 同因同解。
 * 基线为空时 `?? shotConfig.value` 会退化成「本地默认值」，把后端未触碰字段写坏。
 * 截图配置的默认值与后端 Default 恰好一致（enabled:true 等），所以现实中不易撞上，
 * 但不能靠「默认值碰巧相同」来保证正确性（`max_count`/`ocr_engine` 平台差异就是反例）。
 */
async function ensureShotBaseline() {
  if (shotConfigBaseline) return;
  try {
    shotConfigBaseline = await screenshot.getConfig();
  } catch (err) {
    log.warn("[Settings] 提交前获取截图权威配置失败", err);
  }
}

/** 统一提交入口：合成「权威值 + 差量」后提交 */
async function commitShotConfig() {
  await ensureShotBaseline();
  return screenshot.setConfig(shotPayload());
}

async function loadShotConfig() {
  try {
    const loaded = await screenshot.getConfig();
    shotConfigBaseline = loaded;
    if (shotConfigDirty.size > 0) {
      shotConfig.value = { ...loaded, ...shotConfigDelta() };
      log.info(
        `[Settings] 截图配置加载晚于用户操作，已保留改动键：${[...shotConfigDirty].join(",")}`,
      );
    } else {
      shotConfig.value = loaded;
    }
    shotDir.value = await screenshot.dir();
  } catch (err) {
    log.warn("[Settings] 加载截图配置失败", err);
  }
}

// ---- v0.9.0：取字引擎 ----
/** 引擎资源探测结果（决定「增强」选项是否可选，以及提示文案） */
const ocrInfo = ref<OcrEngineInfo | null>(null);

async function loadOcrEngineInfo() {
  try {
    ocrInfo.value = await screenshot.ocrEngineInfo();
  } catch (err) {
    log.warn("[Settings] 读取取字引擎信息失败", err);
  }
}

/** 引擎选择提示：资源缺失时说清「为什么不能选」，齐备时报体积（用户对体积有知情权） */
/**
 * 「标准」引擎的描述：按本平台的实际实现换文案。
 * 有系统引擎时顺带写明实现是什么（Windows WinRT / macOS Vision）——
 * 「标准」在不同平台背后是完全不同的东西，写清楚就不用用户猜。
 */
const systemEngineDesc = computed(() => {
  const info = ocrInfo.value;
  if (!info) return t("settings.shotOcrEngineSystemDesc");
  if (!info.system_available) return t("settings.shotOcrEngineSystemUnsupported");
  if (info.system_engine === "vision") return t("settings.shotOcrEngineSystemVision");
  return t("settings.shotOcrEngineSystemDesc");
});

/**
 * 「增强」引擎可否被选中 —— 判据是**模型是否齐备**（三端随包），
 * 而不是 `enhanced_ready`（= 运行库 + 模型）。
 *
 * ⚠️ 这条判据改过两次，别再改回去：
 * - v0.9.0 首发：只要 `!enhanced_ready` 就禁用。macOS 上运行库不随包、下载又
 *   **只在选中「增强」后才触发** → 「标准」禁用（当时 mac 没有系统引擎）、
 *   「增强」也禁用 → **两个都点不了**，用户被永久卡死。
 * - 中间态（未发布）：`enhanced_ready || !system_available`（拿「本平台没有系统引擎」兜底）。
 *   一旦给 macOS 接上系统 Vision，这个兜底条件在 mac 上**不再成立**，
 *   死锁会原样复现（mac 默认走 system → 永远选不中 enhanced → 运行库永不下载）。
 * - 现在：只看 `enhanced_models_ready`。运行库缺失**不是**禁用理由 ——
 *   它由后台下载补齐，提示文案会说清「首次自动下载」。
 */
const enhancedSelectable = computed(() => ocrInfo.value?.enhanced_models_ready === true);

const ocrEngineHint = computed(() => {
  const info = ocrInfo.value;
  if (!info) return t("settings.shotOcrEngineChecking");
  // 模型缺失 = 安装不完整 —— 这是唯一「选了也用不了」的情形
  if (!info.enhanced_models_ready) return t("settings.shotOcrEngineModelsMissing");
  // 模型在、运行库不在：macOS / Linux 的**正常初始状态**，选中后会后台下载。
  // 「正在获取」与「缺失/请重装」必须分开说，否则用户会以为坏了。
  if (!info.enhanced_runtime_ready) return t("settings.shotOcrEngineDownloading");
  return t("settings.shotOcrEngineReady", { mb: String(info.enhanced_size_mb) });
});

/** 切换取字引擎：乐观更新 + 失败回滚（与卡片内其他开关同款约定） */
async function onOcrEngine(kind: OcrEngineKind) {
  const prev = shotConfig.value.ocr_engine;
  if (prev === kind) return;
  shotConfigDirty.add("ocr_engine");
  shotConfig.value = { ...shotConfig.value, ocr_engine: kind };
  try {
    await commitShotConfig();
    shotConfigBaseline = shotPayload();
    shotConfigDirty.delete("ocr_engine");
    // 切换后重新探测一次：后端会顺带回报资源状态与当前生效引擎
    await loadOcrEngineInfo();
  } catch (err) {
    shotConfig.value = { ...shotConfig.value, ocr_engine: prev };
    shotConfigDirty.delete("ocr_engine");
    showAlert("warn", t("settings.shotTitle"), err instanceof Error ? err.message : String(err));
  }
}

/** 数值类改动：先乐观写 ref，再 await IPC，失败则回读真实配置纠偏
 *
 * v1.2.7：`corner_radius` / `max_count` 走的是 `v-model.number` **直接写 ref**，
 * 没有独立的 change handler，所以 dirty 键要在这里按「与基线是否不同」反推。
 * 否则用户在加载窗口内拖滑块，会被晚到的 `getConfig()` 整体覆盖（+= 丢更新）。
 */
async function persistShotConfig() {
  for (const k of ["corner_radius", "max_count"] as const) {
    if (shotConfigBaseline && shotConfig.value[k] !== shotConfigBaseline[k]) {
      shotConfigDirty.add(k);
    }
  }
  const snapshot = shotPayload();
  try {
    const res = await screenshot.setConfig(snapshot);
    shotConfigBaseline = snapshot;
    shotConfigDirty.clear();
    if (!res.applied && snapshot.enabled) {
      // 快捷键没注册上（被系统/其他软件占用）——其余配置已保存，只是热键没生效
      hotkeyError.value = t("settings.shotShortcutConflict") + " " + (res.error ?? "");
      await loadShotConfig();
    }
  } catch (err) {
    showAlert("warn", t("settings.shotTitle"), err instanceof Error ? err.message : String(err));
    await loadShotConfig();
  }
}

// ===== v0.8.0 修订：快捷键「按键录制」控件 =====
// 交互：点一下按钮进入录制态 → 直接按下想要的组合键 → 自动归一化成 global-hotkey 语法并保存；
// 保存后 Rust 会把**真实注册结果**带回来，被占用则红字提示并回滚到原组合（不用让用户猜）。
const hotkeyRecording = ref(false);
const hotkeyError = ref("");
/** 录制中只按了修饰键时的实时预览（如 "Ctrl + Shift + …"） */
const hotkeyPreview = ref("");
const hotkeyBtnEl = ref<HTMLButtonElement | null>(null);

const IS_MAC =
  typeof navigator !== "undefined" &&
  /mac/i.test(`${navigator.platform} ${navigator.userAgent}`);

// ===== v1.2.10（2026-10-26）：权限中心 =====
//
// 为什么要有这块：v1.2.10 之前权限状态只存在于顶部横幅和截图授权弹窗里，设置页
// 「checkPermissions」零命中 —— 用户想主动确认「我到底授权了没有」在界面上无路可走，
// 只能靠横幅出现/消失猜。横幅此后只承担「通知」，操作统一收进这张卡片。
//
// 为什么 onMounted 查一次 + 手动刷新，不做定时轮询：Rust 的 `check_permissions`
// 在未放行时会跑 `codesign` / `xattr` 两个子进程做诊断，接进 1Hz/2Hz 轮询等于
// 每 2 秒 fork 两个进程（见 `commands/platform/macos.rs` 的明确警告）。
// 用户从系统设置改完授权后点一下「刷新状态」重查即可，成本只落在他主动操作时。
/** null = 还没查到第一次（渲染成占位符，避免闪一下「未授权」） */
const permissions = ref<PermissionStatus | null>(null);

async function loadPermissions() {
  if (!IS_MAC) return;
  try {
    permissions.value = await tracker.checkPermissions();
  } catch (err) {
    log.warn("[Settings] 读取权限状态失败", err);
  }
}

/** 「去授权」：主动弹系统授权框（非 macOS 恒 true）。失败不提示 —— 用户能看到状态没变。 */
async function onPermGrantScreen() {
  try {
    await screenshot.requestPermission();
  } catch (err) {
    log.warn("[shot] 请求屏幕录制授权失败", err);
  }
  // 系统框是异步的，紧接着查通常还是旧值；仍查一次，让「已授权」的情况立刻刷新。
  await loadPermissions();
}

/**
 * 「重置并重启」：tccutil 清掉指向旧版本签名指纹的脏记录，再重启进程。
 * 两步缺一不可 —— 只清不重启，新授权对当前进程无效；只重启，脏记录还在。
 * 成功后进程直接退出，下面的 await 不会返回。
 */
async function onPermResetRestart() {
  try {
    await tracker.resetScreenCapturePermission();
  } catch (err) {
    // 重置失败不阻断重启：tccutil 在个别系统上要 sudo，至少让「重启」这条路还能走
    log.warn("[shot] 重置屏幕录制授权失败", err);
  }
  try {
    await tracker.restartApp();
  } catch (err) {
    showAlert("warn", t("settings.shotTitle"), err instanceof Error ? err.message : String(err));
  }
}

/**
 * 辅助功能「去系统设置」。
 *
 * 为什么直接跳转而不先弹授权框：本仓库全程没有 AXUIElement / CGEventTap 调用，
 * 空闲检测走的是 `CGEventSourceSecondsSinceLastEventType`（不需要辅助功能）。
 * 这里的开关只影响空闲判据精度，缺失时按「不空闲」处理，统计照常可用 ——
 * 不值得为它再弹一个用户看不懂的系统框。跳到列表页让用户自己决定即可。
 */
async function onPermOpenAccessibility() {
  try {
    await tracker.openPrivacySettings("accessibility");
  } catch (err) {
    log.warn("[Settings] 打开辅助功能设置失败", err);
  }
}

/** 把存储语法（CmdOrCtrl+Shift+A）渲染成用户看得懂的按键标签 */
function formatHotkey(s: string): string {
  if (!s) return "—";
  const parts = s.split("+").map((p) => p.trim()).filter(Boolean);
  const out: string[] = [];
  for (const p of parts) {
    const u = p.toUpperCase();
    if (["CMDORCTRL", "CMDORCONTROL", "COMMANDORCTRL", "COMMANDORCONTROL"].includes(u)) {
      out.push(IS_MAC ? "⌘" : "Ctrl");
    } else if (["SUPER", "CMD", "COMMAND", "META"].includes(u)) {
      out.push(IS_MAC ? "⌘" : "Win");
    } else if (u === "CTRL" || u === "CONTROL") {
      out.push(IS_MAC ? "⌃" : "Ctrl");
    } else if (u === "ALT" || u === "OPTION") {
      out.push(IS_MAC ? "⌥" : "Alt");
    } else if (u === "SHIFT") {
      out.push(IS_MAC ? "⇧" : "Shift");
    } else {
      out.push(p.length === 1 ? p.toUpperCase() : p);
    }
  }
  return out.join(IS_MAC ? "" : " + ");
}

function startHotkeyRecording() {
  hotkeyRecording.value = true;
  hotkeyPreview.value = "";
  hotkeyError.value = "";
  // macOS 上点击按钮不会自动获得焦点（Safari/WKWebView 行为），
  // 不主动 focus 的话 keydown 会派发到 document，录制看起来"没反应"。
  void nextTick(() => hotkeyBtnEl.value?.focus());
}

function stopHotkeyRecording() {
  hotkeyRecording.value = false;
  hotkeyPreview.value = "";
}

/** 修饰键 → global-hotkey 记号。Ctrl 统一记成 CmdOrCtrl，让配置能跨平台复用 */
function modifierTokens(e: KeyboardEvent): string[] {
  const out: string[] = [];
  if (e.ctrlKey) out.push("CmdOrCtrl");
  if (e.metaKey) out.push("Super");
  if (e.altKey) out.push("Alt");
  if (e.shiftKey) out.push("Shift");
  return out;
}

/** 主键 → global-hotkey 记号；返回 null 表示「只是按了修饰键」或该键无法表达 */
function mainToken(e: KeyboardEvent): string | null {
  const k = e.key;
  if (["Control", "Shift", "Alt", "Meta", "AltGraph", "CapsLock"].includes(k)) return null;
  if (k === " ") return "Space";
  // '+' 是语法分隔符，无法表达；让用户换一个键而不是静默写入一个永远解析失败的字符串
  if (k === "+") return null;
  if (/^Arrow(Up|Down|Left|Right)$/.test(k)) return k.slice(5);
  if (/^F([1-9]|1[0-9]|2[0-4])$/.test(k)) return k.toUpperCase();
  if (k.length === 1) return k.toUpperCase();
  if (["Enter", "Tab", "Backspace", "Delete", "Home", "End", "PageUp", "PageDown", "Insert", "PrintScreen"].includes(k)) {
    return k;
  }
  return null;
}

async function onHotkeyKeydown(e: KeyboardEvent) {
  if (!hotkeyRecording.value) return;
  e.preventDefault();
  e.stopPropagation();
  if (e.key === "Escape") {
    stopHotkeyRecording();
    return;
  }
  const mods = modifierTokens(e);
  const main = mainToken(e);
  if (!main) {
    hotkeyPreview.value = mods.length ? `${formatHotkey(mods.join("+"))}${IS_MAC ? "" : " + …"}` : "";
    return;
  }
  if (mods.length === 0) {
    hotkeyError.value = t("settings.shotShortcutNeedModifier");
    return;
  }
  stopHotkeyRecording();
  await applyHotkey([...mods, main].join("+"));
}

/** 写入并保存；Rust 报注册失败则回滚到上一个可用组合 */
async function applyHotkey(combo: string) {
  const prev = shotConfig.value.shortcut;
  if (combo === prev) {
    hotkeyError.value = "";
    return;
  }
  shotConfigDirty.add("shortcut");
  shotConfig.value = { ...shotConfig.value, shortcut: combo };
  try {
    const res = await commitShotConfig();
    if (res.applied) {
      shotConfigBaseline = shotPayload();
      shotConfigDirty.delete("shortcut");
      hotkeyError.value = "";
      return;
    }
    hotkeyError.value = `${t("settings.shotShortcutConflict")} ${res.error ?? ""}`;
    shotConfig.value = { ...shotConfig.value, shortcut: prev };
    shotConfigDirty.delete("shortcut");
    await commitShotConfig();
  } catch (err) {
    hotkeyError.value = err instanceof Error ? err.message : String(err);
    shotConfig.value = { ...shotConfig.value, shortcut: prev };
    shotConfigDirty.delete("shortcut");
  }
}

async function resetHotkey() {
  await applyHotkey(DEFAULT_SCREENSHOT_CONFIG.shortcut);
}


async function onShotEnabled(e: Event) {
  const next = (e.target as HTMLInputElement).checked;
  const prev = shotConfig.value.enabled;
  shotConfigDirty.add("enabled");
  shotConfig.value = { ...shotConfig.value, enabled: next };
  try {
    await commitShotConfig();
    shotConfigBaseline = shotPayload();
    shotConfigDirty.delete("enabled");
  } catch (err) {
    shotConfig.value = { ...shotConfig.value, enabled: prev };
    shotConfigDirty.delete("enabled");
    showAlert("warn", t("settings.shotTitle"), err instanceof Error ? err.message : String(err));
  }
}

async function onShotToggle(e: Event, key: "auto_save") {
  const next = (e.target as HTMLInputElement).checked;
  const prev = shotConfig.value[key];
  shotConfigDirty.add(key);
  shotConfig.value = { ...shotConfig.value, [key]: next };
  try {
    await commitShotConfig();
    shotConfigBaseline = shotPayload();
    shotConfigDirty.delete(key);
  } catch (err) {
    shotConfig.value = { ...shotConfig.value, [key]: prev };
    shotConfigDirty.delete(key);
    showAlert("warn", t("settings.shotTitle"), err instanceof Error ? err.message : String(err));
  }
}

// v1.1.0：延时截图（借鉴 Snapzy）。倒计时渲染与取消入口在 main 窗的悬浮倒计时组件
//（监听 screenshot-countdown 事件），本函数只负责发起。
async function onShotDelayed(seconds: number) {
  try {
    await screenshot.triggerDelayed(seconds);
  } catch (err) {
    showAlert("warn", t("settings.shotTitle"), err instanceof Error ? err.message : String(err));
  }
}

async function openShotDir() {
  if (!shotDir.value) return;
  try {
    await tracker.revealPath(shotDir.value);
  } catch {
    /* 打不开目录不影响主流程 */
  }
}

async function loadShotHistory() {
  try {
    // v1.1.1：首页加载（offset=0），成功后再按「加载更多」追加翻页
    shotHistory.value = await screenshot.list(SHOT_PAGE_SIZE, 0);
    shotHasMore.value = shotHistory.value.length === SHOT_PAGE_SIZE;
    await fetchShotThumbs(shotHistory.value);
  } catch (err) {
    log.warn("[Settings] 加载截图历史失败", err);
  }
}

/** 按需拉取一批截图的缩略图（并发；文件缺失的条目返回空串，不进表） */
async function fetchShotThumbs(items: ScreenshotOut[]) {
  const pairs = await Promise.all(
    items.map(async (s) => [s.id, await screenshot.thumbnail(s.id, 320)] as const),
  );
  const map: Record<number, string> = {};
  for (const [id, url] of pairs) {
    if (url) map[id] = url;
  }
  shotThumbs.value = { ...shotThumbs.value, ...map };
}

/** 「加载更多」：以当前已加载数量为 offset 翻下一页并追加 */
async function loadMoreShots() {
  if (shotLoadingMore.value) return;
  shotLoadingMore.value = true;
  try {
    const next = await screenshot.list(SHOT_PAGE_SIZE, shotHistory.value.length);
    if (next.length === 0) {
      shotHasMore.value = false;
    } else {
      shotHistory.value = [...shotHistory.value, ...next];
      shotHasMore.value = next.length === SHOT_PAGE_SIZE;
      await fetchShotThumbs(next);
    }
  } catch (err) {
    log.warn("[Settings] 加载更多截图失败", err);
  } finally {
    shotLoadingMore.value = false;
  }
}

async function revealShot(id: number) {
  try {
    await screenshot.reveal(id);
  } catch {
    /* ignore */
  }
}

async function deleteShot(id: number) {
  try {
    await screenshot.remove(id);
    shotHistory.value = shotHistory.value.filter((s) => s.id !== id);
    delete shotThumbs.value[id];
  } catch (err) {
    showAlert("warn", t("settings.shotTitle"), err instanceof Error ? err.message : String(err));
  }
}

// v0.9.1：截图历史多选（批量删除）
const shotSelecting = ref(false);
const selectedShotIds = ref<number[]>([]);

function toggleShotSelecting() {
  shotSelecting.value = !shotSelecting.value;
  if (!shotSelecting.value) selectedShotIds.value = [];
}
function toggleShotSelect(id: number) {
  const i = selectedShotIds.value.indexOf(id);
  if (i >= 0) selectedShotIds.value.splice(i, 1);
  else selectedShotIds.value.push(id);
}
function selectAllShots() {
  selectedShotIds.value = shotHistory.value.map((s) => s.id);
}
function clearShotSelection() {
  selectedShotIds.value = [];
}
async function deleteSelectedShots() {
  if (selectedShotIds.value.length === 0) return;
  const ids = [...selectedShotIds.value];
  try {
    await screenshot.removeMany(ids);
    shotHistory.value = shotHistory.value.filter((s) => !ids.includes(s.id));
    for (const id of ids) delete shotThumbs.value[id];
    selectedShotIds.value = [];
    shotSelecting.value = false;
  } catch (err) {
    showAlert("warn", t("settings.shotTitle"), err instanceof Error ? err.message : String(err));
  }
}

// 截图初始化：配置 + 取字引擎 + 历史 + screenshot-done 监听（主窗口收到 Rust 事件后刷新）
async function initScreenshot() {
  await loadShotConfig();
  await loadOcrEngineInfo();
  await loadShotHistory();
  try {
    unlistenShotDone = await listen("screenshot-done", () => {
      void loadShotHistory();
    });
    // v0.9.2：截图失败提示已统一提升到主窗口 App.vue 的**全局 toast**
    // （此前只在设置页弹错 → 用户用快捷键截图、人不在设置页时失败是静默的），
    // 此处不再重复监听，避免同一次失败弹两次提示。
  } catch {
    /* 非 Tauri 环境 */
  }
}

const pruneDialogOpen = ref(false);
const deviceStats = ref<DeviceStats[]>([]);
const selectedDeviceIds = ref<string[]>([]);

async function openDevicePrune() {
  pruneDialogOpen.value = true;
  selectedDeviceIds.value = [];
  try {
    deviceStats.value = await tracker.devicesWithStats();
  } catch (e: any) {
    showAlert("warn", t("settings.loadFailed"), t("settings.loadDevicesFailed", { err: e?.message || e }));
  }
}

function formatSeconds(s: number): string {
  return formatDuration(s);
}

async function onConfirmPruneByDevice() {
  const ids = selectedDeviceIds.value;
  if (ids.length === 0) {
    try {
      const n = await tracker.pruneData(retention.value);
      showAlert("info", t("settings.cleaned"), t("settings.cleanedOld", { n, days: retention.value }));
      pruneDialogOpen.value = false;
    } catch (e) {
      showAlert("warn", t("settings.cleanFailed"), t("settings.cleanFailedMsg", { err: e instanceof Error ? e.message : String(e) }));
    }
    return;
  }
  pruneDialogOpen.value = false;
  let totalDeleted = 0;
  const backups: string[] = [];
  try {
    for (const id of ids) {
      const res = await tracker.backupAndPruneDevice(id);
      totalDeleted += res.deleted_count;
      backups.push(res.backup_path);
    }
    showAlert(
      "info",
      t("settings.cleaned"),
      t("settings.cleanedByDeviceMsg", { n: totalDeleted, devices: ids.length, backups: backups.join("\n") })
    );
  } catch (e) {
    log.error("按设备清理失败", e);
    showAlert("warn", t("settings.cleanFailed"), t("settings.cleanFailedMsg", { err: e instanceof Error ? e.message : String(e) }));
  }
}

function confirmCleanAll() {
  showAlert(
    "confirm",
    t("settings.cleanAllConfirmTitle"),
    t("settings.cleanAllConfirmMsg", { days: retention.value }),
    async () => {
      try {
        const n = await tracker.pruneData(retention.value);
        showAlert("info", t("settings.cleaned"), t("settings.cleanedSimple", { n }));
      } catch (e) {
        log.error("清理失败", e);
        showAlert("warn", t("settings.cleanFailed"), t("settings.cleanFailedMsg", { err: e instanceof Error ? e.message : String(e) }));
      }
    }
  );
}

// （v0.6.2-beta.19 移除"重置设备 ID"按钮：后端未实现 reset_device_id 命令，截图里的"重置"按钮只用作视觉示意，保留会让用户点了报错。设备 ID 不可重置：它绑定了 sessions 数据。）

// 通用设置初始化：版本 + 通用设置 + 备份配置 + 状态栏配置
//
// ── v1.2.7：这一组字段也栽在同一个坑里（收尾审计发现的第三处）──
// 症状（release 构建实测，进设置页 +17ms 就开始打字）：
//     用户输入 '__race_probe_name__' → getSettings() 返回后被回填成 'DESKTOP-SV0FM39'
//   · 用户在设备名框里打的字被静默清空并填回旧值。更糟的是：若他接着点「保存」，
//     保存回去的就是**旧名字**（因为 v-model 已被覆盖），用户会以为「改名不生效」。
//   · `autostart` 更隐蔽：它有即时提交（onAutostart 直接写后端），若加载晚于点击，
//     UI 被回滚成旧值而后端已是新值 → 两边长期不一致，下次进页面又「自己变回去」。
//
// 修法同前两处：权威基线 + 差量。区别是这组字段**没有独立的 change handler**
// （`deviceName`/`idleMin`/`retention` 是 v-model 直改），所以差量靠「当前值 vs
// 基线」比较反推，而不是靠标记 —— 比较法对 v-model 直改也适用。
//
// ⚠️ 关键细节：比较必须在**用新基线覆盖之前**做。若先 `settingsBaseline = s`
//   再比较，空的 deviceName('') 与 s.device_name 必然不等 → 被误判成「用户改过」
//   → 用户永远看不到真实的设备名。这是很容易写错的顺序问题。
let settingsBaseline: Awaited<ReturnType<typeof tracker.getSettings>> | null = null;
/** 用户是否已经动过某个表单字段（动过就不许被加载覆盖） */
type SettingsDirtyKey =
  | "device_name"
  | "idle_threshold"
  | "data_retention_days"
  | "autostart";
const settingsDirty = new Set<SettingsDirtyKey>();

/**
 * 表单「未加载」时的初值 —— 用作 first-load 的隐式基线。
 * ⚠️ 必须与下面 `const deviceName = ref("")` 等几个 ref 的初值保持一致。
 * 改那几个 ref 的初值时，这里要同步改，否则「加载窗口内的用户输入」会被
 * 误判成「没改过」而被覆盖（或反过来把初值误判成用户改动）。
 */
const FORM_INIT = {
  device_name: "",
  idle_threshold: 5 * 60,
  data_retention_days: 365,
  autostart: false,
} as const;

/** 依据「当前 UI 值 vs 旧基线」反推哪些键被用户改过（v-model 直改也能捕捉到） */
function collectSettingsDirty(prev: typeof settingsBaseline) {
  // ⚠️ 首次加载时 `prev` 是 null，但不能就此 return —— 那会让「首次进入页面、
  //    加载还没回来就打字」这一最常见场景完全失去保护（实测就是这么漏的）：
  //    deviceName 初值 ''、用户输入 '__race_probe_name__'，这一改必须被记住。
  //    解法：没有旧基线时用**表单初值**当基线（FORM_INIT 常量，见上）。
  const base = prev ?? FORM_INIT;
  if (deviceName.value !== base.device_name) settingsDirty.add("device_name");
  if (Math.round(idleMin.value * 60) !== base.idle_threshold) {
    settingsDirty.add("idle_threshold");
  }
  if (retention.value !== base.data_retention_days) {
    settingsDirty.add("data_retention_days");
  }
  if (autostart.value !== base.autostart) settingsDirty.add("autostart");
}

async function initSettings() {
  try {
    version.value = await getVersion();
  } catch {
    /* 浏览器预览模式忽略 */
  }
  try {
    const prev = settingsBaseline; // 必须在覆盖前取旧基线
    const s = await tracker.getSettings();
    collectSettingsDirty(prev); // 用旧基线判断用户是否在等待期间改过
    settingsBaseline = s;
    settings.value = s;
    if (settingsDirty.size > 0) {
      // 加载值打底，用户改过的字段保留用户值（方向不能反）
      if (!settingsDirty.has("device_name")) deviceName.value = s.device_name;
      if (!settingsDirty.has("idle_threshold")) {
        idleMin.value = Math.max(1, Math.round(s.idle_threshold / 60));
      }
      if (!settingsDirty.has("data_retention_days")) retention.value = s.data_retention_days;
      if (!settingsDirty.has("autostart")) autostart.value = s.autostart;
      log.info(
        `[Settings] 通用设置加载晚于用户操作，已保留改动字段：${[...settingsDirty].join(",")}`,
      );
    } else {
      deviceName.value = s.device_name;
      idleMin.value = Math.max(1, Math.round(s.idle_threshold / 60));
      retention.value = s.data_retention_days;
      autostart.value = s.autostart;
    }
  } catch {
    /* 浏览器预览模式忽略 */
  }
  try {
    await loadBackupConfig();
  } catch {
    /* 浏览器预览模式忽略 */
  }
  try {
    await loadStatusBarConfig();
  } catch {
    /* 浏览器预览模式忽略 */
  }
}

// 切换开机自启（v0.6.2-beta.23：去掉成功弹窗；v0.6.2-beta.24：乐观更新 + 失败回滚）
// 关键：<input> 是单向 :checked 绑定，vue 下次 render 会用 autostart.value 强制覆盖原生 checked，
// 若这里不翻转 autostart.value，toggle 会"弹回"，表现成"点不动"。故先乐观翻转，失败再回滚。
async function onAutostart(e: Event) {
  const next = (e.target as HTMLInputElement).checked;
  autostart.value = next; // 乐观更新：立即反映到 :checked 与 :class="{ on }"
  // v1.2.7：autostart 是即时提交的，用户一点就代表「我希望它是这个值」，
  // 记进 dirty 以免晚到的 getSettings() 把它回滚成旧值（那会造成 UI 与后端不一致）
  settingsDirty.add("autostart");
  try {
    await tracker.setAutostart(next);
    // 提交成功：把该字段并入基线，解除 dirty
    if (settingsBaseline) settingsBaseline = { ...settingsBaseline, autostart: next };
    settingsDirty.delete("autostart");
  } catch (err) {
    autostart.value = !next; // 失败回滚
    settingsDirty.delete("autostart");
    showAlert("warn", t("settings.autostartFailed"), t("settings.autostartFailedMsg", { err: err instanceof Error ? err.message : String(err) }));
  }
}

async function onSave() {
  // 实际生效值：设备名为空时后端回落旧名（见 save_settings 语义），
  // 保存成功后用真实生效值回写，避免「输入框空、实际用旧名」的不一致。
  const effectiveName = deviceName.value.trim() || settings.value.device_name;
  const effectiveIdle = idleMin.value * 60;
  const effectiveRetention = retention.value;
  try {
    await tracker.saveSettings({
      idleThreshold: effectiveIdle,
      deviceName: effectiveName,
      dataRetentionDays: effectiveRetention,
    });
    // 同步内存态与界面，保持「所见即所得」
    settings.value.device_name = effectiveName;
    settings.value.idle_threshold = effectiveIdle;
    settings.value.data_retention_days = effectiveRetention;
    deviceName.value = effectiveName;
    // v1.2.7：保存成功即视为「用户改动已被后端接受」，清掉差量并刷新基线，
    // 否则下次进页面还带着旧 dirty，会挡住后续的权威值刷新。
    settingsBaseline = { ...settings.value };
    settingsDirty.clear();
    showAlert("info", t("settings.saved"), t("settings.savedMsg"));
  } catch (e) {
    log.error("保存设置失败", e);
    showAlert("warn", t("settings.saveFailed"), t("settings.saveFailedMsg", { err: e instanceof Error ? e.message : String(e) }));
  }
}

async function onExport() {
  try {
    const res = await tracker.exportAll();
    exportPath.value = res.path;
    exportDialogOpen.value = true;
  } catch (e) {
    log.error("导出失败", e);
    showAlert("warn", t("settings.exportFailed"), t("settings.exportFailedMsg", { err: e instanceof Error ? e.message : String(e) }));
  }
}

async function reveal(path: string) {
  try {
    await tracker.revealPath(path);
  } catch (e) {
    showAlert("warn", t("settings.openFailed"), t("settings.openFailedMsg", { err: e instanceof Error ? e.message : String(e) }));
  }
}

async function copy(path: string) {
  try {
    await navigator.clipboard.writeText(path);
    showAlert("info", t("settings.copied"), t("settings.copiedMsg"));
  } catch {
    showAlert("warn", t("settings.copyFailed"), t("settings.copyFailedMsg", { err: path }));
  }
}

const logSize = ref<number | null>(null);
const logExportDialogOpen = ref(false);
const logExportPath = ref("");

function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 B";
  const units = ["B", "KB", "MB", "GB"];
  const i = Math.floor(Math.log(bytes) / Math.log(1024));
  return `${(bytes / Math.pow(1024, i)).toFixed(2)} ${units[i]}`;
}

async function refreshLogSize() {
  try {
    const size = await invoke<number>("get_log_size");
    logSize.value = size;
  } catch (e) {
    log.error("读取日志大小失败", e);
    logSize.value = null;
  }
}

async function exportLogs() {
  try {
    const res = await invoke<{ path: string }>("export_logs");
    logExportPath.value = res.path;
    logExportDialogOpen.value = true;
    void refreshLogSize();
  } catch (e) {
    log.error("导出日志失败", e);
    showAlert("warn", t("settings.exportFailed"), t("settings.exportFailedMsg", { err: e instanceof Error ? e.message : String(e) }));
  }
}

async function revealLogDir() {
  try {
    const logDir = await invoke<string>("get_log_dir");
    await tracker.revealPath(logDir);
  } catch (e) {
    log.error("打开日志目录失败", e);
    showAlert("warn", t("settings.openFailed"), t("settings.openFailedMsg", { err: e instanceof Error ? e.message : String(e) }));
  }
}

function pickImport() {
  fileInput.value?.click();
}

async function onImport(e: Event) {
  const file = (e.target as HTMLInputElement).files?.[0];
  if (!file) return;
  try {
    const text = await file.text();
    const n = await tracker.importData(text);
    showAlert("info", t("settings.importSuccess"), t("settings.importedMsg", { n }));
  } catch (err) {
    log.error("导入失败", err);
    showAlert("warn", t("settings.importFailed"), t("settings.importFailedMsg", { err: err instanceof Error ? err.message : String(err) }));
  } finally {
    (e.target as HTMLInputElement).value = "";
  }
}

async function goDownload(url: string) {
  try {
    await tracker.openUrl(url);
  } catch (e: any) {
    showAlert("warn", t("settings.openFailed"), t("settings.openDownloadFailed", { err: e?.message || e, url }));
  }
}

async function onCheckUpdate() {
  checking.value = true;
  updateResult.value = null;
  try {
    updateResult.value = await tracker.checkUpdate();
    if (updateResult.value.has_update) {
      showAlert("info", t("settings.foundNew"), t("settings.newVersionMsg", { current: updateResult.value.current, latest: updateResult.value.latest }));
    } else {
      showAlert("info", t("settings.upToDate", { current: updateResult.value.current }), t("settings.upToDateMsg", { current: updateResult.value.current }));
    }
  } catch (e: any) {
    log.error("检查更新失败", e);
    showAlert("warn", t("settings.checkUpdateFailed"), t("settings.checkUpdateFailedMsg", { err: e?.message || e }));
  } finally {
    checking.value = false;
  }
}
</script>

<style scoped>
/* ============ v0.6.2-beta.19 卡片化样式 ============ */
.settings {
  display: flex;
  flex-direction: column;
  gap: 16px;
  max-width: 720px;
}

/* 页头：左侧品牌色横条 + 标题 */
.settings-header {
  display: flex;
  align-items: center;
  gap: 10px;
  margin: 4px 0 8px;
}
.settings-header h2 {
  font-size: 20px;
  font-weight: 700;
  color: var(--text);
  margin: 0;
  letter-spacing: 0.2px;
}
.header-bar {
  width: 4px;
  height: 22px;
  border-radius: 2px;
  background: var(--accent, #ff7e27);
}

/* 卡片本体 */
.setting-card {
  background: var(--card);
  border: 1px solid var(--border);
  border-radius: 14px;
  padding: 16px 18px;
  transition: box-shadow 0.18s ease, border-color 0.18s ease;
}
.setting-card:hover {
  box-shadow: 0 2px 12px rgba(0, 0, 0, 0.04);
}

/* 卡片头：图标 + 标题/副标题 */
.card-head {
  display: flex;
  align-items: center;
  gap: 12px;
  margin-bottom: 14px;
}
.head-icon {
  width: 40px;
  height: 40px;
  flex-shrink: 0;
  border-radius: 11px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  color: #fff;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.06);
}
.icon-orange { background: linear-gradient(135deg, #ff8a3d, #ff7e27); }
.icon-blue   { background: linear-gradient(135deg, #5a9cff, #3b82f6); }
.icon-green  { background: linear-gradient(135deg, #4cd998, #34c759); }
.icon-red    { background: linear-gradient(135deg, #ff6b6b, #ef4444); }
.icon-pink   { background: linear-gradient(135deg, #ff8db3, #ff6b9d); }

.head-text { min-width: 0; }
.head-text h3 {
  font-size: 15px;
  font-weight: 600;
  margin: 0;
  color: var(--text);
  line-height: 1.3;
}
.head-text p {
  font-size: 12.5px;
  color: var(--text-dim, #86868b);
  margin: 2px 0 0;
  line-height: 1.45;
}

/* 卡片 body */
.card-body { padding: 2px 0 0; }

/* 表单行 */
.form-row { margin-bottom: 14px; }
.form-row:last-of-type { margin-bottom: 8px; }
.form-row > label {
  display: block;
  font-size: 13px;
  color: var(--text-dim, #86868b);
  margin-bottom: 6px;
  font-weight: 500;
}
.form-row.row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
}
.form-row.row > label { margin-bottom: 0; }
.field-hint {
  font-size: 12px;
  color: var(--text-dim, #86868b);
  margin: 6px 0 0;
  line-height: 1.5;
}

/* 快捷键录制控件（v0.8.0 修订） */
.hotkey-row {
  display: flex;
  align-items: center;
  gap: 8px;
}
.hotkey-btn {
  min-width: 148px;
  padding: 8px 14px;
  border: 1px dashed var(--border);
  border-radius: 8px;
  background: var(--bg, #f5f5f7);
  color: var(--text);
  font-size: 13px;
  font-weight: 600;
  font-variant-numeric: tabular-nums;
  cursor: pointer;
  transition: border-color 0.15s, background 0.15s, color 0.15s;
}
.hotkey-btn:hover:not(:disabled) {
  border-color: #2f6bff;
}
.hotkey-btn.recording {
  border-style: solid;
  border-color: #2f6bff;
  background: rgba(47, 107, 255, 0.12);
  color: #2f6bff;
}
.hotkey-btn.invalid {
  border-color: #ef4444;
  color: #ef4444;
}
.hotkey-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
.field-hint--error {
  color: #ef4444;
}

/* 文本输入 */
.text-input {
  width: 100%;
  max-width: 320px;
  padding: 8px 12px;
  border: 1px solid var(--border);
  border-radius: 8px;
  font-size: 14px;
  background: var(--bg, #f5f5f7);
  color: var(--text);
  transition: border-color 0.15s, background 0.15s;
}
.text-input:focus {
  outline: none;
  border-color: var(--accent, #ff7e27);
  background: var(--card);
  box-shadow: 0 0 0 3px rgba(255, 126, 39, 0.12);
}
.text-input.narrow { max-width: 160px; }

/* 胶囊式单选（语言） */
.radio-pills {
  display: flex;
  gap: 10px;
  flex-wrap: wrap;
}
.radio-pill {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  padding: 7px 14px;
  border: 1.5px solid var(--border);
  border-radius: 999px;
  font-size: 13px;
  color: var(--text);
  background: var(--bg, #f5f5f7);
  cursor: pointer;
  user-select: none;
  -webkit-user-select: none;
  transition: border-color 0.15s, background 0.15s, color 0.15s;
}
.radio-pill:hover { border-color: rgba(255, 126, 39, 0.45); }
.radio-pill.active {
  border-color: var(--accent, #ff7e27);
  background: rgba(255, 126, 39, 0.08);
  color: var(--accent, #ff7e27);
  font-weight: 500;
}
.radio-pill input { display: none; }
.radio-dot {
  width: 14px;
  height: 14px;
  border-radius: 50%;
  border: 1.5px solid var(--border);
  background: var(--card);
  position: relative;
  flex-shrink: 0;
  transition: border-color 0.15s, background 0.15s;
}
.radio-pill.active .radio-dot {
  border-color: var(--accent, #ff7e27);
  background: var(--accent, #ff7e27);
}
.radio-pill.active .radio-dot::after {
  content: '';
  position: absolute;
  inset: 3px;
  background: #fff;
  border-radius: 50%;
}

/* 卡片底部操作 */
.card-actions {
  display: flex;
  justify-content: flex-end;
  margin-top: 8px;
  padding-top: 12px;
  border-top: 1px dashed rgba(0, 0, 0, 0.06);
}

/* 主按钮（实心品牌色） */
.primary-btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  border: none;
  background: var(--accent, #ff7e27);
  color: #fff;
  padding: 8px 18px;
  border-radius: 8px;
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
  transition: background 0.15s, transform 0.05s;
}
.primary-btn:hover { background: #f56f1a; }
.primary-btn:active { transform: scale(0.97); }
.primary-btn:disabled { opacity: 0.5; cursor: not-allowed; }
.primary-btn.outline {
  background: transparent;
  color: var(--accent, #ff7e27);
  border: 1px solid var(--accent, #ff7e27);
}
.primary-btn.outline:hover { background: rgba(255, 126, 39, 0.08); }

/* Ghost 按钮（次要操作） */
.ghost-btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  border: 1px solid var(--border);
  background: var(--bg, #f5f5f7);
  color: var(--text);
  padding: 7px 14px;
  border-radius: 8px;
  font-size: 13px;
  cursor: pointer;
  transition: all 0.15s;
}
.ghost-btn:hover {
  border-color: var(--accent, #ff7e27);
  color: var(--accent, #ff7e27);
  background: rgba(255, 126, 39, 0.05);
}
.ghost-btn:active { transform: scale(0.97); }
.ghost-btn.danger:hover {
  border-color: #ef4444;
  color: #ef4444;
  background: rgba(239, 68, 68, 0.05);
}

/* 危险按钮 */
.danger-btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  border: 1px solid rgba(239, 68, 68, 0.4);
  background: rgba(239, 68, 68, 0.04);
  color: #ef4444;
  padding: 7px 14px;
  border-radius: 8px;
  font-size: 13px;
  cursor: pointer;
  transition: all 0.15s;
}
.danger-btn:hover {
  background: rgba(239, 68, 68, 0.1);
  border-color: #ef4444;
}
.danger-btn:active { transform: scale(0.97); }
.danger-hint {
  font-size: 12px;
  color: #ef4444;
  margin: 8px 0 0;
  line-height: 1.5;
  opacity: 0.8;
}

/* 按钮行 */
.btn-row {
  display: flex;
  gap: 8px;
  flex-wrap: wrap;
  margin-top: 4px;
}
.btn-row-end { justify-content: flex-end; }

/* 自动备份：路径选择行 + 上次备份行 */
.path-row {
  display: flex;
  gap: 8px;
  align-items: center;
}
.path-row .text-input { flex: 1; max-width: none; }
.path-row .ghost-btn { flex-shrink: 0; }
.form-row.row.between { justify-content: space-between; }
.last-backup {
  font-size: 12.5px;
  color: var(--text-dim, #86868b);
}
.last-backup b { color: var(--text); font-weight: 600; }

/* 设备 ID 条 */
.id-bar {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}
.id-mono {
  flex: 1;
  min-width: 200px;
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: 13px;
  padding: 8px 12px;
  background: var(--bg, #f5f5f7);
  border: 1px solid var(--border);
  border-radius: 8px;
  color: var(--text);
  word-break: break-all;
}
.id-actions { display: flex; gap: 6px; flex-shrink: 0; }

/* 子区（缩进 + 顶部虚线） */
.sub-zone {
  margin-top: 16px;
  padding-top: 12px;
  border-top: 1px dashed rgba(0, 0, 0, 0.08);
}
.sub-zone.disabled {
  opacity: 0.45;
  pointer-events: none;
}
/* v0.7.6：状态栏子项复选框（基础原生 checkbox + 大点击区） */
.check-input {
  width: 18px;
  height: 18px;
  cursor: pointer;
  accent-color: #3b82f6;
}
.check-input:disabled {
  cursor: not-allowed;
}
/* v0.7.6：非 macOS 平台内存项禁用态（配合后端 MEMORY_SUPPORTED） */
.op-muted {
  opacity: 0.5;
}
.op-muted .check-input {
  cursor: not-allowed;
}
.platform-note {
  margin-left: 6px;
  font-size: 11px;
  color: var(--text-dim, #86868b);
  font-weight: 400;
}
.sub-zone h4 {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 13px;
  font-weight: 600;
  margin: 0 0 6px;
  color: var(--text);
}

/* 关于区（meta 行） */
.about-list { display: flex; flex-direction: column; gap: 0; }
.meta-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 8px 0;
  font-size: 13px;
  border-bottom: 1px solid var(--border);
}
.meta-row:last-child { border-bottom: none; }
.meta-label { color: var(--text-dim, #86868b); }
.meta-value { color: var(--text); font-weight: 500; }
.meta-value.mono {
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  font-size: 12.5px;
}

/* 桌宠卡 */
.pet-hungry {
  color: var(--accent, #ff7e27);
  font-weight: 500;
  font-size: 12.5px;
  margin: 8px 0 0;
  display: flex;
  align-items: center;
  gap: 6px;
}

/* 皮肤选择器 */
.pet-skin-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 8px;
  margin-top: 8px;
}
@media (max-width: 480px) {
  .pet-skin-grid { grid-template-columns: 1fr; }
}
.pet-skin-tile {
  appearance: none;
  border: 1.5px solid var(--border, #e5e7eb);
  border-radius: 10px;
  padding: 10px 12px;
  text-align: left;
  background: var(--bg, #fff);
  color: inherit;
  cursor: pointer;
  transition: border-color 0.15s, background 0.15s, transform 0.05s;
  display: flex;
  flex-direction: column;
  gap: 4px;
  font: inherit;
}
.pet-skin-tile:hover { border-color: rgba(255, 126, 39, 0.45); }
.pet-skin-tile:active { transform: scale(0.98); }
.pet-skin-tile.is-active {
  border-color: var(--accent, #ff7e27);
  background: rgba(255, 126, 39, 0.06);
  box-shadow: 0 0 0 3px rgba(255, 126, 39, 0.10);
}
.pet-skin-head { display: flex; align-items: center; gap: 6px; }
/* v1.2.2: emoji 换成 AppIcon 矢量图标，颜色随主题橙 */
.pet-skin-icon { color: #FF7E27; }
.pet-skin-name { font-weight: 600; font-size: 13px; }
.pet-skin-desc { font-size: 11px; opacity: 0.7; line-height: 1.3; }

/* 危险区卡片 */
.danger-card {
  border-color: rgba(239, 68, 68, 0.25);
  background: linear-gradient(to bottom, var(--card), rgba(239, 68, 68, 0.02));
}

/* Toggle Switch（iOS 风） */
.toggle-switch {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  cursor: pointer;
  user-select: none;
  -webkit-user-select: none;
  flex-shrink: 0;
  white-space: nowrap;
}
.toggle-switch input {
  opacity: 0;
  width: 0;
  height: 0;
  position: absolute;
}
.toggle-slider {
  display: inline-block; /* span 默认 inline 会让 width/height 失效，必须显式声明 */
  position: relative;
  width: 44px;
  height: 24px;
  flex-shrink: 0;
  background: #d1d1d6;
  border-radius: 12px;
  transition: background 0.25s ease;
  vertical-align: middle;
}
.toggle-slider::after {
  content: '';
  position: absolute;
  top: 2px;
  left: 2px;
  width: 20px;
  height: 20px;
  background: #fff;
  border-radius: 50%;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.2);
  transition: transform 0.25s ease;
}
.toggle-switch.on .toggle-slider { background: var(--accent, #ff7e27); }
.toggle-switch.on .toggle-slider::after { transform: translateX(20px); }
.toggle-state {
  font-size: 12.5px;
  color: var(--text-dim, #86868b);
  font-weight: 500;
}
.toggle-switch.on .toggle-state { color: var(--accent, #ff7e27); }

/* 更新结果行 */
.outdated { color: var(--accent, #FF7E27) !important; }
.link-btn {
  margin-left: 8px;
  background: none;
  border: 1px solid var(--accent, #FF7E27);
  color: var(--accent, #FF7E27);
  padding: 2px 10px;
  border-radius: 6px;
  font-size: 12px;
  cursor: pointer;
}
.link-btn:hover { background: var(--accent, #FF7E27); color: #fff; }

/* 按设备清理弹窗内设备列表 */
.device-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
  max-height: 360px;
  overflow: auto;
  padding: 4px 2px;
}
.device-row {
  display: flex;
  align-items: flex-start;
  gap: 10px;
  padding: 8px 10px;
  border: 1px solid var(--border);
  border-radius: 8px;
  cursor: pointer;
  transition: border-color 0.15s, background 0.15s;
}
.device-row:hover { background: var(--bg-soft, rgba(0, 0, 0, 0.03)); }
.device-row.checked {
  border-color: var(--accent, #FF7E27);
  background: rgba(255, 126, 39, 0.06);
}
.device-row input[type="checkbox"] { margin-top: 4px; cursor: pointer; }
.device-info { flex: 1; min-width: 0; }
.device-name {
  font-weight: 600;
  font-size: 13px;
  display: flex;
  align-items: center;
  gap: 8px;
}
.device-meta {
  font-size: 11px;
  color: var(--text-dim);
  margin-top: 4px;
  display: flex;
  gap: 4px;
  flex-wrap: wrap;
}
.self-tag {
  font-size: 10px;
  padding: 1px 6px;
  border-radius: 8px;
  background: var(--accent, #FF7E27);
  color: #fff;
}
.default-tag {
  font-size: 10px;
  padding: 1px 6px;
  border-radius: 8px;
  background: rgba(192, 57, 43, 0.12);
  color: #c0392b;
}
.empty {
  text-align: center;
  color: var(--text-dim);
  padding: 24px 0;
}

/* ===== v0.8.0：屏幕截图卡片 ===== */
.zone-title {
  margin: 14px 0 6px;
  font-size: 12px;
  font-weight: 600;
  color: var(--muted);
}

/* ============ v1.2.10：权限中心卡片 ============ */
/* 刷新按钮推到卡片头右端 —— 它是「查询」不是「操作」，不和下面的授权按钮抢视觉位。 */
.perm-refresh {
  margin-left: auto;
  flex-shrink: 0;
  padding: 5px 10px;
  font-size: 12px;
}
.perm-row + .perm-row {
  margin-top: 14px;
  padding-top: 12px;
  border-top: 1px dashed rgba(0, 0, 0, 0.08);
}
.perm-main {
  display: flex;
  align-items: center;
  gap: 10px;
  flex-wrap: wrap;
}
.perm-label {
  font-size: 13px;
  font-weight: 500;
  color: var(--text);
}
.perm-badge {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  padding: 2px 8px;
  border-radius: 999px;
  font-size: 12px;
  font-weight: 500;
  line-height: 1.5;
}
/* 颜色走 style.css 的语义变量，深浅色模式自动跟随（不硬编码绿/红） */
.perm-badge.ok {
  color: var(--success);
  background: rgba(52, 199, 89, 0.1);
}
.perm-badge.bad {
  color: var(--danger);
  background: rgba(255, 59, 48, 0.09);
}
.perm-badge.pending {
  color: var(--text-dim, #86868b);
  background: rgba(0, 0, 0, 0.04);
}
.perm-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: currentColor;
  flex-shrink: 0;
}
.perm-actions {
  margin-left: auto;
  display: flex;
  gap: 8px;
  flex-wrap: wrap;
}
.perm-hint-warn {
  color: var(--danger);
  opacity: 0.85;
  margin-top: 10px;
}
.perm-hint-nomac {
  margin-top: 0;
}
.dir-row {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
  flex: 1;
  justify-content: flex-end;
}
.path-text {
  max-width: 300px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 12px;
  color: var(--muted);
}
.range-input {
  width: 180px;
  accent-color: var(--accent);
}
/* ===== v0.9.0：取字引擎选择（胶囊式卡片，与语言选择同风格） ===== */
.engine-picker {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 10px;
  margin-top: 6px;
}
.engine-opt {
  display: flex;
  flex-direction: column;
  gap: 3px;
  padding: 9px 11px;
  border: 1px solid var(--border);
  border-radius: 10px;
  background: var(--card);
  cursor: pointer;
  transition: border-color 0.15s, background 0.15s;
}
.engine-opt:hover:not(.off) {
  border-color: var(--accent);
}
.engine-opt.on {
  border-color: var(--accent);
  background: var(--accent-soft, var(--seg-bg));
}
/* 资源缺失：置灰不可选（禁用态要让用户一眼看出「不是我不想给，是这儿没有」） */
.engine-opt.off {
  opacity: 0.5;
  cursor: not-allowed;
}
.engine-opt input {
  display: none;
}
.engine-opt b {
  font-size: 13px;
  font-weight: 600;
  color: var(--text);
}
.engine-opt span {
  font-size: 11px;
  line-height: 1.45;
  color: var(--muted);
}
.shot-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
  gap: 12px;
  margin-top: 8px;
}
.shot-item {
  /* v0.9.7 修复：勾选框 .shot-check 是 absolute 定位，锚点必须是卡片自身。
     原实现漏了这行 → 勾选框全部脱锚到页面左上角叠成一团，
     卡片上「看起来该点的位置」什么都没有 → 单选点不动、只有全选能用。 */
  position: relative;
  margin: 0;
  border: 1px solid var(--border);
  border-radius: 10px;
  overflow: hidden;
  background: var(--card);
}
.shot-img {
  display: block;
  width: 100%;
  height: 108px;
  object-fit: cover;
  background: var(--seg-bg);
}
.shot-img--empty {
  background: repeating-linear-gradient(
    45deg,
    var(--seg-bg),
    var(--seg-bg) 8px,
    var(--border) 8px,
    var(--border) 16px
  );
}
.shot-cap {
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding: 8px 10px;
}
.shot-meta {
  font-size: 11px;
  color: var(--muted);
}
.shot-date {
  color: var(--text-dim);
}
.shot-actions {
  display: flex;
  gap: 6px;
  margin-top: 2px;
}
.shot-actions .ghost-btn,
.shot-actions .danger-btn {
  padding: 2px 8px;
  font-size: 11px;
}
/* v0.9.1：截图历史多选工具栏与勾选框 */
.shot-toolbar {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 8px;
  margin-top: 8px;
}
.shot-sel-count {
  font-size: 12px;
  color: var(--muted);
}
.shot-item.selected {
  border-color: var(--accent);
  box-shadow: 0 0 0 2px var(--accent) inset;
}
/* v0.9.7：多选模式下整卡可点（点图片任意位置 = 切换勾选），光标给提示 */
.shot-item.selectable {
  cursor: pointer;
}
.shot-check {
  position: absolute;
  top: 6px;
  left: 6px;
  z-index: 2;
  display: flex;
  align-items: center;
  justify-content: center;
  width: 22px;
  height: 22px;
  background: rgba(0, 0, 0, 0.45);
  border-radius: 6px;
  cursor: pointer;
}
.shot-check input {
  width: 15px;
  height: 15px;
  cursor: pointer;
}
</style>
