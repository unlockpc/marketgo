# MarketGo

**Go-To-Market Automation（全球营销自动化）** —— UnzooAI 产品家族的一员。

输入产品 URL，自动生成多语言营销内容，并发布到多个平台。

> UnzooAI 家族：**Unzoo**（AI 的手 —— 浏览器自动化）、**SoloMD**（AI 的眼 —— 文档处理）、**MarketGo**（AI 的嘴 —— 全球营销）。

> 原名 **UnMarket**。npm 包名与 CLI 命令仍为 `unmarket`。

> English version: [README.md](./README.md)

## 功能特性

- **CLI**：覆盖分析、生成、发布、调度的完整命令行工作流
- **桌面应用**：基于 Tauri 的 GUI（Rust 后端 + TypeScript 前端，中文/英文界面）
- **MCP 服务**：通过 Model Context Protocol 向 AI 智能体暴露核心能力
- **多语言**：AI 生成与翻译内容（支持 18 种语言代码）
- **多 AI 供应商**：Anthropic、OpenAI、Google、OpenRouter，以及多家国内供应商
- **多平台发布**：API 直发与浏览器自动化发布（通过 Unzoo）

## 项目结构

```
src/            TypeScript 源码（CLI + 核心 + MCP 服务）
  cli/          基于 Commander 的 CLI 命令
  core/         crawler、content-generator、publisher、scheduler、ai-engine 等
  storage/      SQLite 数据库、配置（YAML）、加密的账号存储
  browser/      Unzoo 浏览器客户端
  mcp/          MCP 服务
src-tauri/      Tauri 桌面应用（Rust）
dist/           编译产物（tsc）—— CLI 入口为 dist/index.js
```

运行时数据位于 `~/.unmarket/` 目录下（`config.yaml`、SQLite 数据库、以及 `logs/`）。

## 安装

需要 **Node.js >= 18**。

### CLI

```bash
git clone <repo>
cd unmarket
npm install
npm run build:cli   # 先重新编译 better-sqlite3（原生模块），再执行 tsc
npm link            # 将 `unmarket` 命令注册为全局可用
```

`npm run build:cli` 会重新编译 `better-sqlite3` 原生模块并编译 TypeScript。
若只需重新编译原生模块，使用 `npm run rebuild:node`。
发布的可执行命令将 `unmarket` 映射到 `dist/index.js`。

### 桌面应用（Tauri）

桌面应用是一个 Tauri 项目（Rust 后端、TypeScript 前端）。构建时需要
[Rust 工具链](https://www.rust-lang.org/tools/install) 以及 Tauri CLI
（已作为 devDependency 安装）。

```bash
npm install
npm run tauri:dev     # 以开发模式运行桌面应用
npm run tauri:build   # 生成安装包
```

当前配置的打包目标为 **Windows 安装包**（`.msi` 与 NSIS `.exe`，安装程序语言包含
英文与简体中文）。详见 `src-tauri/tauri.conf.json`。

## CLI 快速上手

### 1. 添加产品

**手动创建（无需浏览器）：**

```bash
unmarket product create \
  --name "My Product" \
  --url "https://example.com" \
  --tagline "A great tool for developers" \
  --description "My Product helps developers build faster." \
  --type tool \
  --features "Fast,Easy to use,Open source" \
  --platforms "twitter,reddit,hackernews" \
  --languages "en,zh"
```

`--type` 可选值：`tool`、`saas`、`app`、`service`、`library`、`plugin`。

**添加演示产品用于测试：**

```bash
unmarket product demo
```

**从 URL 分析（需要 Unzoo 浏览器 + 已配置 AI）：**

```bash
unmarket analyze https://example.com --save   # --save 将其保存为产品
unmarket product add https://example.com      # 分析并保存，一步完成
```

### 2. 预览内容

```bash
# 演示内容（无需 AI）
unmarket preview <product-id> --demo -p "twitter,linkedin" -l "en,zh"

# AI 生成（需要配置 AI；未配置时回退为演示内容）
unmarket preview <product-id> -p "twitter,linkedin"
```

`-p, --target-platforms` 与 `-l, --target-languages` 接受逗号分隔的列表
（默认值：`twitter` / `en`）。

### 3. 发布

```bash
# 模拟（dry run）—— 不会真正发布
unmarket publish <product-id> -n -p "twitter,reddit" -l "en"

# 实际发布
unmarket publish <product-id> -p "twitter,reddit" -l "en,zh"
```

`publish` 的模拟标志是 `-n, --simulate`。（顶层的
`unmarket <url>` 命令则使用 `--dry-run`。）

## 配置

```bash
unmarket config init          # 创建 ~/.unmarket/config.yaml
unmarket config show          # 显示当前配置（加 --json 输出原始 JSON）
unmarket config set <k> <v>   # 例如 unmarket config set ai.model gpt-4.1
unmarket config get <key>
unmarket config reset --force
```

### AI 供应商

```bash
unmarket ai models                # 列出供应商与模型
unmarket ai config                # 无参数时显示供应商列表……
unmarket ai config --provider openai --model gpt-4.1 --api-key <key>
unmarket ai test                  # 检查 AI 是否已配置
```

支持的供应商（`AI_PROVIDERS`）：

| Key | 供应商 |
|-----|--------|
| `anthropic` | Anthropic (Claude API) |
| `openrouter` | OpenRouter（多模型）|
| `openai` | OpenAI (GPT) |
| `google` | Google (Gemini) |
| `qwen` | 阿里巴巴（通义千问 Qwen）|
| `doubao` | 字节跳动（豆包 Doubao）|
| `deepseek` | DeepSeek |
| `glm` | 智谱（GLM）|
| `kimi` | 月之暗面（Kimi）|

默认供应商/模型为 `openai` / `gpt-4.1`。`qwen`、`doubao`、`deepseek`、`glm`、`kimi`
使用 OpenAI 兼容的 chat 接口。

## 产品管理

```bash
unmarket product list                 # 加 --all 包含未激活产品
unmarket product show <id>
unmarket product update <id> --priority 8 --weight 5
unmarket product activate <id>
unmarket product deactivate <id>
unmarket product delete <id>
```

## 账号管理

```bash
unmarket account list                 # 加 --platform <p> 按平台过滤
unmarket account show <platform>
unmarket account verify               # 列出需要人工验证的账号
unmarket account count                # 按平台统计账号数量
unmarket account delete <platform>
```

> 账号通过自动注册（见下文）或桌面应用创建 —— CLI 中没有 `account add` 命令。

### 自动注册（需要 Gmail + Unzoo）

```bash
unmarket gmail login            # 在 Unzoo 浏览器中登录 Gmail
unmarket gmail status
unmarket register twitter       # 注册单个平台
unmarket register --all         # 在所有支持的平台上注册
```

支持自动注册的平台：`twitter`、`reddit`、`linkedin`、`devto`、`hackernews`、
`producthunt`。部分平台需要人工验证，流程会暂停等待。

## 调度器

```bash
unmarket run --mode weighted                      # 持续运行
unmarket run --mode round-robin --duration 24h    # 限时运行
unmarket run --mode priority --products id1,id2   # 指定产品
unmarket run --daemon                             # 后台运行
unmarket run stop                                 # 停止正在运行的调度器
```

调度模式（`--mode`，默认 `weighted`）：
- `round-robin`：在产品间等比轮换
- `weighted`：权重越高的产品发布越频繁
- `priority`：优先发布优先级最高的产品
- `smart`：基于分析数据优化

## 队列管理

```bash
unmarket queue list                 # 别名 ls；加 --all 包含已完成任务
unmarket queue add <product-id> --platform <p> --time <iso-8601>
unmarket queue remove <task-id>
unmarket queue pause
unmarket queue resume
unmarket queue clear --force        # 必须加 --force 才会真正清空
```

## 统计

```bash
unmarket stats                          # 总体统计
unmarket stats <product-id>             # 单个产品的统计
unmarket stats --platform twitter --days 30 --top 5
```

可选项：`--platform`、`--days`（默认 `7`）、`--top <n>`。

## 浏览器控制（通过 Unzoo）

```bash
unmarket browser status         # 检查 Unzoo 是否可用
unmarket browser open [url]
unmarket browser navigate <url>
unmarket browser screenshot [path] --full-page
unmarket browser close
```

## MCP 服务

MarketGo 内置一个 MCP 服务，向 AI 智能体暴露核心能力。

```bash
npm run mcp        # 启动 stdio MCP 服务（dist/mcp/server.js）
```

客户端配置示例见 `mcp.json`。暴露的工具：
`analyze_product`、`list_products`、`add_product`、`generate_content`、
`list_accounts`、`gmail_status`、`register_account`。

## 支持的平台

内容生成对任意平台字符串都有效，但**发布**目前实现了以下平台：

### API 直发
- Dev.to
- Hashnode
- Medium
- Discord（webhook）
- Telegram（bot）
- Mastodon

> `github` 与 `slack` 已被识别但尚未实现（GitHub 需要仓库配置）。

### 浏览器自动化发布（需要 Unzoo）
- Twitter / X
- LinkedIn
- Facebook
- Reddit
- 微博（Weibo）

更完整的目标平台清单、注册方式与自动化可行性分析，作为项目路线图记录在
[`PLATFORMS.md`](./PLATFORMS.md) 中。

## 支持的语言

AI 引擎可识别以下语言代码用于生成/翻译：

```
en, zh, zh-TW, ja, ko, de, fr, es, pt, ru, ar, hi, th, vi, id, tr, pl, nl
```

默认配置启用其中一个子集：`en, zh, ja, ko, de, fr, es, pt, ru, ar`。

## 环境变量

```bash
LOG_LEVEL=debug      # 设置控制台日志级别（默认：warn）
DEBUG=1              # 启用调试日志
```

全局 `--debug` 标志与 `DEBUG=1` 效果相同。日志写入 `~/.unmarket/logs/`。

## 开发

```bash
npm run build        # tsc
npm run dev          # tsc --watch
npm run lint         # eslint
npm test             # vitest
```

## 许可证

MIT
