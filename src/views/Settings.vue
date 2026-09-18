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
              :disabled="!statusBarConfig.enabled"
              @change="onStatusBarItem($event, 'show_cpu')"
            />
          </div>
          <div class="form-row row between">
            <label>{{ t("settings.statusBarShowMem") }}</label>
            <input
              type="checkbox"
              class="check-input"
              :checked="statusBarConfig.show_mem"
              :disabled="!statusBarConfig.enabled"
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
              :disabled="!statusBarConfig.enabled"
              @change="onStatusBarItem($event, 'show_disk')"
            />
          </div>
          <div class="form-row row between">
            <label>{{ t("settings.statusBarShowNet") }}</label>
            <input
              type="checkbox"
              class="check-input"
              :checked="statusBarConfig.show_net"
              :disabled="!statusBarConfig.enabled"
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
              :disabled="!statusBarConfig.enabled"
              @change="onStatusBarItem($event, 'float_enabled')"
            />
          </div>
          <p class="field-hint platform-note">{{ t("settings.statusBarFloatHint") }}</p>
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
          <div class="form-row row between">
            <label>{{ t("settings.shotShadow") }}</label>
            <input
              type="checkbox"
              class="check-input"
              :checked="shotConfig.shadow"
              :disabled="!shotConfig.enabled"
              @change="onShotToggle($event, 'shadow')"
            />
          </div>
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

          <div class="form-row row between">
            <label>{{ t("settings.shotNow") }}</label>
            <button class="primary-btn" :disabled="!shotConfig.enabled" @click="onShotNow">
              {{ t("settings.shotNow") }}
            </button>
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
              :class="{ selected: shotSelecting && selectedShotIds.includes(s.id) }"
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
                <span class="pet-skin-emoji" :aria-hidden="true">
                  {{ s.id === 'popmart-3d' ? '🎁' : '🐾' }}
                </span>
                <span class="pet-skin-name">{{ s.name }}</span>
              </div>
              <div class="pet-skin-desc">{{ s.description }}</div>
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
    />

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
  ScreenshotConfig,
  ScreenshotOut,
  OcrEngineInfo,
  OcrEngineKind,
} from "../types";
import { formatDuration } from "../utils/format";
import { petStore } from "../pet/stores/petStore";
import PetSpriteEditor from "../pet/components/PetSpriteEditor.vue";
import { skinRegistry } from "../pet/skins/registry";
import "../pet/skins";

const { t } = useI18n();

// 桌宠编辑器显示状态
const showEditor = ref(false);

// v0.6.2-beta.2：皮肤选择器
const skinList = computed(() => skinRegistry.list());
const activeSkinId = computed(() => skinRegistry.active().id);
function pickSkin(id: string): void {
  skinRegistry.setActive(id);
}

// v0.6.2-beta.3：跨窗口皮肤同步
let unlistenSkin: (() => void) | null = null;
let unlistenPetEnabled: (() => void) | null = null;
// v0.7.6（2026-09-10）：托盘右键快捷开关 ↔ 设置页双向同步
let unlistenStatusBarCfg: (() => void) | null = null;
onMounted(async () => {
  try {
    unlistenSkin = await listen("pet-skin-changed", () => {
      skinRegistry.reloadActive();
    });
  } catch (e) {
    console.error("[Settings] 监听 pet-skin-changed 失败", e);
  }
  try {
    unlistenPetEnabled = await listen("pet-enabled-changed", () => {
      petStore.reload();
    });
  } catch (e) {
    console.error("[Settings] 监听 pet-enabled-changed 失败", e);
  }
  // 托盘菜单勾选后后端 emit 此事件，设置页卡片即时反映（避免两处状态不一致）
  try {
    unlistenStatusBarCfg = await listen<StatusBarConfig>(
      "status-bar-config-changed",
      (e) => {
        statusBarConfig.value = e.payload;
      },
    );
  } catch (e) {
    console.error("[Settings] 监听 status-bar-config-changed 失败", e);
  }
});
onBeforeUnmount(() => {
  if (unlistenSkin) unlistenSkin();
  if (unlistenPetEnabled) unlistenPetEnabled();
  if (unlistenStatusBarCfg) unlistenStatusBarCfg();
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
        console.error("[pet] 切换桌宠失败", err);
        showAlert("warn", t("common.error"), String(err));
      });
  } else {
    invoke("hide_pet_window").catch((err) => {
      console.error("[pet] 隐藏桌宠失败", err);
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
    console.error("[pet] 显示桌宠失败", err);
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
    console.error("[pet] 隐藏桌宠失败", err);
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

async function loadBackupConfig() {
  try {
    backupConfig.value = await tracker.getBackupConfig();
  } catch {
    /* 浏览器预览模式忽略 */
  }
}

async function onToggleAutoBackup(e: Event) {
  const next = (e.target as HTMLInputElement).checked;
  backupConfig.value.enabled = next; // 乐观更新
  await persistBackupConfig();
}

async function chooseBackupFolder() {
  try {
    const selected = await open({ directory: true, multiple: false });
    if (typeof selected === "string" && selected) {
      backupConfig.value.path = selected;
      await persistBackupConfig();
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
let pendingConfirm: (() => void | Promise<void>) | null = null;
function showAlert(
  type: "info" | "confirm" | "warn",
  title: string,
  msg: string,
  onConfirm?: () => void | Promise<void>,
  confirmText?: string
) {
  alertType.value = type;
  alertTitle.value = title;
  alertMsg.value = msg;
  alertConfirmText.value = confirmText ?? "";
  pendingConfirm = onConfirm ?? null;
  alertOpen.value = true;
}
function onAlertConfirm() {
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
});
async function loadStatusBarConfig() {
  try {
    statusBarConfig.value = await tracker.getStatusBarConfig();
  } catch (err) {
    console.warn("[Settings] 加载状态栏配置失败", err);
  }
}
// 乐观更新：先翻 ref 再 await IPC，失败回滚（与 onAutostart 模式一致）
async function onStatusBarEnabled(e: Event) {
  const next = (e.target as HTMLInputElement).checked;
  const prev = statusBarConfig.value.enabled;
  statusBarConfig.value = { ...statusBarConfig.value, enabled: next };
  try {
    await tracker.setStatusBarConfig(statusBarConfig.value);
  } catch (err) {
    statusBarConfig.value = { ...statusBarConfig.value, enabled: prev };
    showAlert(
      "warn",
      t("settings.statusBarOff"),
      err instanceof Error ? err.message : String(err),
    );
  }
}
async function onStatusBarItem(
  e: Event,
  key: "show_cpu" | "show_mem" | "show_disk" | "show_net" | "float_enabled",
) {
  const next = (e.target as HTMLInputElement).checked;
  const prev = statusBarConfig.value[key];
  statusBarConfig.value = { ...statusBarConfig.value, [key]: next };
  try {
    await tracker.setStatusBarConfig(statusBarConfig.value);
  } catch (err) {
    statusBarConfig.value = { ...statusBarConfig.value, [key]: prev };
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

async function loadShotConfig() {
  try {
    shotConfig.value = await screenshot.getConfig();
    shotDir.value = await screenshot.dir();
  } catch (err) {
    console.warn("[Settings] 加载截图配置失败", err);
  }
}

// ---- v0.9.0：取字引擎 ----
/** 引擎资源探测结果（决定「增强」选项是否可选，以及提示文案） */
const ocrInfo = ref<OcrEngineInfo | null>(null);

async function loadOcrEngineInfo() {
  try {
    ocrInfo.value = await screenshot.ocrEngineInfo();
  } catch (err) {
    console.warn("[Settings] 读取取字引擎信息失败", err);
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
  shotConfig.value = { ...shotConfig.value, ocr_engine: kind };
  try {
    await screenshot.setConfig(shotConfig.value);
    // 切换后重新探测一次：后端会顺带回报资源状态与当前生效引擎
    await loadOcrEngineInfo();
  } catch (err) {
    shotConfig.value = { ...shotConfig.value, ocr_engine: prev };
    showAlert("warn", t("settings.shotTitle"), err instanceof Error ? err.message : String(err));
  }
}

/** 数值类改动：先乐观写 ref，再 await IPC，失败则回读真实配置纠偏 */
async function persistShotConfig() {
  const snapshot = { ...shotConfig.value };
  try {
    const res = await screenshot.setConfig(snapshot);
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
  shotConfig.value = { ...shotConfig.value, shortcut: combo };
  try {
    const res = await screenshot.setConfig(shotConfig.value);
    if (res.applied) {
      hotkeyError.value = "";
      return;
    }
    hotkeyError.value = `${t("settings.shotShortcutConflict")} ${res.error ?? ""}`;
    shotConfig.value = { ...shotConfig.value, shortcut: prev };
    await screenshot.setConfig(shotConfig.value);
  } catch (err) {
    hotkeyError.value = err instanceof Error ? err.message : String(err);
    shotConfig.value = { ...shotConfig.value, shortcut: prev };
  }
}

async function resetHotkey() {
  await applyHotkey(DEFAULT_SCREENSHOT_CONFIG.shortcut);
}


async function onShotEnabled(e: Event) {
  const next = (e.target as HTMLInputElement).checked;
  const prev = shotConfig.value.enabled;
  shotConfig.value = { ...shotConfig.value, enabled: next };
  try {
    await screenshot.setConfig(shotConfig.value);
  } catch (err) {
    shotConfig.value = { ...shotConfig.value, enabled: prev };
    showAlert("warn", t("settings.shotTitle"), err instanceof Error ? err.message : String(err));
  }
}

async function onShotToggle(e: Event, key: "auto_save" | "shadow") {
  const next = (e.target as HTMLInputElement).checked;
  const prev = shotConfig.value[key];
  shotConfig.value = { ...shotConfig.value, [key]: next };
  try {
    await screenshot.setConfig(shotConfig.value);
  } catch (err) {
    shotConfig.value = { ...shotConfig.value, [key]: prev };
    showAlert("warn", t("settings.shotTitle"), err instanceof Error ? err.message : String(err));
  }
}

async function onShotNow() {
  try {
    await screenshot.trigger();
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
    shotHistory.value = await screenshot.list(60);
    // 缩略图并发拉取（单张 ~30–80KB，走 data URL 避免额外协议/config 改动）
    const pairs = await Promise.all(
      shotHistory.value.map(async (s) => [s.id, await screenshot.thumbnail(s.id, 320)] as const),
    );
    const map: Record<number, string> = {};
    for (const [id, url] of pairs) {
      if (url) map[id] = url;
    }
    shotThumbs.value = map;
  } catch (err) {
    console.warn("[Settings] 加载截图历史失败", err);
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

// 截图完成 → 刷新历史（主窗口收到 Rust 的 screenshot-done 事件）
onMounted(async () => {
  await loadShotConfig();
  await loadOcrEngineInfo();
  await loadShotHistory();
  try {
    await listen("screenshot-done", () => {
      void loadShotHistory();
    });
    // v0.9.1：截图失败**必须让用户看见原因**。
    // Rust 侧一直有 `emit_to("main", "screenshot-error", e)`，但前端没有任何监听 →
    // 失败是静默的。macOS 缺「屏幕录制」权限时表现最典型：用户只看到「截出来只剩桌面」，
    // 既没有报错，也不知道该去哪里授权。
    await listen<string>("screenshot-error", (ev) => {
      const msg = ev.payload ?? "";
      const isPerm = msg.includes("屏幕录制");
      showAlert(
        "warn",
        isPerm ? t("settings.shotPermTitle") : t("settings.shotTitle"),
        msg,
        isPerm ? () => void tracker.openPrivacySettings("screen_capture") : undefined,
        isPerm ? t("settings.shotPermOpen") : undefined
      );
    });
  } catch {
    /* 非 Tauri 环境 */
  }
});

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
    console.error("按设备清理失败", e);
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
        console.error("清理失败", e);
        showAlert("warn", t("settings.cleanFailed"), t("settings.cleanFailedMsg", { err: e instanceof Error ? e.message : String(e) }));
      }
    }
  );
}

// （v0.6.2-beta.19 移除"重置设备 ID"按钮：后端未实现 reset_device_id 命令，截图里的"重置"按钮只用作视觉示意，保留会让用户点了报错。设备 ID 不可重置：它绑定了 sessions 数据。）

onMounted(async () => {
  try {
    version.value = await getVersion();
  } catch {
    /* 浏览器预览模式忽略 */
  }
  try {
    const s = await tracker.getSettings();
    settings.value = s;
    deviceName.value = s.device_name;
    idleMin.value = Math.max(1, Math.round(s.idle_threshold / 60));
    retention.value = s.data_retention_days;
    autostart.value = s.autostart;
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
});

// 切换开机自启（v0.6.2-beta.23：去掉成功弹窗；v0.6.2-beta.24：乐观更新 + 失败回滚）
// 关键：<input> 是单向 :checked 绑定，vue 下次 render 会用 autostart.value 强制覆盖原生 checked，
// 若这里不翻转 autostart.value，toggle 会"弹回"，表现成"点不动"。故先乐观翻转，失败再回滚。
async function onAutostart(e: Event) {
  const next = (e.target as HTMLInputElement).checked;
  autostart.value = next; // 乐观更新：立即反映到 :checked 与 :class="{ on }"
  try {
    await tracker.setAutostart(next);
  } catch (err) {
    autostart.value = !next; // 失败回滚
    showAlert("warn", t("settings.autostartFailed"), t("settings.autostartFailedMsg", { err: err instanceof Error ? err.message : String(err) }));
  }
}

async function onSave() {
  try {
    await tracker.saveSettings({
      idleThreshold: idleMin.value * 60,
      deviceName: deviceName.value.trim() || settings.value.device_name,
      dataRetentionDays: retention.value,
    });
    showAlert("info", t("settings.saved"), t("settings.savedMsg"));
  } catch (e) {
    console.error("保存设置失败", e);
    showAlert("warn", t("settings.saveFailed"), t("settings.saveFailedMsg", { err: e instanceof Error ? e.message : String(e) }));
  }
}

async function onExport() {
  try {
    const res = await tracker.exportAll();
    exportPath.value = res.path;
    exportDialogOpen.value = true;
  } catch (e) {
    console.error("导出失败", e);
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
    console.error("读取日志大小失败", e);
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
    console.error("导出日志失败", e);
    showAlert("warn", t("settings.exportFailed"), t("settings.exportFailedMsg", { err: e instanceof Error ? e.message : String(e) }));
  }
}

async function revealLogDir() {
  try {
    const logDir = await invoke<string>("get_log_dir");
    await tracker.revealPath(logDir);
  } catch (e) {
    console.error("打开日志目录失败", e);
    showAlert("warn", t("settings.openFailed"), t("settings.openFailedMsg", { err: e instanceof Error ? e.message : String(e) }));
  }
}

onMounted(() => {
  void refreshLogSize();
});

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
    console.error("导入失败", err);
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
      showAlert("info", t("settings.upToDate"), t("settings.upToDateMsg", { current: updateResult.value.current }));
    }
  } catch (e: any) {
    console.error("检查更新失败", e);
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

/* 桌宠卡（粉色高亮） */
.pet-card {
  border-left: 3px solid #ff6b9d;
}
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
.pet-skin-emoji { font-size: 18px; line-height: 1; }
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
