# X 养号自动回复 — 设计文档

日期：2026-06-29
状态：待用户复核
分支：fix/ai-settings-and-docs

## 背景与目标

X(Twitter) 养号 runner (`x_nurture_run`, nurture.rs:706) 现做：搜索 niche → 点赞、关注、转推、AI 原创发帖。**没有回复动作**——历史上做过但被移除，原因是"详情页正文抓取在养号场景不可靠，且自动回复质量难保证"。

目标：给 X 养号加**自动回复**——养号时读取搜索到的推文正文，调用已配置的大模型生成**养号互动型**回复（纯社交、友善、切题、不带产品），按配额直接发出。

**可行性已验证**（2026-06-29，在已登录 X 的 `jinguichao` profile 实测）：
- 搜索能采到 `/status/` 链接；
- 详情页 `article [data-testid="tweetText"]` 能稳定读到主推文完整正文（实测 340/490 字符），带 `lang` 属性可判语言、不混入评论；
- 回复框 `[data-testid="tweetTextarea_0"]` 在详情页就位。

## 已确认决策

1. **回复目的**：养号互动型（纯社交友善回复，不带产品、不推销）。用 `gen_nurture_text(kind="x_reply", 推文正文)`。
2. **发送方式**：养号时直接自动发（不走人工审核队列）。
3. **配额** `x_reply_quota(phase)`：**预热 1 / 成长 1 / 成熟 2**。
4. **开关**：新增设置项「X 自动回复」，**默认关**；关时养号完全不回复（不影响现有行为）。

## 风险与兜底（重点）

自动发回复是高风险动作（封号 + AI 质量不稳）。三道兜底：
1. **读不到正文不回复**：`article [data-testid="tweetText"]` 读到非空且 ≥15 字才继续，否则跳过该条（纯图/视频/拿不到都跳过）。
2. **生成不合格不发**：`gen_nurture_text` 返回 None（无 key/调用失败/超长/含链接等）即跳过，绝不发兜底话术。
3. **限速 + 去重 + 开关**：每条回复间随机间隔 30~90s、回复前先停留"阅读"；`x_actions_log` 跨 session 去重不重复回复同一推文；受 `NURTURE_STOP` 停止开关控制；总开关默认关。

风控不可消除——文档化告知，用户已接受。

## 架构总览

```
x_nurture_run（nurture.rs, async）
  ├─ 现有：搜索 niche → 采 status 链接 → 点赞/关注/转推/原创
  └─ 新增「回复」阶段（开关开 + 配额>0 时）：
       从已采到的 status 链接取 quota 条，逐条：
         1. x_read_tweet_text_blocking(url) → Option<正文>   (spawn_blocking: 导航详情页+读 tweetText)
         2. 跳过：正文 None / x_already_acted(reply,url) / nurture_should_stop
         3. gen_nurture_text(app,"x_reply",正文).await → Option<回复>  (async: 调大模型)
         4. 跳过：回复 None
         5. x_send_reply_blocking(回复) → bool                (spawn_blocking: twitter_reply 注入+提交)
         6. x_record_action(reply, url) + emit 进度 + 随机间隔 30~90s
```

回复编排放在 `x_nurture_run` 的 async 上下文（与现有"原创发帖"一致：blocking 读 → await 生成 → blocking 发）。

## 组件

### 1. 开关（config + 命令 + 前端）
- config key `x_nurture_reply_enabled`（"1"=开，缺省/其它=关）。
- 复用现有 AI 设置命令体系：在 `get_ai_config`/`set_ai_config` 里带上该字段，或新增 `set_x_reply_enabled(enabled: bool)` / 读取走 `engine_cfg_get`。**采用**：扩展 `get_ai_config`/`set_ai_config` 返回/接收 `x_reply_enabled`，前端 AI 设置页加一个勾选框。
- runner 入口检查：关 → 跳过整个回复阶段。

### 2. 配额（纯逻辑，可单测）
```rust
fn x_reply_quota(phase: &str) -> i64 {
    match phase { "growth" => 1, "mature" => 2, _ => 1 } // warmup 及兜底=1
}
```

### 3. 读正文 `x_read_tweet_text_blocking(tweet_url) -> Option<String>`
- `unzoo_navigate(tweet_url)` + 等加载（轮询 `article [data-testid="tweetText"]` 出现，最多 ~8s）。
- `unzoo_evaluate` 取 `article [data-testid="tweetText"]` 的 innerText。
- trim 后非空且 `chars().count() >= 15` → `Some(text)`，否则 `None`。

### 4. 生成（复用现有）
- `gen_nurture_text(app, "x_reply", &text).await` → `Option<String>`。
- `gen_nurture_text` 的 `x_reply` 分支已约束：针对推文内容、同语言、≤200~280 字符、无链接/@/hashtag、不合格返回 None（ai.rs:194 区）。若现有约束不足，在该分支微调 prompt（同语言要点可借助实测拿到的 `lang`）。

### 5. 发回复 `x_send_reply_blocking(reply: &str) -> bool`
- 当前已在推文详情页（读正文那步已导航到位）。
- 复用现有 `twitter_reply()`（lib.rs:10351，Draft.js `execCommand('insertText')` 注入到 `[data-testid="tweetTextarea_0"]` → 点 `[data-testid="tweetButtonInline"]`）。成功返回 true。

### 6. 去重/记录（复用现有）
- `x_already_acted(conn, account_id, tweet_url)` 查是否回过（action_type 不限，但回复用 target=tweet_url；为区分点赞/回复，去重时仅查 `action_type='reply'`——若现有 `x_already_acted` 不分 action_type，新增一个 `x_already_replied(conn, account_id, url)` 或给 x_already_acted 加 action_type 参数）。
- `x_record_action(conn, account_id, "reply", tweet_url)` 记一条。

## 数据流细节

- 回复阶段在点赞/转推之后执行（已采链接复用）。
- 取链接：从 `x_nurture_run` 已有的 `tweets: Vec<String>`（status 链接）里按顺序/打乱取前 `quota` 条。
- 每条独立 try：任一步失败/跳过都 continue 下一条，不整体失败。
- 配额是上限：实际发出可能少于 quota（读不到/生成失败/去重命中都不计入发出）。

## 错误处理

- 开关关 / 配额 0 / 无链接 → 跳过回复阶段，不报错。
- 读正文超时/为空 → 跳过该条。
- 生成 None → 跳过该条。
- 发回复失败（DOM 变动/被拦）→ 记日志，跳过该条，不计 record。
- 未配置大模型 key → `gen_nurture_text` 返回 None，全部跳过（等于不回复）；可在 runner 日志提示"未配置 AI，跳过回复"。

## 测试

- **单元**：`x_reply_quota(phase)`（warmup/growth=1、mature=2）；正文长度门（<15 字 → None 的纯逻辑可抽函数测）。
- **手动**（已登录 X）：开开关 → 跑 X 养号 → 观察：读到正文、生成切题回复、发出、`x_actions_log` 去重、间隔限速、关开关则不回复。

## 非目标

- 营销/带产品回复、intent_score 意向筛选（纯互动）。
- 人工审核队列（reply_history pending_review）——本功能直接发。
- 回复时间线 / 通知 / @我的推文（只回搜索到的 niche 推文）。
- 自动调参/自愈选择器（实测微调即可）。
