# X(Twitter) 养号设计（L1/L2 + 极少原创 · human 模式 · 按方向）

日期：2026-06-15
状态：设计已确认，待写实现计划
前置：依赖「Unzoo 操作迁移到 human 模式」（`2026-06-15-unzoo-human-mode-migration-design.md`，已实现）

## 1. 目标与范围

**目标**：把 X 账号养成「可信、活跃的真人感账号」，用于后续推广（发产品推/在相关推文下回复时不被限流/封号）。

**核心判断**：X 反自动化极激进、新号极易被限流/shadowban；且「关注一堆人但 0 推文」是典型机器人指纹 → 需要少量原创填充时间线。但 **X 上自动发推是头号封号触发点**，故原创=极少量 + 最严闸门。

**范围（已确认）**：
- **L1**：浏览 home/explore + 点赞 + 关注
- **L2**：转推 + 良性回复
- **L3**：极少量原创推文（良性、不带推广、强闸门）
- 驱动：**浏览器，走 Unzoo human 模式**（子项目 A 已落地，`unzoo_click`/`unzoo_type` 已转 human）；**语义定位优先**（text/role/aria-label，不猜 data-testid）。
- 按 persona **方向(niche)多选**，目标按方向→关键词搜索取。

## 2. niche 分类（X 16 官方方向，双语展示）

来源：X 消费端 Topics 的 16 个顶层方向。前端展示用「English(中文)」。做成 Rust 静态常量 `X_NICHES`（单一来源），前端经命令拉取渲染多选框。

| key | label（展示） | X 搜索关键词 |
|-----|--------------|------------|
| `technology` | Technology(科技) | technology, tech, AI, software |
| `business_finance` | Business & finance(商业财经) | business, finance, investing |
| `science` | Science(科学) | science, research |
| `careers` | Careers(职业) | careers, jobs, hiring |
| `gaming` | Gaming(游戏) | gaming, games |
| `news` | News(新闻) | news, breaking news |
| `entertainment` | Entertainment(娱乐) | entertainment |
| `arts_culture` | Arts & culture(艺术文化) | art, culture |
| `music` | Music(音乐) | music |
| `movies_tv` | Movies & TV(影视) | movies, film, TV |
| `sports` | Sports(体育) | sports |
| `fashion_beauty` | Fashion & beauty(时尚美妆) | fashion, beauty |
| `food` | Food(美食) | food, cooking |
| `travel` | Travel(旅行) | travel |
| `outdoors` | Outdoors(户外) | outdoors, hiking |
| `hobbies` | Hobbies & interests(兴趣爱好) | hobbies, DIY |

按 persona 多选；选几个方向 = 像有多元兴趣的真人。面向研发的产品通常选 Technology/Business & finance/Science/Careers。

## 3. 架构（复用 GitHub 养号骨架）

- 在 nurture arm + `quick_nurture`/`start_account_nurture` 加分发：`platform=="twitter"|"x"` → `x_nurture_run`（async）。
- `x_nurture_run` 流程：
  1. 读 `x_niches` + 号龄分期（复用 `nurture_phase_and_target`，twitter warmup=21 天）。
  2. 选一个方向 → 关键词（时间种子）。
  3. 浏览器导航 X 搜索：`https://x.com/search?q=<kw>&f=live`（最新推文，用于点赞/回复/转推）、`&f=user`（用户，用于关注）。
  4. 采推文链接 / @handle → 去重（本账号 `x_actions_log` + 跨账号 ≥3 排除）→ 选取。
  5. 按分期配额执行 L1/L2 动作（human 模式语义定位）。
  6. 满足 L3 闸门时发 1 条极少原创。
  7. 如实记录耗时 + 动作（同 GitHub 优化后的记录方式）。
- 走 `spawn_blocking`（阻塞 reqwest 不能在 async 直接调）。

## 4. 目标采集（X 搜索）

- 方向→关键词 → `x.com/search?q=<kw>&f=live` 采推文 permalink（`a[href*='/status/']`）；`&f=user` 采 @handle/profile 链接。
- 用 `x_actions_log` 过滤本账号已操作 + 跨账号触碰过多。
- 语义定位优先：human 模式可按 aria-label/role/text 定位推文操作按钮，弱化 CSS 选择器脆弱性。

## 5. 动作实现（human 模式语义定位，best-effort 仍需实测校准）

| 动作 | 定位（优先语义） |
|------|------------------|
| 点赞 like | `role=button` + aria-label 含 "Like"（已 Like 为 "Liked"，跳过避免取消） |
| 转推 retweet | aria-label "Repost" → 确认菜单 "Repost" |
| 回复 reply | aria-label "Reply" → `unzoo_human_type` 输入正文 → "Reply"/"Post" 按钮 |
| 关注 follow | 推文/用户卡片/profile 上可见文本 "Follow"（"Following" 跳过） |
| 原创 tweet | home compose（"What is happening?!" 输入框）→ `unzoo_human_type` → "Post" |

回复/原创正文复用「良性短文案池」（不带链接/推广，参考 GitHub 的 `gh_benign_comment`）。

## 6. 反检测（X 最严，比 GitHub 狠）

- **量级极保守**（按分期）：
  - warmup（0–21 天）：仅浏览 + 1~2 赞/天；**不关注、不转推、不回复**。原创仅当号龄≥14 起、极低频（见 L3 闸门）。
  - growth（21–42 天）：+ 关注 1~3/天、点赞 3~5/天、偶尔回复/转推；原创维持极低频。
  - mature（42 天+）：维持上述 + 原创低频。
- **原创(L3)闸门**：号龄 ≥ 14 天 **且** 已有 L1 历史（点赞/关注记录）；频率上限 ~1~2 条/周。意图：让号在 warmup 后段就有零星推文（比「21 天 0 推突然发」更自然），但严控频率。
- **关注最克制**：X 对 follow 速率/churn 盯得最死 → 关注量最低、间隔最长。
- 强抖动 + 长最小间隔 + 摊到活跃时段（复用现有）。
- **跨账号去同质化**：限制同一推文/账号被多少 persona 触碰（`x_actions_log` 全局查询），防多号赞同一推。
- 复用 per-persona 代理/指纹隔离 + human 模式拟人输入。

## 7. IP 策略调整

`platform_ip_policy`：**`twitter` | `x` 从 `static_overseas` 改为 `shared_overseas`**（per-persona 专属机场节点，区域稳定即可）。

理由：X 对数据中心/VPN 级 IP 比小红书宽容，不需国外固定专线；多账号的底线是**每号专属 + 区域稳定、绝不共用**，per-persona 专属机场节点（优化点 #11 失效自动换相似节点 → 区域稳定）已满足，且成本更低。

## 8. 数据模型（最小改动）

- `accounts` 加一列：`x_niches`（所选方向 key 的 JSON 数组）。
- 新表 `x_actions_log(id, account_id, action_type, target, date, created_at)`；`action_type ∈ {like, follow, retweet, reply, tweet}`；索引 (account_id, action_type) 与 (target)。
- `X_NICHES` 静态常量（key/label/keywords）+ 命令 `x_niches_catalog` / `get_account_x_niches` / `set_account_x_niches`。
- 前端：X 账号卡片加「🎯 方向」入口（多选弹窗，打开时回勾已选）。

## 9. 失败处理 / 记录

- 未登录 → 标 `logged_out` 并阻塞提示重登（复用现有健康机制 + human 模式的反馈校验更易判断）。
- 动作定位失败 → 记 `[X-ACTION]` 日志并跳过该动作、不崩（降级，参考 GitHub `gh_click_first` 思路）。
- 如实记录本次耗时（`total_seconds` + 累加 `total_nurture_seconds`）+ 每动作写 `x_actions_log`（吸取 GitHub 记录缺口教训）。

## 10. 测试

- **Rust 单测**（纯逻辑）：`X_NICHES` key 唯一/keywords 非空；niche→keywords 映射；目标去重/跨账号限流；分期配额；L3 原创闸门（warmup 拦、号龄<14 拦、无 L1 历史拦）；良性文案不含链接。
- **前端**：方向多选渲染全部 16 项、双语 label、选择持久化 + 回勾。
- **集成（手动，需 Unzoo + 已登录 X 号）**：跑一次 session，能在所选方向内点赞/关注/（满足时）回复；`x_actions_log` 有记录；human 模式 `[HUMAN]` 日志可见；选择器/语义定位实测校准。

## 11. 明确不做（YAGNI）

- 不做大量原创/内容运营（只极少量填时间线）。
- 不在养号阶段做任何推广（推广是独立环节）。
- 不做自动改 profile（头像/bio/banner 由人工，参照 GitHub）。
- 富文本/特殊输入沿用现状；X 正文走 `unzoo_human_type`（Draft.js 兼容性在集成冒烟时验证，若不稳则回退 `unzoo_twitter_type`）。
