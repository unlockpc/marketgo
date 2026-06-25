# 小红书专属养号 runner — 设计文档

日期：2026-06-25
状态：已确认，待用户复核
依赖：`2026-06-25-nurture-topics-unified-design.md`（统一主题系统，已实现）——本 runner 直接消费 `account_topic_keywords`。

## 背景与目标

小红书目前没有专属养号 runner，走通用 `simulate_browsing_blocking`（纯滚动，不读主题）。现在主题系统已就绪（`account_topic_keywords` 能返回账号所选主题的关键词，小红书主题的关键词即主题名）。

目标：给小红书做一个**搜索驱动**的专属 runner，照搬 SegmentFault runner 的"搜索→拟人浏览→点进阅读"结构，按号龄分期：**预热期全程只读，成长期加极少量点赞**。

## 关键决策（已确认）

1. 动作强度：预热期只读；成长期对少量笔记**点赞**。**v1 只做点赞**，收藏/关注/评论/发布后续再说。
2. 所有点击动作前"等页面加载完 + 随机延迟"，不固定 sleep 抢跑。
3. 养号默认**预热 7 天 / 成长 5 天**，沿用现有养号策略 UI 可编辑。

## 架构总览

照搬 `segmentfault_nurture_run` + `sf_nurture_browse_blocking`（nurture.rs:203/265）的结构，借用 X runner 的 `x_actions_log` 去重模式做点赞。

```
quick_nurture 调度（lib.rs）
  └─ platform=xiaohongshu/redbook → nurture::xiaohongshu_nurture_run(...)（在 generic 滚动 fallback 之前）

xiaohongshu_nurture_run（nurture.rs，照搬 segmentfault_nurture_run）
  ├─ account_topics(keys) + account_topic_keywords(kws) + 分期(nurture_strategies platform='xiaohongshu')
  ├─ keys 空 → 跳过提示；kws 空 → 跳过
  ├─ 分期强度表 → (n_search, read_per_search, n_like)
  ├─ spawn_blocking → xhs_nurture_browse_blocking(...)
  └─ 写养号统计（total_nurture_seconds + nurture_daily_logs，与其它 runner 一致）

xhs_nurture_browse_blocking(keywords, n_search, read_per_search, n_like, dur, seed)
  ├─ xhs_logged_in_blocking() 未登录 → 报错提示手工登录
  ├─ 循环搜索：xorshift 选词 → 导航 search_result?keyword= → 等加载 → 拟人滚动 → 采笔记链接去重
  ├─ 点进 read_per_search 篇：导航 → 等加载 → 滚动阅读
  └─ 成长期：配额内点赞已读笔记（xhs_like_blocking + xhs_actions_log 去重）
```

## 分期强度

| 阶段 | 搜索次数 n_search | 每次阅读 read_per_search | 点赞 n_like |
|---|---|---|---|
| 预热 warmup | 2 | 2 | 0 |
| 成长 growth | 3 | 3 | 2 |
| 成熟 mature | 2 | 2 | 1 |

分期由 `nurture_phase_and_target(age, warmup, growth, smin, smax)` 决定（与其它 runner 同）。

## 时间控制（所有点击动作）

不用固定 sleep 抢在加载前操作。每个动作前：

- **打开页面**（搜索页 / 笔记页）：`unzoo_navigate` 后**轮询关键元素出现**确认加载完（照搬 `sf_logged_in_blocking` 的 `unzoo_element_exists` 轮询，最多等若干秒）→ 再叠加 `get_human_delay` 随机停顿 → 才滚动/采集。
- **点赞**：进笔记页 → 等加载完 → 拟人滚动阅读 → **再随机停 2~5 秒** → 点击点赞按钮 → 点击后短暂 settle 再进行下一步。

辅助函数 `xhs_wait_loaded_blocking(selector, max_secs)`：轮询 `unzoo_element_exists(selector)`，命中即返回 true，超时返回 false（页面没加载好则跳过该次动作，不报错）。

## 养号策略默认值（预热 7 / 成长 5，可编辑）

- **新库 seed**（lib.rs:3699 区）：小红书 `warmup_days` 14 → **7**；seed 循环后补一句 `UPDATE nurture_strategies SET growth_days=5 WHERE platform IN ('xiaohongshu','redbook')`（seed 元组不含 growth_days）。
- **老库迁移**（迁移块）：`UPDATE nurture_strategies SET warmup_days=7, growth_days=5 WHERE platform='xiaohongshu' AND warmup_days=14 AND growth_days IS NULL`——仅改"仍是自动默认值"的行，**不覆盖用户手改过的**。
- **编辑**：沿用现有「养号策略」UI（`update_nurture_strategy` 已支持 warmup_days/growth_days），无新代码。

## 点赞基础设施（借用 X 的 `x_actions_log` 同构）

- 新表 `xhs_actions_log(id, account_id, action_type, target, date, created_at)`（与 `x_actions_log` 同构）+ 迁移 `CREATE TABLE IF NOT EXISTS`。
- `xhs_already_acted(conn, account_id, target)` / `xhs_record_action(conn, account_id, action_type, target)`（照搬 `x_already_acted`/`x_record_action`）。
- 点赞前 `xhs_already_acted(... note_url)` 去重，避免重复点赞同一篇（跨 session）。

## 小红书站点 DOM（最佳猜测，需登录后实跑微调）

这些是站点专属、最易碎的部分，实现时给最佳猜测，**可能需要登录后实测调整**：

- **搜索 URL**：`https://www.xiaohongshu.com/search_result?keyword={q}`（已确认，lib.rs:1694）。
- **笔记链接选择器**：`a[href*="/explore/"]`（搜索结果卡片指向笔记详情 `/explore/<id>`）。
- **登录指示**（`xhs_logged_in_blocking`）：导航 `https://www.xiaohongshu.com/`，轮询登录后入口出现（如"发布"按钮 / 用户头像 / 创作入口）vs 登录按钮，照搬 `sf_logged_in_blocking` 的轮询结构。
- **点赞按钮选择器**（`xhs_like_blocking`）：笔记页点赞按钮（如 `.like-wrapper` / `[class*="like"]`，实测确认）。

搜索/滚动/阅读是页面级、稳；登录检测、笔记链接、点赞按钮是易碎项，实现时标注并允许微调。

## 错误处理

- 未登录 → 返回 Err（提示手工登录），不退回纯滚动。
- 单次 `unzoo_navigate` 失败 / 等加载超时 → 跳过该次，继续下一次（不整体失败）。
- 点赞按钮找不到 → 跳过该次点赞（不报错、不计数）。
- 关键词为空 / 未选主题 → 跳过并提示。

## 测试

- **分期强度** `xhs_phase_intensity(phase) -> (n_search, read_per_search, n_like)`：纯逻辑 → 单元测试（warmup 不点赞、growth 点赞、mature 维持）。
- **策略默认迁移**：可单元测试老库迁移 SQL（warmup=14+growth=NULL → 7/5；已改过的不动）。
- **浏览器部分**（搜索/阅读/点赞/登录检测）：无法单测，需在已登录小红书账号上手动验证。

## 非目标

- 收藏 / 关注 / 评论 / 发布 / AI 内容（v1 只做搜索→阅读 + 成长期点赞）。
- 小红书 DOM 选择器的自动自愈（实测微调即可）。
