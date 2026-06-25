# 养号主题统一系统（平台隔离）— 设计文档

日期：2026-06-25
状态：已确认方向，待用户复核
取代：`2026-06-25-xhs-nurture-topics-design.md`（小红书专属版，作废）

## 背景与目标

当前每个平台的"养号方向/领域"各搞一套、且只能选不能加：
- GitHub：常量 `GH_DOMAINS`（key/label/**topics**）+ 列 `accounts.gh_domains`
- X：常量 `X_NICHES`（key/label/**keywords**）+ 列 `accounts.x_niches`
- SegmentFault：常量 `SF_DOMAINS`（key/label/**keywords**）+ 列 `accounts.sf_domains`
- 小红书：无（通用滚动）

目标：做一个**平台隔离、可扩展**的统一主题系统——
1. 任一平台一套主题（内置 + 用户自定义），平台之间互不混。
2. 用户可现场**添加/删除自定义主题**（现有三套都没有的新能力）。
3. **三套现有平台一并迁入**统一系统；小红书作为新接入者。
4. 以后任何平台想用主题，只需在 `builtin_topics()` 加一组、卡片放个入口，零新表零新命令。

## 关键决策（已与用户确认）

- 平台隔离：主题按 platform 分组。
- 现有 GitHub/X/SegmentFault **现在就迁入**统一系统（非保留旧结构）。
- 保留 runner 需要的 **keywords**（GitHub 的 topics 归一为 keywords）。
- 主题 = key + label + keywords[]；小红书主题的 keywords 默认就是其 label（主题名即搜索词）。

## 架构总览

**保留四个常量当内置数据源**（key 不变 → 数据零丢失、无缝迁移），在其上加统一层：

```
内置        builtin_topics(platform) -> Vec<TopicDef{key,label,keywords}>
              ├─ github        ← GH_DOMAINS（topics 当 keywords）
              ├─ twitter/x     ← X_NICHES（keywords）
              ├─ segmentfault  ← SF_DOMAINS（keywords）
              └─ xiaohongshu   ← XHS_TOPICS（keywords = [label]）
自定义      表 custom_topics(key PK, platform, label, keywords TEXT/JSON 可空)
账号已选    通用列 accounts.nurture_topics（JSON keys；账号自带 platform，一账号一平台，无需 per-平台列）
catalog     topics_catalog(platform) = builtin(platform) ⋃ custom WHERE platform
runner 关键词 account_topic_keywords(conn, account_id) = 按账号 platform 收集所选 key 的 keywords（去重）
```

> key 跨平台可重名（如 github 与 segmentfault 都有 `frontend`），互不影响：账号的 platform 决定 key 落在哪套 catalog。

## 数据存储与迁移

```sql
ALTER TABLE accounts ADD COLUMN nurture_topics TEXT;
CREATE TABLE IF NOT EXISTS custom_topics (
  key      TEXT PRIMARY KEY,
  platform TEXT NOT NULL,
  label    TEXT NOT NULL,
  keywords TEXT            -- JSON 数组，可空；空则 runner 用 label 当关键词
);
```

一次性数据迁移（把旧三列搬进统一列，key 不变所以直接搬 JSON）：

```sql
UPDATE accounts SET nurture_topics = gh_domains WHERE platform='github'        AND nurture_topics IS NULL AND gh_domains IS NOT NULL;
UPDATE accounts SET nurture_topics = x_niches   WHERE platform IN('twitter','x') AND nurture_topics IS NULL AND x_niches   IS NOT NULL;
UPDATE accounts SET nurture_topics = sf_domains WHERE platform='segmentfault'  AND nurture_topics IS NULL AND sf_domains IS NOT NULL;
```

旧列 `gh_domains/x_niches/sf_domains` **保留不删**（便于回滚），但代码不再读写。

### 小红书内置主题（12 个，keywords = label）

美妆护肤 · 穿搭时尚 · 美食探店 · 旅行出行 · 家居家装 · 母婴育儿 · 健身运动 · 数码科技 · 职场成长 · 情感生活 · 萌宠 · 养生健康

## 后端

### 类型

```rust
pub struct TopicDef  { pub key: String, pub label: String, pub keywords: Vec<String> }
#[derive(serde::Serialize, Clone)]
pub struct TopicItem { pub key: String, pub label: String, pub keywords: Vec<String>, pub builtin: bool }
```

### 核心 helper（conn-level，可测）

- `builtin_topics(platform) -> Vec<TopicDef>`：四常量按平台映射；未知平台返回空。
- `account_topics(conn, account_id) -> Vec<String>`：读 `nurture_topics` JSON keys。
- `topics_catalog_from(conn, platform) -> Vec<TopicItem>`：内置(builtin=true) ⋃ custom 表 WHERE platform(builtin=false)。
- `set_account_topics_conn(conn, account_id, keys)`：按账号 platform 的 catalog 过滤非法 key 后写入。
- `add_custom_topic_conn(conn, platform, label) -> Result<TopicItem>`：label trim 非空 + 与该 platform 现有(内置+自定义)label 不重名；uuid key 入表。
- `delete_custom_topic_conn(conn, key)`：内置 key 拒绝；删表行 + 从所有账号 `nurture_topics` 移除该 key。
- `account_topic_keywords(conn, account_id) -> Vec<String>`：按账号 platform + 所选 keys，从 catalog 收集 keywords 去重；自定义无 keywords 时回退用 label。**供三个 runner 调用**。

### 命令（6 个，注册到 invoke_handler）

| 命令 | 签名 |
|---|---|
| `topics_catalog` | `(platform) -> Vec<TopicItem>` |
| `get_account_topics` | `(account_id) -> Vec<String>` |
| `set_account_topics` | `(account_id, keys: Vec<String>)` |
| `add_custom_topic` | `(platform, label) -> TopicItem` |
| `delete_custom_topic` | `(key)` |
| `account_topic_labels` | `() -> HashMap<account_id, Vec<String>>`（后端按 platform 解析 key→label，供卡片 chip 直接显示）|

### runner 改造（nurture.rs，3 处，逻辑等价替换）

| runner | 原读取 | 原关键词 | 改为 |
|---|---|---|---|
| github（28-47） | `account_gh_domains` | `gh_domain_topics(&dom_keys)` | `account_topics` + `account_topic_keywords` |
| segmentfault（272-285） | `account_sf_domains` | `sf_domain_keywords(&keys)` | 同上 |
| x（328-344） | `account_x_niches` | `x_niche_keywords(&keys)` | 同上 |

空检查（`未选→跳过`）保持不变（基于 `account_topics(...).is_empty()`）。关键词收集顺序与原先一致（常量顺序 + 按所选 key 收集去重）。

### 删除的旧代码（统一后冗余）

- 命令：`gh_domains_catalog`/`get|set_account_gh_domains`、`x_niches_catalog`/`get|set_account_x_niches`、`sf_domains_catalog`/`get|set_account_sf_domains`、`account_niches`（从 invoke_handler + 定义一并移除）。
- helper：`account_gh_domains`/`account_x_niches`/`account_sf_domains`、`gh_domain_topics`/`x_niche_keywords`/`sf_domain_keywords`。
- 对应旧测试（`read_account_domains`/`read_account_niches`/`gh_domain_topics_collects_and_dedups`/`x_niche_keywords_collects_and_dedups`）删除或改写为新统一测试。
- 保留：`GhDomain/XNiche/SfDomain` 结构 + 四个常量（仍是内置数据源）。

## 前端

- 三函数 `pickGithubDomains`/`pickXNiches`/`pickSegmentfaultDomains` 合并为一个 `pickTopics(accountId, platform)`：
  - catalog 调 `topics_catalog(platform)`；已选调 `get_account_topics`；保存调 `set_account_topics`。
  - modal 底部「+ 添加主题」→ `add_custom_topic(platform, label)`；自定义项带删除 ✕ → `delete_custom_topic(key)`。
  - 复用现有 `.modal.active` + `toggleNicheRow` 样式。
- 账号卡片入口（app.ts:3135-3137）：github/twitter/x/segmentfault/xiaohongshu 统一显示 `🎯 主题` 按钮，`onclick="pickTopics(id, platform)"`，替换原三个分支。
- chip 展示：改用 `account_topic_labels()` 直接拿 label 列表渲染，移除前端三个 label map（`ghDomainLabels/xNicheLabels/sfDomainLabels`）+ `account_niches` 调用。

## 错误处理

- `add_custom_topic`：label trim 空 → 错误；同 platform 重名（含内置）→「主题已存在」错误。
- `delete_custom_topic`：内置 key → 错误（不可删）。
- `set_account_topics`：非该平台 catalog 的 key 静默过滤。
- 前端各 invoke 失败 → `showToast(..., 'error')`。

## 测试

后端新增 `mod topics_tests`（内存 SQLite，建 accounts + custom_topics 表）：
- `builtin_topics` 各平台非空、字段映射正确（github topics→keywords、xhs keywords=label）。
- `topics_catalog_from` 平台隔离：custom 只在对应 platform 出现；内置在前。
- `account_topics` set→get 往返；set 过滤非法 key。
- `add_custom_topic`：trim + 同平台重名拒绝；不同平台同名允许。
- `delete_custom_topic`：自定义删除从 catalog + 账号已选移除；内置拒绝。
- `account_topic_keywords`：内置 key 收集 keywords + 去重；自定义无 keywords 回退 label。

前端：esbuild 打包 + `tsc --noEmit` + 应用内手动验证（GitHub/X/SF/小红书账号卡片均能开 modal、增删主题、保存后 chip 正确）。

## 非目标

- 小红书专属养号 runner（拿主题搜索→浏览→读笔记）——后续独立任务；本次小红书仍走通用滚动，但主题已可选、数据已就绪。
- 自定义主题的 keywords 编辑 UI（本次自定义主题不填 keywords，runner 回退用 label）。
- 主题分组/层级、主题级配额。
