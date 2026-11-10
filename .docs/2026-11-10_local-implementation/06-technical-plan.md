# P06 技术实现方案：Skill／MCP／宿主注入

- 日期：2026-11-10
- 阶段状态：本机实现完成并纳入 P07 macOS arm64 测试包；真实宿主安装态闭环未完成，不越过 P00 Linux ARM64 门槛。

## 1. 范围

实现 `ossia-integrations`，让用户从 CLI 或 Tauri 管理窗口接入 Codex／DSH。集成产物只调用当前本体可执行文件，不复制引擎、不安装模型 SDK、不接管宿主会话。

支持两种互斥模式：

- `skill-mcp`：安装公共 Skill，并通过宿主受支持的配置入口注册 stdio MCP。
- `plugin`：安装带 Skill／MCP 的薄本地插件；DSH 同时声明 Host／Client 入口。

验证使用临时 `CODEX_HOME`、`DSH_HOME` 和测试 profile，不写入用户默认宿主配置；真实默认 profile 和安装态宿主闭环仍未验证。

## 2. 工程布局

- `packages/ossia-integrations`：宿主发现、计划、应用、状态、修复、移除和受管记录。
- `integrations/skill/ossia/SKILL.md`：公共 Skill 源。
- `integrations/codex/`、`integrations/dsh/`：插件模板、图标和本地化资源。
- `<library home>/integrations/`：生成的 marketplace／bundle、受管文件和 `managed.json`。

依赖方向为 `ossia-types ← ossia-integrations ← ossia-runtime`。集成模块不打开 Library，不直接调用乐谱数据库；它只管理宿主配置、受管文件与启动命令。

## 3. 公共接口

`ossia-types` 增加以下序列化 DTO，JSON 使用 camelCase；枚举取值使用 kebab-case：

- `HostKind`：`codex`、`dsh`。
- `IntegrationMode`：`skill-mcp`、`plugin`。
- `IntegrationAction`：`detect`、`plan`、`apply`、`status`、`repair`、`remove`。
- `IntegrationState`：`unavailable`、`available`、`planned`、`pending-host`、`installed`、`conflict`、`broken`、`unsupported`。
- `IntegrationRequest`：`action`、可选 `host/profile/mode`、可选 `planHash`。
- `HostDetection`：宿主、版本、可执行文件、配置根、Skill 根、profiles、能力和问题。
- `IntegrationPlan`：稳定 `planHash`、管理 ID、目标、绝对 home／executable、写入条目、冲突、警告和宿主待完成标记。
- `IntegrationEntry`：`create`／`replace`／`remove`、所有权 ID、目标路径、类型、内容哈希。
- `IntegrationConflict`：目标、类别和可读消息。
- `IntegrationStatus`：当前模式、检测结果、受管记录和逐项观测结果。

CLI 使用嵌套子命令：

```text
ossia integrate detect
ossia integrate plan '<json>'
ossia integrate apply '<json>'
ossia integrate status '<json>'
ossia integrate repair '<json>'
ossia integrate remove '<json>'
```

`plan`、`apply`、`repair` 需要 host／mode；`apply` 还必须携带与重新计算计划一致的 `planHash`。错误复用现有稳定错误码，不新增协议错误。

## 4. 宿主适配

### 4.1 发现

- Codex 配置根：显式 `CODEX_HOME`，否则 `~/.codex`；用户 Skill 根为 `$HOME/.agents/skills`。
- DSH 配置根：显式 `DSH_HOME`，否则 `~/.dsh`；从 `profiles/*/package.json` 读取可用 profile。
- 可执行文件优先使用显式 `OSSIA_CODEX_BIN`／`OSSIA_DSH_BIN`，其次当前平台安装位置与 PATH 上的宿主 CLI。只有 `--version` 和所需子命令探测成功才报告已安装。
- 不按 cwd 推断仓库路径。集成模板根默认来自本体安装资源；开发／测试可显式传入资源根。

### 4.2 Codex

`skill-mcp`：

1. 写入 `$HOME/.agents/skills/ossia/SKILL.md`。
2. 执行 `codex mcp add ossia -- <绝对exe> mcp --home <绝对home>`；需要开发资源时只附加显式环境值。
3. 用 `codex mcp list --json` 核对命令、参数、启用状态和 Skill 哈希。

`plugin`：

1. 生成 `<home>/integrations/codex/marketplace/{.agents/plugins/marketplace.json,plugins/ossia/...}`。
2. 插件含 portable `plugin.json`、`mcp.json`、`skills/ossia/SKILL.md` 和图标。
3. 执行 `codex plugin marketplace add <root>`、`codex plugin add ossia@ossia-local`。
4. 用 `codex plugin list --json` 和插件目录哈希核对来源、安装和启用状态。

### 4.3 DSH

`skill-mcp`：

1. 写入 `$DSH_HOME/skills/ossia/SKILL.md`。
2. 生成仅含配置层的本地 bundle，通过 `dsh plugin --profile <profile> add <bundle>` 注册 `@deepseek-ai/dsh-mcp-client`，避免改写用户含自定义标签的 profile patch。

`plugin`：

1. 生成预构建 bundle，声明 `dsh.bundle`、`dsh.client`、`engines.dsh`、Host／Client 导出和图标／本地化资源。
2. Host 配置注册同一 stdio MCP；Client 入口只提供包身份与本地化，不创建第二套谱面 UI。Agent 返回的一次性 URL 仍由 DSH Sidebar Browser／链接行为打开。
3. Web profile 若 Browser 禁用，在 bundle 中只覆写 `ui-sidebar-browser` 的启用项；Desktop 保持既有启用状态。

## 5. 计划、事务与所有权

- `plan` 读取现状、计算目标文件与宿主动作、生成稳定哈希，不写任何文件。
- `apply` 重新计算并核对 `planHash`，先检查全部冲突，再快照受影响文件和命令输出。各步骤失败时恢复文件、撤销已执行的宿主动作并保留旧记录。
- `status` 同时检查：受管记录、文件哈希、宿主配置／插件清单、可执行路径和资源可用性。标记文件存在不等于已连接。
- 模式切换先撤销旧模式受管条目，再安装新模式；失败时恢复旧模式。不得并行注册两个同名 MCP server。
- `repair` 只重写哈希仍匹配本工具所有权的文件；用户在宿主配置中改写的条目返回 `conflict`，不无条件覆盖。
- `remove` 只删除与记录匹配的条目；保留宿主的其他服务、插件、设置和全部乐谱数据。
- 所有普通文件写入使用临时文件＋原子重命名；Unix 下受管目录权限为当前用户私有。

## 6. Skill

Skill 固定描述真实工作流：`status/guide` → `list/import` → `open` → `context` → `edit` → 文件编辑 → `check/job` → `commit` → 报告版本与验证范围。它不内嵌动态版本、当前 home 或 JSON schema；这些信息由 `guide`／MCP 返回。

## 7. 管理窗口与 CLI

- CLI 与管理窗口共用 `ossia-integrations` 请求和结果。
- Tauri 只增加管理窗口可调用的 detect／plan／apply／repair／remove 命令；不新增 HTTP 或 MCP 工具，也不向谱面网页暴露 native bridge。
- 管理页显示每个宿主的版本、配置根、profiles、模式、状态、计划变更和冲突；不存在、待重启或待宿主启用时明确显示，不显示“已连接”。

## 8. 测试与验收

自动化覆盖：

- 空配置、已有其他服务、同名非受管条目、损坏／只读配置、中文和空白路径、重复 apply、repair、模式切换、remove、用户改写冲突、可执行路径迁移。
- 生成物的 JSON／YAML 可解析，Codex marketplace／plugin 与 DSH bundle manifest、Skill、绝对命令和哈希正确。
- Codex `mcp list --json`／`plugin list --json` 与 DSH `--dump-config` 核对实际配置，不以标记文件代替。
- 两个宿主生成的 MCP 配置连接同一测试库时只有一个持库实例，view 选区隔离、编辑提交串行。
- 管理页在普通浏览器中保持只读；原生窗口可读取状态并展示操作。

命令检查：`cargo fmt --all -- --check`、`cargo test --workspace`、`cargo clippy --workspace --all-targets -- -D warnings`、`pnpm check:types`、`pnpm check:contracts`、UI typecheck／build、`pnpm test:ui`。

隔离宿主验证只使用临时 `CODEX_HOME`／`DSH_HOME`。若模型驱动 GUI 闭环不可用，记录为宿主验收缺口，不把 CLI 配置检查写成真实宿主已验收。

## 9. 明确边界

- P06 自身不发布独立插件、不打包安装器、不修改默认用户宿主 profile。
- 不使用 `OSSIA_DATA_DIR`，不接管模型会话或凭证。
- P07 macOS arm64 测试包已产出，但 Linux ARM64 及其他平台的原生宿主行为仍待目标机验收。
