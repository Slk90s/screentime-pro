<template>
  <!-- 分类规则管理视图
       负责：展示/新增/编辑/删除「自动归类规则」，并一键按规则重算历史数据。
       v0.3.1：新增/编辑改成弹窗（避免长规则看不到完成提醒）
       注：开机自启开关已移至「设置」页（Settings.vue），此处不再重复。 -->
  <div class="rules-view">
    <!-- ============ 规则列表 ============ -->
    <section class="card">
      <div class="head">
        <h3>{{ t("rules.title") }}</h3>
        <div class="head-actions">
          <button class="primary" @click="openCreate">{{ t("rules.add") }}</button>
          <button @click="doReclassify">{{ t("rules.reclassify") }}</button>
        </div>
      </div>
      <p class="hint">
        {{ t("rules.hint") }}
      </p>

      <table class="rule-table">
        <thead>
          <tr>
            <th class="sortable" @click="setSort('field')">
              {{ t("rules.colField") }}<span class="sort-ind">{{ sortMark("field") }}</span>
            </th>
            <th class="sortable" @click="setSort('match_type')">
              {{ t("rules.colMatch") }}<span class="sort-ind">{{ sortMark("match_type") }}</span>
            </th>
            <th class="sortable" @click="setSort('pattern')">
              {{ t("rules.colPattern") }}<span class="sort-ind">{{ sortMark("pattern") }}</span>
            </th>
            <th class="sortable" @click="setSort('category_id')">
              {{ t("rules.colCategory") }}<span class="sort-ind">{{ sortMark("category_id") }}</span>
            </th>
            <th class="sortable" @click="setSort('priority')">
              {{ t("rules.colPriority") }}<span class="sort-ind">{{ sortMark("priority") }}</span>
            </th>
            <th class="sortable" @click="setSort('enabled')">
              {{ t("rules.colEnabled") }}<span class="sort-ind">{{ sortMark("enabled") }}</span>
            </th>
            <th>{{ t("rules.colOps") }}</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="r in sortedRules" :key="r.id">
            <td>{{ t("rules.field." + r.field) }}</td>
            <td>{{ t("rules.match." + r.match_type) }}</td>
            <td class="pattern">{{ r.pattern }}</td>
            <td>
              <span class="cat-tag" :style="{ background: catColor(r.category_id) }">
                {{ catName(r.category_id) }}
              </span>
            </td>
            <td>{{ r.priority }}</td>
            <td>{{ r.enabled ? t("rules.yes") : t("rules.no") }}</td>
            <td class="ops">
              <button @click="openEdit(r)">{{ t("rules.edit") }}</button>
              <button class="danger" @click="confirmRemove(r.id, r.pattern)">{{ t("rules.remove") }}</button>
            </td>
          </tr>
          <tr v-if="rules.length === 0">
            <td colspan="7" class="empty">{{ t("rules.empty") }}</td>
          </tr>
        </tbody>
      </table>
    </section>

    <!-- ============ 新增/编辑 弹窗 ============ -->
    <Modal
      v-model="formOpen"
      :type="'info'"
      :title="editingId === null ? t('rules.createTitle') : t('rules.editTitle', { id: editingId })"
      :confirm-text="editingId === null ? t('rules.addBtn') : t('rules.saveBtn')"
      :cancel-text="t('common.cancel')"
      @confirm="submit"
      width="560px"
    >
      <div class="form-grid">
        <label>
          {{ t("rules.lblField") }}
          <select v-model="form.field">
            <option value="process_name">{{ t("rules.field.process_name") }}</option>
            <option value="window_title">{{ t("rules.field.window_title") }}</option>
            <option value="exe_path">{{ t("rules.field.exe_path") }}</option>
            <option value="bundle_id">{{ t("rules.field.bundle_id") }}</option>
            <option value="name">{{ t("rules.field.name") }}</option>
          </select>
        </label>
        <label>
          {{ t("rules.lblMatch") }}
          <select v-model="form.match_type">
            <option value="contains">{{ t("rules.match.contains") }}</option>
            <option value="equals">{{ t("rules.match.equals") }}</option>
            <option value="prefix">{{ t("rules.match.prefix") }}</option>
            <option value="suffix">{{ t("rules.match.suffix") }}</option>
            <option value="regex">{{ t("rules.match.regex") }}</option>
          </select>
        </label>
        <label class="grow">
          {{ t("rules.lblPattern") }}
          <input v-model="form.pattern" :placeholder="t('rules.patternPh')" />
        </label>
        <label>
          {{ t("rules.lblCategory") }}
          <select v-model="form.category_id">
            <option v-for="c in categories" :key="c.id" :value="c.id">{{ catName(c.id) }}</option>
          </select>
        </label>
        <label>
          {{ t("rules.lblPriority") }}
          <input type="number" v-model.number="form.priority" />
        </label>
        <label class="check">
          <input type="checkbox" v-model="form.enabled" /> {{ t("rules.lblEnabled") }}
        </label>
      </div>
    </Modal>

    <!-- 通用提示/确认弹窗 -->
    <Modal
      v-model="alertOpen"
      :type="alertType"
      :title="alertTitle"
      :message="alertMsg"
      :confirm-text="t('common.confirm')"
      :cancel-text="alertType === 'info' ? '' : t('common.cancel')"
      width="380px"
      @confirm="onAlertConfirm"
    />
  </div>
</template>

<script setup lang="ts">
// 分类规则管理界面
// 功能：
// 1. 规则增删改查（get_rules / add_rule / update_rule / delete_rule）
// 2. 一键按规则重算历史分类（reclassify_all）
// 注：开机自启开关已移至 Settings.vue，本页只管分类规则。
// 所有代码均带中文注释，符合项目「新增界面必须加注释」的规范。

import { ref, computed, onMounted } from "vue";
import { useI18n } from "vue-i18n";
import Modal from "../components/Modal.vue";
import { tracker } from "../api/tracker";
import { categoryName } from "../i18n/categories";
import type { CategoryOut, RuleOut } from "../types";

const { t } = useI18n();

// 规则列表与分类字典
const rules = ref<RuleOut[]>([]);
const categories = ref<CategoryOut[]>([]);

// v0.6.2-beta.16：表格排序状态
// - 默认按分类（category_id）升序分组 + 同分类内按匹配值（pattern）字母序
// - 单击列头：设为该列升序；再次点击该列：切降序；点击其他列：切到该列升序
type SortKey =
  | "field"
  | "match_type"
  | "pattern"
  | "category_id"
  | "priority"
  | "enabled";
type SortDir = "asc" | "desc";
const sortKey = ref<SortKey>("category_id");
const sortDir = ref<SortDir>("asc");
function setSort(k: SortKey) {
  if (sortKey.value === k) {
    // 同列再次点击：切换升降序
    sortDir.value = sortDir.value === "asc" ? "desc" : "asc";
  } else {
    // 切到新列：升序
    sortKey.value = k;
    sortDir.value = "asc";
  }
}
function sortMark(k: SortKey): string {
  if (sortKey.value !== k) return "";
  return sortDir.value === "asc" ? " ▲" : " ▼";
}

// 按当前 sortKey/sortDir 排序的规则列表（默认 category_id asc + pattern asc）
const sortedRules = computed<RuleOut[]>(() => {
  const list = rules.value.slice();
  const k = sortKey.value;
  const dir = sortDir.value === "asc" ? 1 : -1;
  list.sort((a, b) => {
    const av = (a as any)[k];
    const bv = (b as any)[k];
    // 字符串走 localeCompare（兼顾中文 + 大小写不敏感）；布尔/数字按数值
    let cmp: number;
    if (typeof av === "boolean" && typeof bv === "boolean") {
      cmp = (av === bv ? 0 : av ? 1 : -1);
    } else if (typeof av === "number" && typeof bv === "number") {
      cmp = av - bv;
    } else {
      cmp = String(av ?? "").localeCompare(String(bv ?? ""), "zh-CN", {
        sensitivity: "base",
      });
    }
    if (cmp !== 0) return cmp * dir;
    // 同主键再按 pattern asc 兜底（看匹配值字母序）
    if (k !== "pattern") {
      const pa = String((a as any).pattern ?? "");
      const pb = String((b as any).pattern ?? "");
      const pcmp = pa.localeCompare(pb, "zh-CN", { sensitivity: "base" });
      if (pcmp !== 0) return pcmp;
    }
    return a.id - b.id;
  });
  return list;
});

// 弹窗控制
// - formOpen: 新增/编辑表单弹窗
// - alertOpen: 通用提示/确认弹窗
const formOpen = ref(false);
const alertOpen = ref(false);
const alertType = ref<"info" | "confirm" | "warn">("info");
const alertTitle = ref("");
const alertMsg = ref("");
/** 通用弹窗：type=info 时只有确定按钮，type=confirm 时确认触发 onConfirm */
let pendingConfirm: (() => void | Promise<void>) | null = null;
function showAlert(
  type: "info" | "confirm" | "warn",
  title: string,
  msg: string,
  onConfirm?: () => void | Promise<void>
) {
  alertType.value = type;
  alertTitle.value = title;
  alertMsg.value = msg;
  pendingConfirm = onConfirm ?? null;
  alertOpen.value = true;
}

// 表单状态：editingId 为 null 表示新增，否则为正在编辑的规则 id
const editingId = ref<number | null>(null);
const form = ref({
  field: "process_name",
  match_type: "contains",
  pattern: "",
  category_id: "other",
  priority: 0,
  enabled: true,
});

// 加载规则与分类
async function loadAll() {
  rules.value = await tracker.rules();
  categories.value = await tracker.categories();
}

// 分类名（按 id 经 i18n 取本地化显示名；未知 id 回退到 id）
function catName(id: string): string {
  return categoryName(id);
}
// 分类颜色（用于标签背景）
function catColor(id: string): string {
  return categories.value.find((c) => c.id === id)?.color || "#888780";
}

// 提交：新增或保存修改（Modal @confirm 触发）
async function submit() {
  const f = form.value;
  if (!f.pattern.trim()) {
    showAlert("warn", t("rules.validateFail"), t("rules.patternEmpty"));
    return;
  }
  try {
    if (editingId.value === null) {
      await tracker.addRule({
        field: f.field,
        matchType: f.match_type,
        pattern: f.pattern.trim(),
        categoryId: f.category_id,
        priority: f.priority,
      });
      showAlert("info", t("rules.added"), t("rules.addedMsg", { pattern: f.pattern }));
    } else {
      await tracker.updateRule({
        id: editingId.value,
        field: f.field,
        matchType: f.match_type,
        pattern: f.pattern.trim(),
        categoryId: f.category_id,
        priority: f.priority,
        enabled: f.enabled,
      });
      showAlert("info", t("rules.updated"), t("rules.updatedMsg", { id: editingId.value }));
    }
    await loadAll();
    resetForm();
    formOpen.value = false;
  } catch (e: any) {
    console.error("[Rules] submit failed:", e);
    showAlert("warn", t("rules.opFailed"), t("rules.opFailedMsg", { err: e?.message || e }));
  }
}

// 打开「新增」弹窗
function openCreate() {
  resetForm();
  formOpen.value = true;
}
// 打开「编辑」弹窗（载入某条规则到表单）
function openEdit(r: RuleOut) {
  editingId.value = r.id;
  form.value = {
    field: r.field,
    match_type: r.match_type,
    pattern: r.pattern,
    category_id: r.category_id,
    priority: r.priority,
    enabled: r.enabled,
  };
  formOpen.value = true;
}

// 重置表单到「新增」状态
function resetForm() {
  editingId.value = null;
  form.value = {
    field: "process_name",
    match_type: "contains",
    pattern: "",
    category_id: "other",
    priority: 0,
    enabled: true,
  };
}

// 删除规则：先用 Modal 确认
function confirmRemove(id: number, pattern: string) {
  showAlert(
    "confirm",
    t("rules.removeTitle"),
    t("rules.removeMsg", { pattern }),
    async () => {
      try {
        await tracker.deleteRule(id);
        showAlert("info", t("rules.removed"), t("rules.removedMsg", { pattern }));
        await loadAll();
      } catch (e: any) {
        console.error("[Rules] deleteRule failed:", e);
        showAlert("warn", t("rules.removeFailed"), t("rules.removeFailedMsg", { err: e?.message || e }));
      }
    }
  );
}

// 一键按当前规则重算历史分类
async function doReclassify() {
  try {
    const n = await tracker.reclassify();
    showAlert("info", t("rules.reclassifyDone"), t("rules.reclassifyMsg", { n }));
    await loadAll();
  } catch (e: any) {
    console.error("[Rules] reclassify failed:", e);
    showAlert("warn", t("rules.reclassifyFailed"), t("rules.reclassifyFailedMsg", { err: e?.message || e }));
  }
}

/** 通用弹窗的 confirm 事件：优先执行 pendingConfirm，否则单纯关闭 */
function onAlertConfirm() {
  if (pendingConfirm) {
    const cb = pendingConfirm;
    pendingConfirm = null;
    void cb();
  }
}

onMounted(loadAll);
</script>

<style scoped>
.rules-view {
  display: flex;
  flex-direction: column;
  gap: 16px;
}
.card {
  background: var(--card, #fff);
  border: 1px solid var(--border, #e5e7eb);
  border-radius: 12px;
  padding: 16px;
}
.card h3 {
  margin: 0 0 12px;
  font-size: 15px;
}
.head {
  display: flex;
  align-items: center;
  justify-content: space-between;
}
.hint {
  color: var(--text-dim, #6b7280);
  font-size: 12px;
  margin: 0 0 12px;
  line-height: 1.6;
}
/* 规则表格 */
.rule-table {
  width: 100%;
  border-collapse: collapse;
  font-size: 13px;
}
.rule-table th,
.rule-table td {
  border-bottom: 1px solid var(--border, #e5e7eb);
  padding: 8px 10px;
  text-align: left;
}
/* v0.6.2-beta.16：可排序列头 */
.rule-table th.sortable {
  cursor: pointer;
  user-select: none;
}
.rule-table th.sortable:hover {
  background: rgba(255, 126, 39, 0.08); /* 机械师橙淡底 */
}
.sort-ind {
  font-size: 10px;
  color: var(--brand, #FF7E27);
  margin-left: 4px;
}
.rule-table .pattern {
  font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
}
.rule-table .ops button {
  margin-right: 6px;
  font-size: 12px;
  cursor: pointer;
}
.cat-tag {
  display: inline-block;
  padding: 2px 8px;
  border-radius: 999px;
  color: #fff;
  font-size: 12px;
}
.empty {
  text-align: center;
  color: var(--text-dim, #6b7280);
}
/* 头部操作区 */
.head-actions {
  display: flex;
  gap: 8px;
}
/* 弹窗内表单（弹窗化布局） */
.form-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 14px;
}
.form-grid label {
  display: flex;
  flex-direction: column;
  font-size: 12px;
  color: var(--text-dim, #6b7280);
  gap: 4px;
}
.form-grid label.grow {
  grid-column: 1 / -1;
}
.form-grid label.check {
  flex-direction: row;
  align-items: center;
  gap: 6px;
  grid-column: 1 / -1;
}
.form-grid input,
.form-grid select {
  padding: 6px 8px;
  border: 1px solid var(--border, #e5e7eb);
  border-radius: 8px;
  font-size: 13px;
  background: var(--card, #fff);
  color: var(--text, #1f2937);
}
button {
  border: 1px solid var(--border, #e5e7eb);
  background: var(--card, #fff);
  padding: 6px 14px;
  border-radius: 8px;
  font-size: 13px;
  cursor: pointer;
}
button.primary {
  background: var(--brand, #FF7E27);
  color: #fff;
  border-color: transparent;
}
button.danger {
  color: #c0392b;
}
</style>
