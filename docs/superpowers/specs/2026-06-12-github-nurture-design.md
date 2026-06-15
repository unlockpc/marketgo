# GitHub 养号设计（L1/L2 · 浏览器 · 按领域）

日期：2026-06-12
状态：设计已确认，待写实现计划

## 1. 目标与范围

**目标**：把 GitHub 账号养成「可信开发者身份」，用于后续推广（在 Discussions/Issues 评论、README、分享 repo 时不被判 spam / sockpuppet）。

**核心判断**：GitHub 的「养号」不能靠通用的「打开主页随机滚动」——那对 GitHub 几乎无价值。GitHub 判断「真开发者」的信号是 profile 完整度、follow/star/watch 的社交活动、领域一致性、账号年龄与真实互动历史。

**范围（已确认）**：
- 做 **L1 + L2**，**不做 L3**（不刷 contribution 绿格 / 不做 commit）。
- 驱动方式：**纯浏览器（Unzoo）**，不引入 GitHub API / PAT / token 存储。
  - 推论：因为没有 API 出口，**不存在「浏览器 IP 与 API IP 不一致」的风控问题**；一切复用 persona 既有的 profile + 专属 socks5 代理 + 指纹隔离。

### L1 / L2 定义

- **profile 补全 —— 人工手动（不自动化）**
  - Name / Bio（及可选头像、profile README）由人工填好，养号引擎不碰 profile 设置。空 profile 的号不应进入 L2 评论（见 L2 解锁闸门），由人工保证 profile 就绪。
- **L1 —— 社交信号（日常，低频）**
  - 纯日常社交动作，无一次性步骤：每个活跃日的动作池（随机化，不固定套路）——浏览 home/explore/trending（拟人停留）、star 1~3 个领域内 repo、follow 0~2 个开发者、watch 0~1 个 repo。
- **L2 —— 真实互动（偶尔，更克制）**
  - 在已有 Issues/Discussions 下发**良性短评论**（如「同样遇到这个问题」「按这个方法解决了，谢谢」），LLM 生成 + 审核闸门，**不带任何推广意图**（推广是独立环节）。
  - 频率按周计（每周 2~4 条），只在已有线程评论、不主动开 issue。
  - 解锁闸门：账号年龄 ≥ 7 天且已有一定 L1 历史。

## 2. 架构与衔接

不新建并行系统，挂到现有 nurture 引擎上：

```
nurture 调度器（现有, 7×24, 活跃时段 + 最小间隔 + per-persona 隔离）
  └─ platform=="github"
       └─ github_nurture_session(account, phase, duration)   ← 新（替换通用滚动）
            · spawn_blocking + 浏览器(Unzoo)            ← 复用引擎既有阻塞范式
            · 复用 persona profile / socks5 代理 / 指纹
```

- `platform=="github"` 时，养号执行从「打开主页滚动」改为调用 GitHub 专属 session。
- 复用现有 `tasks` 表、调度节流、`nurture_phase_and_target` 分期、随机延迟、活跃时段、健康机制。
- 走 `spawn_blocking`（浏览器辅助函数是阻塞 reqwest，不能在 async 里直接调，见项目既有约定）。

## 3. 领域分类（按 persona 多选）

领域多选**按 persona/账号**生效：每个 GitHub 账号开通时选 1+ 领域，存到账号上，养号只在其领域内取目标。领域一致性本身是反检测信号。

领域分类做成 Rust 静态常量 `GH_DOMAINS`（唯一来源），前端通过命令拉取渲染多选框。

| key | 领域 | GitHub topics |
|-----|------|---------------|
| `frontend` | 前端 | frontend, react, vue, angular, typescript, nextjs, tailwindcss |
| `backend` | 后端 | backend, api, nodejs, golang, spring-boot, microservices, graphql |
| `ml` | AI/机器学习（研究/训练） | machine-learning, deep-learning, pytorch, tensorflow, nlp, computer-vision, generative-ai |
| `ai_coding` | AI 编程/Agent/Skills | claude, claude-code, mcp, model-context-protocol, ai-agents, langchain, rag, prompt-engineering, github-copilot, cursor, coding-assistant |
| `data` | 数据工程 | data-science, data-engineering, data-analysis, apache-spark, etl, pandas |
| `devops` | DevOps/云原生 | devops, kubernetes, docker, terraform, ansible, cloud-native, observability |
| `mobile` | 移动开发 | android, ios, flutter, react-native, swift, kotlin, jetpack-compose |
| `security` | 安全 | security, cybersecurity, penetration-testing, cryptography, ethical-hacking, infosec |
| `web3` | 区块链/Web3 | blockchain, ethereum, solidity, web3, smart-contracts, defi |
| `gamedev` | 游戏开发 | gamedev, unity, godot, unreal-engine, game-engine |
| `database` | 数据库 | database, postgresql, mysql, redis, mongodb, sqlite |
| `embedded` | 嵌入式/IoT | embedded, iot, arduino, raspberry-pi, esp32, microcontroller |
| `devtools` | 开发工具/效率 | cli, developer-tools, vscode, neovim, terminal, automation |

设计原则：每个领域 6~8 个真实、高 population 的 topic，至少含 2~3 个超大 population topic（react/kubernetes/machine-learning/claude 等），保证随机取目标时目标池充足。

## 4. 目标选取

1. 读账号 `gh_domains` → 轮选一个领域 → 映射 topics。
2. 浏览器导航 `github.com/topics/<topic>?o=desc&s=stars` 或搜索页，收集候选 repo / 开发者。
3. 用 `gh_actions_log` 过滤该账号已操作过的目标，并应用跨账号去重（见 §5）。
4. 随机挑选 → 执行 star / follow / watch / 停留。
5. L2：在所选领域的 repo 内打开 Discussions/Issues → 挑线程 → LLM 良性评论 → 审核队列 → 提交（复用现有 GitHub Discussions/评论选择器）。

## 5. 反检测 / 去同质化

- **领域一致性**：每号固定在自己所选领域活动。
- **跨账号去重**：限制同一 repo / 开发者在时间窗口内被多少个 persona 触碰，防「多个号同天 star 同一 repo」这一最典型水军特征。靠 `gh_actions_log` 全局查询限流。
- **节奏自然**：每日动作组合随机化（非固定套路）+ 抖动 + 摊到活跃时段 + 最小间隔（复用现有）。量级稀疏：每天 star 1~3 / follow 0~2 / watch 0~1；L2 每周 2~4。
- **分期闸门**：warmup 只做轻 L1（至多 1 star，不评论）；growth 起 L1 全量 + 偶尔 L2；mature 稳定 L1 + L2。L2 额外要求号龄 ≥ 7 天 + 有 L1 历史。
- **隔离**：复用 per-persona 专属 socks5 出口 IP + 指纹。

## 6. 数据模型（最小改动）

- `accounts` 加一列：
  - `gh_domains` —— 所选领域 key 的 JSON 数组。
  - **不加 token 列**；profile 完整度由人工保证、不入库（不加 `gh_profile_setup`）。
- 新表 `gh_actions_log(id, account_id, action_type, target, date, created_at)` —— 节奏控制 + 去重 + 跨账号去同质化。
- `GH_DOMAINS` 静态常量（key/label/topics）+ 一个返回它的 Tauri 命令供前端渲染多选框。

## 7. 失败处理

- 未登录 → 标 `logged_out` 并阻塞，提示重登（复用现有健康机制）。
- 选择器失效 → 记录并跳过该动作、不崩（graceful degrade）。
- L2 评论失败 → 记录 + 跳过/重试。

## 8. 测试

- **Rust 单测**：
  - 领域 → topics 映射可解析；`GH_DOMAINS` 的 key 唯一、topics 非空。
  - `gh_actions_log` 去重 / 跨账号限流逻辑。
  - 分期闸门：warmup 拦 L2、号龄 < 7 天拦 L2。
  - 目标选取排除已操作目标。
- **前端**：多选框渲染后端返回的全部领域；选择持久化到账号。
- **集成（手动，需 Unzoo + 已登录号）**：跑一次 session，能在所选领域内 star 一个 repo、follow 一个开发者、发一条过闸评论。标注为手动验证（需真实浏览器）。

## 9. 明确不做（YAGNI）

- 不做 L3 contribution 绿格 / commit。
- 不引入 GitHub API / PAT / token 存储 / SSH。
- 不做自动 profile 补全（Name/Bio/头像/profile README 均由人工手动完成）。
- 不在养号阶段做任何推广（推广是独立环节）。

## 10. 待定 / 需用户输入

- 用户可告知**与其产品最相关的领域**，以便针对该领域把 topics 调得更精准（不影响架构，仅 `GH_DOMAINS` 内容微调）。
