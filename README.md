# MarketGo

**Go-To-Market Automation** — part of the UnzooAI product family.

Input a product URL, auto-generate multilingual marketing content, and publish it across many platforms.

> Part of the UnzooAI family: **Unzoo** (AI's hands — browser automation), **SoloMD** (AI's eyes — documents), **MarketGo** (AI's voice — global marketing).

> Formerly **UnMarket**. The npm package name and CLI command remain `unmarket`.

## Features

- **CLI**: Full command-line workflow for analyzing, generating, publishing, and scheduling
- **Desktop App**: Tauri-based GUI (Rust backend + TypeScript frontend, Chinese/English UI)
- **MCP Server**: Exposes core actions to AI agents over the Model Context Protocol
- **Multi-language**: AI-generated and translated content (18 language codes supported)
- **Multi-provider AI**: Anthropic, OpenAI, Google, OpenRouter, and several China-based providers
- **Multi-platform publishing**: API-based and browser-based (via Unzoo) publishers

## Project Layout

```
src/            TypeScript source (CLI + core + MCP server)
  cli/          Commander-based CLI commands
  core/         crawler, content-generator, publisher, scheduler, ai-engine, ...
  storage/      SQLite database, config (YAML), encrypted account storage
  browser/      Unzoo browser client
  mcp/          MCP server
src-tauri/      Tauri desktop app (Rust)
dist/           Compiled output (tsc) — CLI entry is dist/index.js
```

Runtime data lives under `~/.unmarket/` (`config.yaml`, SQLite database, and `logs/`).

## Installation

Requires **Node.js >= 18**.

### CLI

```bash
git clone <repo>
cd unmarket
npm install
npm run build:cli   # rebuilds better-sqlite3 (native) then runs tsc
npm link            # makes the `unmarket` command available globally
```

`npm run build:cli` rebuilds the `better-sqlite3` native module and compiles TypeScript.
If you only need to recompile the native module, use `npm run rebuild:node`.
The published binary maps `unmarket` → `dist/index.js`.

### Desktop App (Tauri)

The desktop app is a Tauri project (Rust backend, TypeScript frontend). Building it
requires the [Rust toolchain](https://www.rust-lang.org/tools/install) and the Tauri CLI
(installed as a dev dependency).

```bash
npm install
npm run tauri:dev     # run the desktop app in development
npm run tauri:build   # produce installers
```

The configured bundle targets are **Windows installers** (`.msi` and an NSIS `.exe`,
with English/Simplified-Chinese installer languages). See `src-tauri/tauri.conf.json`.

## CLI Quick Start

### 1. Add a product

**Manual creation (no browser needed):**

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

`--type` accepts: `tool`, `saas`, `app`, `service`, `library`, `plugin`.

**Add a demo product for testing:**

```bash
unmarket product demo
```

**Analyze from a URL (requires Unzoo browser + configured AI):**

```bash
unmarket analyze https://example.com --save   # --save stores it as a product
unmarket product add https://example.com      # analyze and save in one step
```

### 2. Preview content

```bash
# Demo content (no AI needed)
unmarket preview <product-id> --demo -p "twitter,linkedin" -l "en,zh"

# AI-generated (requires AI configuration; falls back to demo if not configured)
unmarket preview <product-id> -p "twitter,linkedin"
```

`-p, --target-platforms` and `-l, --target-languages` take comma-separated lists
(defaults: `twitter` / `en`).

### 3. Publish

```bash
# Simulate (dry run) — no posts are made
unmarket publish <product-id> -n -p "twitter,reddit" -l "en"

# Actual publish
unmarket publish <product-id> -p "twitter,reddit" -l "en,zh"
```

The dry-run flag for `publish` is `-n, --simulate`. (The top-level
`unmarket <url>` command uses `--dry-run` instead.)

## Configuration

```bash
unmarket config init          # create ~/.unmarket/config.yaml
unmarket config show          # show current config (add --json for raw)
unmarket config set <k> <v>   # e.g. unmarket config set ai.model gpt-4.1
unmarket config get <key>
unmarket config reset --force
```

### AI providers

```bash
unmarket ai models                # list providers and models
unmarket ai config                # show providers (no args) ...
unmarket ai config --provider openai --model gpt-4.1 --api-key <key>
unmarket ai test                  # check whether AI is configured
```

Supported providers (`AI_PROVIDERS`):

| Key | Provider |
|-----|----------|
| `anthropic` | Anthropic (Claude API) |
| `openrouter` | OpenRouter (multi-model) |
| `openai` | OpenAI (GPT) |
| `google` | Google (Gemini) |
| `qwen` | Alibaba (Qwen) |
| `doubao` | ByteDance (Doubao) |
| `deepseek` | DeepSeek |
| `glm` | Zhipu (GLM) |
| `kimi` | Moonshot (Kimi) |

The default provider/model is `openai` / `gpt-4.1`. `qwen`, `doubao`, `deepseek`,
`glm`, and `kimi` use the OpenAI-compatible chat API.

## Product Management

```bash
unmarket product list                 # add --all to include inactive
unmarket product show <id>
unmarket product update <id> --priority 8 --weight 5
unmarket product activate <id>
unmarket product deactivate <id>
unmarket product delete <id>
```

## Account Management

```bash
unmarket account list                 # add --platform <p> to filter
unmarket account show <platform>
unmarket account verify               # accounts needing manual verification
unmarket account count                # account count per platform
unmarket account delete <platform>
```

> Accounts are created via auto-registration (below) or the desktop app — there is no
> `account add` CLI command.

### Auto-registration (requires Gmail + Unzoo)

```bash
unmarket gmail login            # log into Gmail in the Unzoo browser
unmarket gmail status
unmarket register twitter       # register a single platform
unmarket register --all         # register on all supported platforms
```

Auto-registration is supported for: `twitter`, `reddit`, `linkedin`, `devto`,
`hackernews`, `producthunt`. Some platforms require manual verification, which the
flow pauses for.

## Scheduler

```bash
unmarket run --mode weighted                      # run continuously
unmarket run --mode round-robin --duration 24h    # time-limited
unmarket run --mode priority --products id1,id2   # specific products
unmarket run --daemon                             # run in background
unmarket run stop                                 # stop a running scheduler
```

Scheduling modes (`--mode`, default `weighted`):
- `round-robin`: equal rotation between products
- `weighted`: more frequent posts for higher-weight products
- `priority`: highest-priority products first
- `smart`: analytics-optimized

## Queue Management

```bash
unmarket queue list                 # alias: ls; --all to include completed
unmarket queue add <product-id> --platform <p> --time <iso-8601>
unmarket queue remove <task-id>
unmarket queue pause
unmarket queue resume
unmarket queue clear --force        # --force is required to actually clear
```

## Statistics

```bash
unmarket stats                          # overall stats
unmarket stats <product-id>             # stats for one product
unmarket stats --platform twitter --days 30 --top 5
```

Options: `--platform`, `--days` (default `7`), `--top <n>`.

## Browser Control (via Unzoo)

```bash
unmarket browser status         # check whether Unzoo is available
unmarket browser open [url]
unmarket browser navigate <url>
unmarket browser screenshot [path] --full-page
unmarket browser close
```

## MCP Server

MarketGo ships an MCP server that exposes core actions to AI agents.

```bash
npm run mcp        # starts the stdio MCP server (dist/mcp/server.js)
```

See `mcp.json` for a sample client configuration. Exposed tools:
`analyze_product`, `list_products`, `add_product`, `generate_content`,
`list_accounts`, `gmail_status`, `register_account`.

## Supported Platforms

Content generation works for any platform string, but **publishing** is implemented
for the following:

### API-based publishers
- Dev.to
- Hashnode
- Medium
- Discord (webhook)
- Telegram (bot)
- Mastodon

> `github` and `slack` are recognized but not yet implemented (GitHub requires
> repository configuration).

### Browser-based publishers (require Unzoo)
- Twitter / X
- LinkedIn
- Facebook
- Reddit
- Weibo

A broader catalog of target platforms, registration methods, and automation feasibility
is documented in [`PLATFORMS.md`](./PLATFORMS.md) as the project roadmap.

## Supported Languages

The AI engine recognizes these language codes for generation/translation:

```
en, zh, zh-TW, ja, ko, de, fr, es, pt, ru, ar, hi, th, vi, id, tr, pl, nl
```

The default config enables a subset: `en, zh, ja, ko, de, fr, es, pt, ru, ar`.

## Environment Variables

```bash
LOG_LEVEL=debug      # set console log level (default: warn)
DEBUG=1              # enable debug logging
```

The global `--debug` flag has the same effect as `DEBUG=1`. Logs are written to
`~/.unmarket/logs/`.

## Development

```bash
npm run build        # tsc
npm run dev          # tsc --watch
npm run lint         # eslint
npm test             # vitest
```

## License

MIT
