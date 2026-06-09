# MarketGo 产品规划（对标 AiToEarn 的能力补齐）

> 背景：AiToEarn（开源、19K★、MIT）是同赛道——AI 内容 **Create · Publish · Engage · Monetize** 覆盖 14 平台。
> 本规划：把它有、我们缺的能力，按"提升产品体验"的 ROI 集成进 MarketGo；同时守住我们的护城河。

## 0. 护城河：发布方式（不能丢）

| | AiToEarn | **MarketGo** |
|---|---|---|
| 发布机制 | OAuth 2.0 **官方 API** | **Unzoo 真人浏览器**（登录态在真实 UI 发）|
| 限制 | API 要审批/限流/多数不对个人号开放；矩阵号几乎做不到 | 任何平台/任何个人号/任何内容、无配额 |
| 多账号防关联 | 弱（创作者授权自己账号）| **1 邮箱=1 profile=1 机场IP=1 指纹**，真人指纹 |

**结论：核心场景"多账号矩阵 + 真人级铺量"我们更强。补能力时不要倒退成 API 路线。**

## 1. 能力补齐清单（按 ROI 排序）

| # | 能力 | 集成方案 | 难度 | 状态 |
|---|---|---|---|---|
| 1 | **AI 配图/封面** | 复用 Gemini（Nano Banana / gemini-3-pro-image）做"文字→图"，存本地、加入 post 媒体 | 低 | 🔨 进行中 |
| 2 | **内容日历** | 现有 `scheduled_at` → 月历视图（哪天/哪个邮箱/发什么）| 低 | 🔨 进行中 |
| 3 | **矩阵内容变体** | 按邮箱批量生成**差异化**文案+图（同主题不同表达，防关联）| 中 | 待 |
| 4 | **AI 视频生成** | 文字→视频（Veo/Seedance/Kling）接抖音/TikTok/视频号/YT 发布器 | 高 | 待 |
| 5 | **Engage 升级** | 转化信号识别（"求链接/怎么买"=高意向）+ 品牌提及监控 + 统一互动/线索收件箱 | 中 | 待 |
| 6 | **MCP Server** | 把发帖/获客/身份管理暴露为 MCP tools（CLAUDE.md 本就要求客户端带 MCP）| 中 | 待 |
| 7 | **Monetize 市场** | CPS/CPE/CPM 广告主↔创作者结算——独立业务线，先记 | 战略 | 暂缓 |

## 2. 落地顺序

- **P1 体验快赢（先做）**：① AI 配图 + ② 内容日历 + ③ 矩阵变体 —— 都基于已有能力，几天上线，让"内容发布"体验追平/超过对手。
- **P2**：⑤ Engage 升级（精准获客）+ ⑥ MCP（满足自有规范）。
- **P3 战略大件**：④ AI 视频、⑦ Monetize。

## 3. 技术接入点（已有可复用）
- 文案：`generate_content` / `call_gemini_api` 等（已支持 Gemini/OpenAI/DeepSeek/Qwen）
- 图：Gemini `gemini-3-pro-image-preview`（REST `:generateContent` + `responseModalities:["IMAGE"]`）
- 发布/媒体：`posts` 表（media_paths/media_type）+ 各平台发布器（小红书/抖音/X…）+ Unzoo `/upload`
- 排期：`posts.scheduled_at` + `post_schedule_tick`
