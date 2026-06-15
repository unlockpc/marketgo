# Unzoo 操作迁移到 human 模式 设计

日期：2026-06-15
状态：设计已确认，待写实现计划

## 1. 背景与目标

marketgo 当前对 Unzoo 的所有点击/输入走 `browser_*` MCP 工具（`/mcp/tools/call`）。这些工具已发 isTrusted=true 的真实键鼠事件，但仍是「机械直线移动、零 typo、固定节奏」。

Unzoo v1.9.0 提供原生 **human 模式**（`human_*` 工具），拟人度与健壮性更高：
- **输入**（`human_type`）：真人节奏 + **打字错+backspace 自我纠正** + 读回校验
- **点击**（`human_click`）：观察→稳健定位（穿透 shadow/同源 iframe + 遮挡检测）→**bezier 轨迹移动**→停顿→可信点击→**验证页面反馈**，返回可解释报告
- **行为档位**（`human_profile_set`）：fast/normal/careful/mobile，可覆盖 pace_ms/dwell_ms/typo_rate/move_steps/think_min/think_max
- **语义定位**：可用 aria-label / role / 可见文本定位（不止 CSS）

**目标**：把 marketgo 对 Unzoo 的**通用点击/输入**迁移到 human 模式，让所有平台的自动化更拟人、更健壮（也缓解「猜 CSS 选择器」的脆弱性）。

**术语澄清**：human 模式是技术；fast/normal/careful/mobile 是其内部速度档位。选 normal 仍是完整 human 模式（bezier/typo/反馈校验全有），只是默认走「普通人速度」。

## 2. 迁移机制（关键）

`unzoo_click`（17 处调用）与 `unzoo_type`（22 处调用）各自**只有一个内部出口**（`unzoo_mcp("browser_click")` / `"browser_type"`）。因此**只改这两个底层助手内部**转调 human 工具，即可让全部调用点 + 上层包装（`unzoo_click_any`、marketgo 自己的 `human_click`/`human_type`、各平台动作、GitHub star/follow/watch、回复里的点击步骤）一次性受益，**签名不变、调用点零改动**。

## 3. 行为档位策略

- 启动时设全局默认 `human_profile_set("normal")`（拟人与速度平衡）。
- **本轮只设全局档位**，不做 per-call 覆盖。敏感场景（后续 X 养号 / warmup / 发推）抬到 `careful` 的「按调用覆盖」机制留到子项目 B 再加（human_* 工具本就支持 `profile` 参数，届时加一个带 profile 的助手变体即可）。

## 4. 迁移范围

**✅ 本轮切到 human 模式：**
- `unzoo_click` → `human_click`
- `unzoo_type` → `human_type`

（连带覆盖 `unzoo_click_any`、marketgo `human_click`/`human_type` 包装，以及所有经由它们的平台动作。）

**⛔ 本轮不迁（保持现状）：**
- **富文本专用输入**：`unzoo_twitter_type`（Draft.js）、`unzoo_prosemirror_type`（Medium/知乎 ProseMirror）。它们走编辑器专用注入，human_type 的「输入+读回校验」在 contenteditable 上有风险。**注意**：这些编辑器之前的「点击聚焦」若走 `unzoo_click`，仍会自动切到 human_click。列为后续单独评估项。
- `unzoo_scroll` / `unzoo_hover`：browser_* 已是 isTrusted 真实滚轮/移动，且 human_* 无独立对应。
- `unzoo_navigate` / `unzoo_evaluate` / `unzoo_get_text`：非输入交互。
- `unzoo_upload`：human_upload 存在但低优先，本轮不迁。

## 5. 助手实现

- 新增 `unzoo_human_click(selector: &str) -> Result<(), String>`：
  - 取 active tab → `unzoo_mcp("human_click", { tab_id, selector })`（不传 profile，走全局档位）
  - 用 CSS `selector`（最高优先级）对齐现有调用点；语义定位（text/role）留给新代码（子项目 B）按需使用。
- 新增 `unzoo_human_type(selector: &str, text: &str) -> Result<(), String>`：
  - `unzoo_mcp("human_type", { tab_id, selector, text })`
- `unzoo_click` / `unzoo_type` 改为薄包装，按开关（见 §6）路由到 human 或 browser。
- **遮挡处理**：human_click 默认 `force=false`；失败 → 记 `[HUMAN]` 报告并返回 Err。现有调用方已处理 Err（如 `gh_click_first` 试下一个选择器、`post_reply_to_url` 轮询重试），行为兼容。

## 6. 安全开关

新增配置项 `unzoo_input_mode`（值 `human` | `browser`，默认 `human`），存 `config` 表。

- `unzoo_click`/`unzoo_type` 读该开关：`human` → 走 human 助手；`browser` → 走原 browser_* 路径。
- 目的：万一 human 模式在某平台翻车，能一键切回 browser_*，无需回滚代码。
- 启动时按开关决定是否调用 `human_profile_set`。

## 7. 错误处理 / 返回

- `unzoo_mcp` 返回工具文本报告；沿用现状：MCP 调用 HTTP 成功即视为成功，失败返回 Err。
- 把 human_* 的可解释报告记到日志（info/debug）便于诊断定位方式/遮挡/反馈。

## 8. 验证

浏览器交互无法单测，迁移后做**跨平台冒烟**（需 Unzoo + 已登录号）：
1. GitHub 养号 star/follow 经 human 模式仍能点中。
2. 一条回复或发布的点击路径仍正常。
3. 登录态检测（导航类）不受影响。
4. 启动日志确认 `human_profile_set("normal")` 已应用。
5. 切换 `unzoo_input_mode=browser` 能回到旧行为（开关有效）。

可加的轻量自动化检查：单测 `unzoo_input_mode` 开关的读取/默认值逻辑（纯函数层面）。

## 9. 风险 / 范围外

- **整体变慢**：human 模式比 browser_* 慢；可接受，必要时调档或对非敏感批量操作用 fast。
- **富文本编辑器不迁**：Twitter/Medium/知乎 的文本注入保持现状，单独评估。
- **遮挡失败暴露隐患**：human_click 遮挡默认失败，可能让个别平台原本「蒙着点」的流程报错 → 靠安全开关 + 冒烟兜底。
- 不在本轮：X 养号（子项目 B，建立在本迁移之上）。
