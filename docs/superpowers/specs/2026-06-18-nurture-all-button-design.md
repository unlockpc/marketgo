# 一键养号按钮设计

日期：2026-06-18
分支：fix/ai-settings-and-docs

## 背景

账号管理页（`dist/tauri/index.html` 的 `accounts` 页，页头标题 `👤 身份管理`，第 803-808 行）目前右上角只有一个隐藏的「添加账号」按钮。现有的养号入口有三处：

- **单账号**：每张账号卡片上的 `🌱 快速养号` 按钮 → `openNurtureModal` → `quick_nurture` 命令；
- **批量**：Tasks 页的 `🌱 新建养号任务` → `enqueue_nurture` 队列；
- **自动**：`🟢 一键全自动` 引擎按平台策略自动排期养号。

需求：在账号管理页右上角加一个「一键养号」按钮，点一下就立即把所有账号养一遍。

## 目标

点击页头的 `🌱 一键养号` 按钮后，复用现有 `quick_nurture` 后端命令，对所有符合条件的账号逐个（串行）跑一轮养号。

## 行为规格

1. **点击** 页头 `🌱 一键养号` 按钮 → 弹出 `modalNurtureAll` 弹框。
2. **选时长**：弹框内提供时长下拉，复用单账号弹框的选项（30 秒 / 1 / 2 / 5 / 10 分钟），默认 1 分钟（60 秒）。
3. **生成养号清单**：取 `accounts` 全量，**跳过今日养号次数 ≥ 2 的账号**。今日次数来源：`accountLifecycles.get(account.id)?.today?.sessions_completed`（已在 `loadAccounts()` → `loadAccountLifecycles()` 时加载并缓存）。
   - 若清单为空 → 弹框内提示 `没有需要养号的账号（今日均已养 ≥2 次）`，不进入运行态。
4. **串行执行**：对清单中每个账号依次 `await invoke('quick_nurture', { accountId, seconds })`。后端每个账号要开真实浏览器，因此必须串行，不并发。
5. **实时进度**：弹框显示 `正在养号 3/8：<账号名>`，并提供 **停止** 按钮。停止语义为「养完当前账号后停止」，不中断正在进行的那一个。
6. **单账号失败**：记录为失败并继续下一个，不中断整轮。
7. **汇总**：全部跑完（或被停止）后显示 `✅ 成功 X · ⏭ 跳过 Y · ❌ 失败 Z`，并刷新账号列表（`await loadAccounts()`）。
8. **防重复触发**：运行期间按钮置灰；若已有单账号养号在跑（`nurtureInProgress` 非空），点击时提示「有养号任务正在进行」，不开新轮。

## 实现要点

### HTML（`dist/tauri/index.html`）

- 在 accounts 页头（第 803-808 行）`<h2>` 之后加按钮：
  ```html
  <button class="btn btn-success" id="btnNurtureAll">🌱 一键养号</button>
  ```
- 新增独立弹框 `modalNurtureAll`，结构参照现有 `modalNurture`，包含三段（用 display 切换）：
  - `nurtureAllSetup`：时长下拉 `nurtureAllDuration`（选项同 `nurtureDuration`）+ 开始按钮。
  - `nurtureAllProgress`：进度文案 + 进度条 + 停止按钮 `btnNurtureAllStop`。
  - `nurtureAllComplete`：汇总文案 + 关闭按钮 `btnNurtureAllClose`。

### 逻辑（`src/tauri-frontend/app.ts`，`tsc` 编译至 `dist/tauri/scripts/app.js`）

- 在事件绑定块（~第 1338 行 `btnAddAccount` 附近）加：
  ```ts
  document.getElementById('btnNurtureAll')?.addEventListener('click', openNurtureAllModal);
  ```
- 新增：
  - `openNurtureAllModal()`：检查 `nurtureInProgress`；计算清单与跳过数；打开弹框、展示 setup 段。
  - `startNurtureAll()`：读取时长；切到 progress 段；串行循环调用 `quick_nurture`，每个账号更新进度文案与计数；尊重停止标志 `nurtureAllAborted`；结束后切到 complete 段并 `loadAccounts()`。
  - 停止/关闭按钮的处理（设置 `nurtureAllAborted = true` / 关闭弹框）。
- 新增模块级状态：`let nurtureAllAborted = false;`、`let nurtureAllRunning = false;`。
- 复用现有工具：`openModal/closeModal`、`escapeHtml`、`t()`（i18n）、`accountLifecycles`、`accounts`、`loadAccounts`。

## 边界与约束

- 后端 `quick_nurture` 已负责更新 `last_nurture_at`、`total_nurture_seconds`、每日日志，前端不重复写库。
- 跳过判定只读前端缓存的 lifecycle 数据；缓存缺失（lifecycle 为 undefined）时视为今日 0 次、不跳过。
- 不改动单账号 `modalNurture` 及其倒计时逻辑，两套流程隔离。
- 不引入新的后端命令；纯前端编排已有 `quick_nurture`。

## 不做（YAGNI）

- 不做并发养号、不做养号顺序自定义、不做单账号时长差异化（本轮统一时长）。
- 不持久化「一键养号」为任务队列（与 `enqueue_nurture` 区分；本功能是即时跑一轮）。
- 不改跳过阈值的可配置化（固定 ≥2 次跳过）。
