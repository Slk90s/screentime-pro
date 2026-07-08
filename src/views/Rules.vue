<template>
  <!-- 分类规则管理视图
       负责：展示/新增/编辑/删除「自动归类规则」，并一键按规则重算历史数据。
       注：开机自启开关已移至「设置」页（Settings.vue），此处不再重复。 -->
  <div class="rules-view">
    <!-- ============ 规则列表 ============ -->
    <section class="card">
      <div class="head">
        <h3>分类规则（自动归类引擎）</h3>
        <button class="primary" @click="doReclassify">按规则重算历史</button>
      </div>
      <p class="hint">
        采集到的应用会按「字段 + 匹配方式 + 匹配值」自动归入分类，无需导出后人工整理。
        优先级大的规则先匹配；窗口标题规则需 Windows 默认可取 / macOS 授予「屏幕录制」权限。
      </p>

      <table class="rule-table">
        <thead>
          <tr>
            <th>字段</th>
            <th>匹配方式</th>
            <th>匹配值</th>
            <th>归入分类</th>
            <th>优先级</th>
            <th>启用</th>
            <th>操作</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="r in rules" :key="r.id">
            <td>{{ fieldLabel(r.field) }}</td>
            <td>{{ matchLabel(r.match_type) }}</td>
            <td class="pattern">{{ r.pattern }}</td>
            <td>
              <span class="cat-tag" :style="{ background: catColor(r.category_id) }">
                {{ catName(r.category_id) }}
              </span>
            </td>
            <td>{{ r.priority }}</td>
            <td>{{ r.enabled ? "是" : "否" }}</td>
            <td class="ops">
              <button @click="editRule(r)">编辑</button>
              <button class="danger" @click="removeRule(r.id)">删除</button>
            </td>
          </tr>
          <tr v-if="rules.length === 0">
            <td colspan="7" class="empty">暂无规则</td>
          </tr>
        </tbody>
      </table>
    </section>

    <!-- ============ 区块三：新增 / 编辑表单 ============ -->
    <section class="card">
      <h3>{{ editingId === null ? "新增规则" : "编辑规则 #" + editingId }}</h3>
      <div class="form">
        <label>
          字段
          <select v-model="form.field">
            <option value="process_name">进程名</option>
            <option value="window_title">窗口标题</option>
            <option value="exe_path">可执行路径</option>
            <option value="bundle_id">包名 (Bundle ID)</option>
            <option value="name">展示名</option>
          </select>
        </label>
        <label>
          匹配方式
          <select v-model="form.match_type">
            <option value="contains">包含</option>
            <option value="equals">相等</option>
            <option value="prefix">前缀</option>
            <option value="suffix">后缀</option>
            <option value="regex">正则</option>
          </select>
        </label>
        <label class="grow">
          匹配值
          <input v-model="form.pattern" placeholder="如 wechat / netflix（忽略大小写）" />
        </label>
        <label>
          归入分类
          <select v-model="form.category_id">
            <option v-for="c in categories" :key="c.id" :value="c.id">{{ c.name }}</option>
          </select>
        </label>
        <label>
          优先级
          <input type="number" v-model.number="form.priority" />
        </label>
        <label class="check">
          <input type="checkbox" v-model="form.enabled" /> 启用
        </label>
      </div>
      <div class="form-actions">
        <button class="primary" @click="submit">{{ editingId === null ? "新增" : "保存修改" }}</button>
        <button v-if="editingId !== null" @click="resetForm">取消</button>
      </div>
    </section>
  </div>
</template>

<script setup lang="ts">
// 分类规则管理界面
// 功能：
// 1. 规则增删改查（get_rules / add_rule / update_rule / delete_rule）
// 2. 一键按规则重算历史分类（reclassify_all）
// 注：开机自启开关已移至 Settings.vue，本页只管分类规则。
// 所有代码均带中文注释，符合项目「新增界面必须加注释」的规范。

import { ref, onMounted } from "vue";
import { tracker } from "../api/tracker";
import type { CategoryOut, RuleOut } from "../types";

// 规则列表与分类字典
const rules = ref<RuleOut[]>([]);
const categories = ref<CategoryOut[]>([]);

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

// 字段中文名
function fieldLabel(f: string): string {
  return (
    {
      process_name: "进程名",
      window_title: "窗口标题",
      exe_path: "可执行路径",
      bundle_id: "包名",
      name: "展示名",
    }[f] || f
  );
}
// 匹配方式中文名
function matchLabel(m: string): string {
  return (
    {
      contains: "包含",
      equals: "相等",
      prefix: "前缀",
      suffix: "后缀",
      regex: "正则",
    }[m] || m
  );
}
// 分类名（按 id 查字典；查不到显示 id）
function catName(id: string): string {
  return categories.value.find((c) => c.id === id)?.name || id;
}
// 分类颜色（用于标签背景）
function catColor(id: string): string {
  return categories.value.find((c) => c.id === id)?.color || "#888780";
}

// 提交：新增或保存修改
async function submit() {
  const f = form.value;
  if (!f.pattern.trim()) {
    alert("匹配值不能为空");
    return;
  }
  if (editingId.value === null) {
    await tracker.addRule({
      field: f.field,
      match_type: f.match_type,
      pattern: f.pattern.trim(),
      category_id: f.category_id,
      priority: f.priority,
    });
  } else {
    await tracker.updateRule({
      id: editingId.value,
      field: f.field,
      match_type: f.match_type,
      pattern: f.pattern.trim(),
      category_id: f.category_id,
      priority: f.priority,
      enabled: f.enabled,
    });
  }
  await loadAll();
  resetForm();
}

// 载入某条规则到表单进行编辑
function editRule(r: RuleOut) {
  editingId.value = r.id;
  form.value = {
    field: r.field,
    match_type: r.match_type,
    pattern: r.pattern,
    category_id: r.category_id,
    priority: r.priority,
    enabled: r.enabled,
  };
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

// 删除规则
async function removeRule(id: number) {
  if (!confirm("确定删除该规则？")) return;
  await tracker.deleteRule(id);
  await loadAll();
}

// 一键按当前规则重算历史分类
async function doReclassify() {
  const n = await tracker.reclassify();
  alert(`已更新 ${n} 个应用的分类`);
  await loadAll();
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
/* 表单 */
.form {
  display: flex;
  flex-wrap: wrap;
  gap: 12px;
  align-items: flex-end;
}
.form label {
  display: flex;
  flex-direction: column;
  font-size: 12px;
  color: var(--text-dim, #6b7280);
  gap: 4px;
}
.form label.grow {
  flex: 1;
  min-width: 220px;
}
.form input,
.form select {
  padding: 6px 8px;
  border: 1px solid var(--border, #e5e7eb);
  border-radius: 8px;
  font-size: 13px;
}
.form label.check {
  flex-direction: row;
  align-items: center;
  gap: 6px;
}
.form-actions {
  margin-top: 12px;
  display: flex;
  gap: 8px;
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
