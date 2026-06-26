# 身份统一为 Gmail + 账号级 SOCKS5 覆盖 — 设计文档

日期：2026-06-26
状态：待用户复核
分支：fix/ai-settings-and-docs

## 背景与目标

当前身份(persona)系统分三类，由 `personas.ip_mode`(airport/fixed) + `region`(cn/海外) 区分：

- **Gmail 身份**(airport)：每个身份绑一个机场节点，经 mihomo 本地 listener(`127.0.0.1:30000+`)出网。
- **国内固定IP**(fixed+cn) / **国外固定IP**(fixed+海外)：用户手填固定代理 `fixed_proxy`。

两类都通过 `unzoo_set_profile_proxy2(profile_path, "socks5://...")` 把代理绑到浏览器 profile。账号挂在身份下（`accounts.persona_id`），共用身份的 profile 与出口 IP。`platform_ip_policy` 把平台分流到三类身份，前端「加账号」据此筛选可加平台。

**目标**：把「自定义固定代理」能力从**身份级**下放到**账号级**。身份统一成 Gmail(机场轮换)；单个账号可通过卡片上的「🧦 SOCKS5」按钮覆盖出口——没配置走身份统一的机场节点，配置了走自定义 SOCKS5。

## 已确认决策

1. **旧固定身份数据**：全部删除（删 profile + 解除账号关联，账号本身保留为「未归属」+ 删 persona）。
2. **加账号平台范围**：放开，Gmail 身份可加所有平台（废弃 `ip_policy` 分流；登录方式 google/phone/password 区分不变）。
3. **SOCKS5 生效方式**：启动账号 profile 前动态切代理（账号仍共用身份 profile，不为配代理的账号单建 profile）。
4. **接入点**：养号(含一键养号)、登录预检、发帖/任务引擎**全部接入**。
5. **账号卡片按钮**：所有账号都显示「🧦 SOCKS5」；配置了则在卡片上展示当前代理值。
6. **批量**：支持对多个账号批量配置/清除 SOCKS5。

## 架构总览

```
账号操作入口(养号/预检/发帖)
  └─ unzoo_launch_profile(账号的 persona.profile_id)
     └─ apply_account_proxy(account_id)   ← 新增：动态决定 profile 出口
          ├─ 账号有 custom_proxy → set_profile_proxy(profile, custom_proxy)
          └─ 否则 → set_profile_proxy(profile, "socks5://127.0.0.1:{persona.local_port}")  // 机场端口
     └─ 跑 runner / 操作

身份分类(前端) gmail + __none__  （删 fixed_cn / fixed_overseas）
账号卡片 🧦 SOCKS5 按钮 → set_account_proxy / 批量 set_accounts_proxy → accounts.custom_proxy
```

## A. 移除「固定IP」身份分类

**前端 (app.ts)**
- `IdentityCategory` 由 `'gmail' | 'fixed_cn' | 'fixed_overseas' | '__none__'` 改为 `'gmail' | '__none__'`；`ID_CATEGORIES` 删 fixed_cn/fixed_overseas 两项。
- 删分类 tab 渲染里 fixed 分支、删「+ 新建国内/国外固定身份」按钮与 `createFixedPersonaPrompt`、删新建类型选择器(app.ts:2831 区)的 cn/overseas 分支与 `region==='cn'?'fixed_cn':'fixed_overseas'` 归类。
- 清理 i18n 文案：`idcat.fixedCn`、`idcat.fixedOverseas`、`accounts.newFixedCn`、`accounts.ipFixedCn`、`persona.newFixedCn`、`persona.newFixedCnDesc`、`idcat.emptyFixedCn`、`idcat.emptyFixedOverseas` 等键；引导语(app.ts:1292)与场景卡(app.ts:1281「国内种草/生活 需国内固定IP」)改为不再提固定IP身份，引导改为「需要特殊 IP 的账号在卡片上配 SOCKS5」。

**后端 (lib.rs / multi_account.rs)**
- 从 `invoke_handler` 摘除 `persona_create_fixed` 注册（无 UI 入口）。函数体保留为死代码（标注 deprecated）以缩小改动面，不强求删除。

## B. 删除已有固定身份数据（启动一次性迁移）

- 在 DB 迁移区(lib.rs migrations)新增一次性迁移，受 `config` flag 守护（key 如 `migrated_drop_fixed_personas`，已执行则跳过）：
  1. 查所有 `SELECT id, profile_id FROM personas WHERE ip_mode='fixed'`。
  2. 逐个：复用 `persona_delete` 现有的删 unzoo profile 调用（尽力而为，失败仅记日志）；`UPDATE accounts SET persona_id=NULL WHERE persona_id=?`；`DELETE FROM personas WHERE id=?`。最简做法可直接对每个 fixed persona id 调用 `persona_delete` 的核心逻辑。
  3. 写 `config` flag。
- 固定身份不占机场节点(`node_name=NULL`)，无需释放 nodes。
- 账号本身保留（变未归属），不删账号行。

## C. 「加账号」放开平台范围

- 前端 `personaAddAccounts` 的 candidates 过滤(app.ts:2957 区)：去掉 `c.ip_policy === (region==='cn'?'residential_cn':'static_overseas')` 这条 fixed 分支；Gmail 身份分支由 `ip_policy === 'shared_overseas' && login_method !== 'google'` 改为**仅** `login_method !== 'google' && !provisioned`（即所有非 Google 登录、未开通平台都可手动加；Google 登录平台仍走一键开通）。
- `platform_ip_policy`(lib.rs:1480) 后端函数保留，catalog 仍返回 `ip_policy` 字段（前端忽略），避免牵动其它消费方。

## D. 账号级 SOCKS5 配置

**DB (accounts 表)**
- 加列 `custom_proxy TEXT`（启动内联迁移，照现有 nurture 列 `ALTER TABLE accounts ADD COLUMN ... ` 的写法与健壮性检查）。
- 存规范化后的字符串：无 `://` 前缀自动补 `socks5://`；仅接受 `socks5:// | http:// | https://`，否则报错。空字符串/NULL = 未配置。

**规范化函数（可单测）**
- `normalize_proxy(input: &str) -> Result<Option<String>, String>`：trim；空→`Ok(None)`；无协议→补 `socks5://`；校验协议前缀；返回 `Ok(Some(规范化))`。

**命令 (lib.rs)**
- `set_account_proxy(account_id: String, proxy: Option<String>) -> Result<(), String>`：`normalize_proxy` 后 `UPDATE accounts SET custom_proxy=?`（None→NULL）。
- `set_accounts_proxy(account_ids: Vec<String>, proxy: Option<String>) -> Result<usize, String>`：批量循环上一步，返回成功条数。
- `test_account_proxy(account_id: String) -> Result<String, String>`：复用 `persona_test_ip` 思路——解析账号 profile_path → `apply_account_proxy` → 启动 profile → 导航 `https://api.ip.sb/geoip` → 返回「出口 IP：x (国家 城市)」。

**前端 (app.ts) — 单账号**
- 账号卡片加按钮「🧦 SOCKS5」（所有账号显示）。已配置 → 按钮高亮 + 卡片上展示当前代理值（如 `🧦 1.2.3.4:18080`）。
- 点开弹框(仿 `uiConfirm` 风格)：输入框预填当前值 + 「测试出口IP」按钮(调 `test_account_proxy`，弹框内显示结果) + 「保存」/「清除」。保存调 `set_account_proxy`，成功后 `loadAccounts()` 重渲染。

**前端 (app.ts) — 批量**
- 仿「批量养号任务」弹框模式：新「批量配置 SOCKS5」弹框，列账号 checkbox 多选 + 一个 SOCKS5 输入框 + 「应用到选中」/「清除选中」。调 `set_accounts_proxy`。

## E. 代理动态生效（核心）

**新 helper (lib.rs，async)**
- `apply_account_proxy(conn, account_id) -> Result<(), String>`：
  1. 查 `accounts.custom_proxy` 与 `persona_id`。
  2. 解析账号 profile_path（经 persona.profile_id → `resolve_profile_path`）。
  3. 有 `custom_proxy` → `unzoo_set_profile_proxy2(path, custom_proxy)`。
  4. 否则若 persona 为机场(有 `local_port`) → `unzoo_set_profile_proxy2(path, "socks5://127.0.0.1:{local_port}")`。
  5. 否则(未归属且无自定义)→ 不动，记日志（无 IP 来源）。

**接入点**（均在 `unzoo_launch_profile` 之后、实际操作之前 await 调用）
- `quick_nurture`(lib.rs:9547 `set_active_tab` 后) —— 覆盖单账号养号与一键养号。
- `check_account_login`(本分支新增的登录预检命令) —— launch 之后。
- 发帖/任务引擎：走 `resolve_account_profile` 启动 profile 的位置（lib.rs 任务执行处）——在启动后、动作前调用。

**约束**：profile 代理是即时覆盖，依赖同一 profile 串行操作。一键养号已保证同 profile 不并发；任务引擎同 profile 任务亦串行。文档化此约束。

## 错误处理

- `apply_account_proxy` 设代理失败 → 记日志，继续（退回 profile 当前代理；不阻断操作）。
- `normalize_proxy` 非法格式 → 命令返回 Err，前端 toast 提示。
- 迁移中删 profile 失败 → 记日志，仍解除关联 + 删 persona（避免迁移卡死）。
- `test_account_proxy` 启动/导航失败 → 返回 Err 文案，前端弹框显示。

## 测试

- **单元**：`normalize_proxy`（`host:port`→`socks5://`、补全、协议校验、空→None）。固定身份迁移 SQL（建 fixed persona+账号 → 迁移后 persona 删除、账号 `persona_id` 为 NULL 且账号仍在）。
- **手动**（需登录环境）：账号配 SOCKS5 → `test_account_proxy` 出口 IP 为代理 IP；清除 → 走回机场端口 IP；一键养号/发帖时出口随账号 custom_proxy 切换；批量设置生效。

## 非目标

- 账号级机场节点选择（仍共用身份机场节点）。
- SOCKS5 自动测速 / 轮换 / 健康检查。
- 改动平台开通登录方式(google/phone/password)。
- 删除 `persona_create_fixed` 后端函数体（仅摘 UI 入口）。
