# 小红书养号主题选择 — 设计文档

日期：2026-06-25
状态：已确认，待实现计划

## 背景

小红书账号目前没有专属养号 runner，走通用 `simulate_browsing_blocking`（纯滚动 + 鼠标移动 + hover，不读任何主题/领域）。而 GitHub / X / SegmentFault 三个平台已有成熟的「领域/方向」选择模式：内置一批硬编码的领域常量，账号从中多选，runner 按所选领域驱动养号动作。

本设计给小红书引入一套**主题选择系统**，并在现有模式之上增加现有三套都没有的能力：**用户自定义添加主题**。

## 范围

- **只做小红书**。不改 GitHub/X/SegmentFault。
- **只做主题选择层**：内置主题 + 用户自定义添加 + 勾选 + 持久化存储。
- **不做 runner**。本次养号实际行为不变（小红书仍走通用滚动）；主题为将来的小红书专属 runner 预留。届时 runner 直接拿所选主题名当小红书站内搜索词。

## 设计决策（已确认）

1. 主题 = 一个中文名称（如「美妆护肤」「露营装备」），既是显示名、将来也直接当搜索词。不让用户填关键词列表。
2. 账号**多选**勾选（与 GitHub/X/SegmentFault 一致）。
3. 自定义主题进**全局小红书主题库**，所有小红书账号共享，不是账号私有。
4. 存储用**方案 A：内置常量 + 自定义表**。内置主题是代码事实源（随版本更新、用户删不掉），自定义主题叠加在表里。

## 数据存储

```
内置主题    →  Rust 常量 XHS_TOPICS = &[XhsTopic{ key, label }, ...]
自定义主题  →  新表 xhs_custom_topics(key TEXT PRIMARY KEY, label TEXT NOT NULL)
账号已选    →  accounts.xhs_topics  TEXT 列，存 JSON 数组 of keys
```

- 内置主题 `key` 用语义短词（`beauty` / `food` …）；自定义主题 `key` 用 `Uuid::new_v4().to_string()`。
- `accounts.xhs_topics` 列通过内联 `ALTER TABLE accounts ADD COLUMN xhs_topics TEXT` 迁移，与现有 `x_niches` 的迁移方式一致。
- catalog 合并内置与自定义，每项带 `builtin: bool` 标记，供前端决定能否删除。

### 内置主题清单（12 个）

| key | label |
|---|---|
| beauty | 美妆护肤 |
| fashion | 穿搭时尚 |
| food | 美食探店 |
| travel | 旅行出行 |
| home | 家居家装 |
| parenting | 母婴育儿 |
| fitness | 健身运动 |
| digital | 数码科技 |
| career | 职场成长 |
| emotion | 情感生活 |
| pet | 萌宠 |
| wellness | 养生健康 |

## 后端命令（5 个，注册到 invoke_handler）

| 命令 | 签名 | 行为 |
|---|---|---|
| `xhs_topics_catalog` | `() -> Vec<XhsTopicItem{key,label,builtin}>` | 返回 内置 ⋃ 自定义，内置在前 |
| `get_account_xhs_topics` | `(account_id) -> Vec<String>` | 读账号已选 keys（解析 JSON 列） |
| `set_account_xhs_topics` | `(account_id, keys: Vec<String>)` | 存账号已选；写入前过滤掉 catalog 中不存在的 key（容错） |
| `add_xhs_custom_topic` | `(label) -> XhsTopicItem` | label trim 后非空校验；与现有（内置+自定义）label 重名则拒绝并提示；生成 uuid key 入表，返回新项 |
| `delete_xhs_custom_topic` | `(key)` | 内置 key 拒绝删除；删表行，并从所有账号 `xhs_topics` 已选里移除该 key（防悬空引用） |

复用现有 helper 思路：参考 `account_x_niches` / `set_account_x_niches` / `x_niches_catalog`（lib.rs:1204/1257/1274）的写法。

## 前端

- 新函数 `pickXhsTopics(accountId)`：弹 modal，复用现有 `toggleNicheRow` 勾选行样式（参考 `pickXNiches`，app.ts:1603）。
  - 列表渲染 catalog，已选项预勾。
  - 自定义项右侧带删除 ✕（调 `delete_xhs_custom_topic` 后刷新列表）。
  - modal 底部多一个「+ 添加主题」输入框 + 按钮：输入名称 → `add_xhs_custom_topic` → 刷新列表并自动勾上。
  - 保存调 `set_account_xhs_topics`。
- 账号卡片入口：在 app.ts:3137（SegmentFault 入口）旁加一行，小红书显示 `🎯 主题` 按钮：
  ```ts
  account.platform === 'xiaohongshu'
    ? `<button class="btn btn-small btn-secondary" onclick="pickXhsTopics('${account.id}')" title="选择小红书养号主题">🎯 主题</button>`
    : ''
  ```
- 已选主题在卡片上仍用现有 `🎯 chip`（app.ts:3113）展示，label 取自 catalog。

## 错误处理

- `add_xhs_custom_topic`：label trim 后为空 → 返回错误；与现有 label 重名 → 返回「主题已存在」错误，不重复加。
- `delete_xhs_custom_topic`：传入内置 key → 返回错误（内置不可删）。
- `set_account_xhs_topics`：keys 中不在 catalog 的项静默过滤，不报错。
- 前端 modal 各 invoke 失败 → `showToast(..., 'error')`，与现有 pick 函数一致。

## 测试

后端单元测试（仿 lib.rs:12754 `read_account_niches`，用内存 SQLite）：
- `xhs_topics_catalog` 合并内置 + 自定义、内置在前、无重复。
- `set` 后 `get` 往返一致；`set` 过滤未知 key。
- `add` 去空格 + 重名拒绝；`add` 后出现在 catalog。
- `delete` 自定义后从 catalog 消失，且已勾选该 key 的账号 `xhs_topics` 里也被移除；`delete` 内置被拒。

## 非目标（明确不做）

- 小红书专属养号 runner（拿主题搜索→浏览→读笔记）——后续独立任务。
- 把主题系统推广到 GitHub/X/SegmentFault 或做成跨平台统一系统。
- 主题带搜索关键词列表、主题分组/层级、主题级配额等。
