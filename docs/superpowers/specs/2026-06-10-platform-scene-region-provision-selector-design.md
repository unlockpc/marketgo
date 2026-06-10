# 平台场景/地区分类 + 开通账号平台选择器 — 设计文档

日期：2026-06-10
分支：fix/ai-settings-and-docs

## 背景与目标

当前「账号管理 → 检查并开通账号」（`persona_provision_all`）会无差别遍历写死的 12 个平台常量（`PROVISION_PLATFORMS`），无法选择、也看不出每个平台属于什么领域/地区。

目标：
1. 给所有平台建立 **场景（scene）+ 地区（region）** 两维分类，元数据落在后端平台配置里，前后端统一取用。
2. 「检查并开通账号」改为：先弹一个**按场景分组**的平台选择器，**已开通的平台预选中且锁定不可改**，用户只勾选新要开通的平台，后端只对选中项执行开通。

## 关键决策（已与用户确认）

- **分类方式**：场景为主分类 + 地区为标签（不是按内容形态/纯地区）。
- **"已开通"判定**：该平台账号存在且 `health_status='healthy'`（真正登录成功）才算。仅打开过登录页、未真正登录的不算。
- **选择器平台范围**：展示**全部 29 个**平台；手机号/本地账号类（无 Google 登录）标注"需手动"。
- **元数据位置**：后端 `PlatformConfig` 增加 `scene` / `region` / `mode` 字段，前端从后端拉取。

## 分类维度定义

### 场景 scene（主分类，6 类）
| key | 名称 | 适合产品 |
|---|---|---|
| `research` | 研发/技术 | 开发者工具、API、开源项目 |
| `product` | 产品/创业 | SaaS、独立产品冷启动 |
| `social` | 通用/大众社交 | 大众 App、泛消费 |
| `content` | 知识/内容 | 软文、深度内容、品牌故事 |
| `career` | 职场/商务 | B2B、企业服务、招聘 |
| `lifestyle` | 生活/种草 | 消费品、效率好物、生活方式 |

### 地区 region（标签）
`us`(海外英文) / `jp`(日本) / `kr`(韩国) / `ru`(俄语) / `cn`(国内) / `global`(全球)

### 开通模式 mode
- `auto`(🟢自动)：支持 Google 登录且非敌意平台 → 走全自动 Google 登录/注册。
- `manual`(🟡需手动)：无 Google 登录（手机号/本地账号）或敌意平台（X/Reddit，自动登录有锁号风险）→ 帮用户打开登录页，由用户手点一次。

判定规则：`mode = auto` 当且仅当 `google_oauth == true && platform 不属于 {twitter, reddit}`，否则 `manual`。

## 全平台分类表（29 个）

| platform key | 名称 | scene | region | mode |
|---|---|---|---|---|
| github | GitHub | research | us | auto |
| devto | DEV.to | research | us | auto |
| hashnode | Hashnode | research | us | auto |
| hackernews | Hacker News | research | us | manual |
| qiita | Qiita | research | jp | auto |
| zenn | Zenn | research | jp | auto |
| habr | Habr | research | ru | auto |
| v2ex | V2EX | research | cn | auto |
| segmentfault | SegmentFault | research | cn | auto |
| csdn | CSDN | research | cn | manual |
| oschina | 开源中国 | research | cn | manual |
| producthunt | Product Hunt | product | us | auto |
| betalist | BetaList | product | us | auto |
| alternativeto | AlternativeTo | product | us | auto |
| indiehackers | Indie Hackers | product | us | auto |
| twitter | Twitter/X | social | us | manual |
| reddit | Reddit | social | us | manual |
| facebook | Facebook | social | us | manual |
| telegram | Telegram | social | global | manual |
| weibo | 微博 | social | cn | manual |
| jike | 即刻 | social | cn | manual |
| vk | VK | social | ru | manual |
| medium | Medium | content | us | auto |
| note | note | content | jp | auto |
| zhihu | 知乎 | content | cn | manual |
| naver_blog | Naver Blog | content | kr | manual |
| linkedin | LinkedIn | career | us | auto |
| xiaohongshu | 小红书 | lifestyle | cn | manual |
| sspai | 少数派 | lifestyle | cn | manual |

跨场景的次要归属（仅备注，不进主分类）：Reddit 亦含强研发板块；知乎亦偏通用；少数派偏数字生活/效率。

## 架构与改动

### 后端（src-tauri/src/lib.rs）

1. **`PlatformConfig` 结构体**新增三个字段：`scene: &'static str`、`region: &'static str`、`mode: &'static str`。在 `get_platform_config`（约 2017 行起）每个平台分支按上表填值。

2. **新增 Tauri 命令 `persona_platform_catalog(persona_id: String)`**，返回 `Vec<PlatformCatalogItem>`：
   ```
   PlatformCatalogItem { platform, name, scene, region, mode, provisioned: bool }
   ```
   - 遍历全部 29 个平台 key。
   - `provisioned` = 存在 `accounts` 行满足 `persona_id=? AND platform=? AND health_status='healthy'`。
   - 注册到 `invoke_handler`。

3. **改造 `persona_provision_all`**：签名增加 `platforms: Vec<String>`，循环体由 `for plat in PROVISION_PLATFORMS` 改为 `for plat in &platforms`。其余 get-or-create 账号行 + `account_auto_login` 逻辑不变。`PROVISION_PLATFORMS` 常量保留作为"全选可开通"的默认参考（可选）。

### 前端（src/tauri-frontend/app.ts）

1. **`personaProvisionAll(id, email)` 改造**：不再 `uiConfirm → invoke`，改为：
   - `await invoke('persona_platform_catalog', { personaId: id })` 拉取目录。
   - 调用新建的分组选择器 modal `pickProvisionPlatforms(email, catalog)`，返回用户选中的平台 key 数组。
   - `await invoke('persona_provision_all', { personaId: id, platforms })`，再 `loadAccounts()`。

2. **新建分组选择器 modal `pickProvisionPlatforms`**（复用现有 `.modal.active` / `.modal-content` 样式，参考 `uiPrompt`/`uiConfirm` 的动态创建方式）：
   - 按 scene 分组渲染（6 组，组标题含 emoji）。
   - 每个平台一个 checkbox，行内显示：名称 + 地区国旗 + mode 徽章（🟢自动 / 🟡需手动）。
   - `provisioned=true` 的项：`checked` + `disabled`，并标"已开通·锁定"。
   - 底部按钮：「全选可开通的」「取消」「开始开通(N)」，N 为当前新勾选数量，动态更新。「全选可开通的」= 勾选所有 `mode=auto && !provisioned` 的平台（手动类不在一键全选范围，避免一次性弹一堆登录页）。
   - 返回 Promise<string[] | null>：取消返回 null；确定返回**新勾选**（不含已锁定）的平台 key 数组。

3. scene/region/mode 的中文名与 emoji 映射：前端维护一张展示用映射（scene key → "💻 研发/技术" 等，region key → 国旗，mode key → 徽章文案）。元数据本身来自后端，前端只做展示翻译。

## 边界与错误处理

- 用户一个都没新勾选就点「开始开通」：提示"没有新选择的平台"，不调用后端。
- `persona_platform_catalog` 失败：toast 报错，不弹选择器。
- 已锁定项不可被取消（disabled），即使 DOM 被改也不会进入提交数组（提交时按 `!provisioned && checked` 过滤）。
- 手动类平台（mode=manual）选中后，后端 `account_auto_login` 走既有 MANUAL 分支（打开登录页），结果归入"需你点一下 N"。

## 测试要点

- catalog 返回 29 项，scene/region/mode 与上表一致。
- 一个 persona 下某平台 health_status=healthy → catalog.provisioned=true → 选择器中预选+禁用。
- 选中 2 个 auto 平台 + 1 个 manual 平台 → provision 只处理这 3 个，返回 "已登录/注册 / 需点一下 / 失败" 计数正确。
- 不选任何新平台 → 前端拦截，不调后端。

## 不做（YAGNI）

- 不做"投放场景预设包"（方案 C）。
- 不在本次引入内容形态维度。
- 不改国内/手动平台的自动化能力（小红书等仍需手动）。
