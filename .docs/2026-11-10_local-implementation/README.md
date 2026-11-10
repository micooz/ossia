# Ossia 本地架构分阶段实施索引

- 日期：2026-11-10
- 状态：P00 的 Linux ARM64 原生验收未通过；P01 至 P05 已实现并接入本地运行时；P06 集成资源已打包，真实宿主安装态未验收；P07 已完成 macOS arm64 测试制品，其余四目标、正式签名和公开发布未完成。
- 产品决策：[本地架构调整方案](../2026-11-10_local-architecture-plan.md)。

## 1. 执行顺序与阅读方式

后续 Agent 先读架构方案、本索引和目标阶段文档，再核对前序阶段的实际交付。可执行路径、接口和命令以仓库实现、实施交接章节和本索引第 5 节为准。

| 阶段 | 文档 | 前置 | 独立交付结果 |
|---|---|---|---|
| P00 | [基线、契约与可行性](00-baseline-and-contracts.md) | 无 | 合成样本、分析对照口径、宿主与 Linux ARM64 可行性证据 |
| P01 | [Rust 乐谱分析](01-rust-score-analysis.md) | P00 | 无 UI、无数据库依赖的乐谱分析模块 |
| P02 | [原生引擎与渲染产物](02-native-engine-and-artifacts.md) | P01 | 可取消的转换／编译接口、产物清单和平台引擎构建配方 |
| P03 | [文档、编辑与版本](03-library-editing-and-history.md) | P02 | 公共源码、单文档编辑所有权、可恢复历史与提交一致性 |
| P04 | [运行时、HTTP、CLI 与 MCP](04-runtime-and-agent-interfaces.md) | P03 | 可独立启动的本地服务、会话视图和 Agent 工具闭环 |
| P05 | [侧边谱面与管理窗口](05-browser-score-and-desktop.md) | P04 | Agent 浏览器中的谱面交互和独立的 Tauri 管理窗口 |
| P06 | [Skill／MCP／插件注入](06-host-integrations.md) | P05 | 本体内置的 Codex／DSH 本地集成，支持安装、修复和移除；已纳入 P07 测试安装包，真实宿主安装态待验收 |
| P07 | [五目标发布](07-release.md) | P06 | macOS arm64 测试 app／DMG、发布材料与代码边界收敛；其余目标受阻 |

按 P00 → P07 顺序验收。前序未通过时，可用测试替身开发后续模块，但必须标记为未集成，不能将阶段标记为完成。缺少平台环境时写明缺失目标，不用“编译通过”替代真实运行验证。

P00 当前结果见[本机基线报告](00-baseline-report.md)；合成输入、TS 观测、正确期望与机器证据保留在 `tests/fixtures/local/`，日常检查由 Rust 测试直接读取。Linux ARM64 的原生构建和运行证据仍是 P00 的未通过门槛。P01 的 Rust 接口、本机验证与降级清单见[实施交接](01-rust-score-analysis.md#5-实施交接)。P02 原生引擎、缓存、五目标配方与未验证平台见[实施交接](02-native-engine-and-artifacts.md#5-实施交接)。P03 新库、编辑／版本事务、队列检查与恢复入口见[实施交接](03-library-editing-and-history.md#5-实施交接)。P04 运行时、HTTP、CLI／MCP、视图与任务入口见[实施交接](04-runtime-and-agent-interfaces.md#6-实施交接)。P05 的浏览器／管理窗口实现、Playwright 与原生闭环证据见[实施交接](05-browser-score-and-desktop.md#6-实施交接)。P06 的集成资源、宿主适配与未验证宿主行为见[实施交接](06-host-integrations.md#6-实施交接)。P07 的 macOS 制品和未验收目标见[实施交接](07-release.md#6-实施交接)。

## 2. 不再重新选择的约束

- 用户 Agent 发起任务；谱面交互位于 Agent 侧边浏览器，Tauri 自身窗口只做管理。
- Rust 统一核心，Vite + React SPA；没有服务端模型调用。Node.js 仅用于开发构建或宿主自身运行。
- 原生 LilyPond 2.26.0 和同版本转换器随本体交付，不引入 WASM、云端服务或用户自装工具链前提。
- 输入为单文件 `.ly`、MusicXML、MIDI；输出为 `.ly`、PDF、MIDI。用户多文件依赖、MusicXML 导出和音符级选择不在本次范围。
- 默认 `~/.ossia/`；项目内使用 `<项目根>/.ossia/`。Agent 直接修改该库中的公共 `documents/<documentId>/score.ly`。
- 一个文档只有一个编辑任务；文件变化自动预览，Agent 完成校验后显式提交，不增加人工接受候选步骤。
- 编译与版本一致性是提交门槛；MIDI／token 差异是诊断，不作为音乐意图正确性的证明。
- 仅发布本体包，由本体注入配置和薄插件；宿主侧不复制引擎。各宿主连接所选数据目录对应的同一运行实例。
- 五目标为 macOS arm64／x86_64、Windows x86_64、Linux arm64／x86_64。Windows ARM64、移动端不交付。
- 全部自有代码采用 GPL-3.0-or-later。运行时使用所选目录中的 `library.sqlite`。

## 3. 共享工程与接口约定

以下是分阶段实施的工程约定。模块边界和对外语义必须保持一致；内部函数命名和依赖版本可按仓库工具链落实并锁定。

### 3.1 工程布局与依赖方向

| 目标位置 | 责任 |
|---|---|
| `crates/ossia-types` | 序列化 DTO、标识符、错误码和协议版本；前端类型由此生成 |
| `crates/ossia-score` | 分词、索引、音高、MIDI、SVG 几何及小节定位 |
| `crates/ossia-engine` | 原生工具链、进程管理、快照编译与产物 |
| `crates/ossia-library` | SQLite、内容存储、公共文件、编辑状态和版本事务 |
| `crates/ossia-runtime` | 单实例、IPC、HTTP、任务调度、文件监听、视图与事件 |
| `crates/ossia-integrations` | 宿主检测、配置计划、注入记录、修复与移除 |
| `packages/desktop/src-tauri` | 唯一发布入口；GUI／serve／mcp／CLI 模式，在创建 Tauri 前分派 |
| `packages/ui` | 浏览器谱面 SPA；管理窗口使用独立入口，不加载谱面工作流 |
| `integrations/` | Skill、Codex 本地插件、DSH 预构建插件的源资源 |
| `tests/fixtures/local/` | 可提交、可重建的合成样本和参考结果 |

依赖方向为 `types ← score ← engine ← library ← runtime`，集成模块通过运行时客户端接入。纯分析模块不依赖 Tauri、数据库、网络或已启动的 GUI。根 Cargo workspace 纳入这些 crate 和既有桌面 crate，共用锁文件；最低 Rust 版本为 1.89（P03 的 bundled SQLite 依赖要求），本机验证使用 1.98.1；P01 建立工作区，后续阶段按需加入实际 crate，不预建空壳模块。

### 3.2 标识、位置和数据目录

- `DocumentId`、`VersionId`、`EditId`、`ViewId`、`JobId` 使用不同的 Rust 新类型，序列化为 UUID 字符串；内容哈希为完整小写 SHA-256。
- `sourceHash` 对源码原始 UTF-8 字节计算，保留 CRLF、BOM 和末尾换行；不因索引或预览重写用户源码。`.ly` 非 UTF-8 输入返回编码错误。
- `SourceSpan` 使用 UTF-8 字节范围 `[byteStart, byteEnd)`；对外行号从 1 开始，列从 1 开始按 Unicode 标量计数。LilyPond 原始列值单独保存，由 P00 的探针确定转换，不能把 UTF-16 下标直接当作字节位置。
- P00 的 2.26.0 实测表明，SVG 回链 `line:char:column` 中 char 为零基 Unicode 标量偏移，column／诊断列为一基、按 8 列展开 tab 的显示列。保留原始字段；优先通过 char 转换字节位置，不能将显示列直接视为源码列。探针输入与适用边界见[位置单位结论](00-baseline-report.md#3-位置单位结论)。
- `page`、全局 `measureStart/measureEnd` 从 1 开始；`staffIndex`、`systemIndex` 从 0 开始。页面坐标使用 SVG user units。
- 数据目录选择顺序为显式 `--home`、`OSSIA_HOME`、默认 `~/.ossia/`。项目开发脚本显式传入仓库 `.ossia/`，不根据任意进程 cwd 猜测项目根。创建后使用规范化绝对路径作为实例键。
- 测试只用临时目录；运行时只识别当前 `library.sqlite`，未知库标识或 schema 返回 `runtime-incompatible`，不读取、覆盖或自动清理其他文件。

### 3.3 最小公共类型

| 类型 | 必须表达的字段／语义 |
|---|---|
| `SourceSnapshot` | `documentId, sourceHash, bytes`；不可变，索引和编译读同一快照 |
| `DocumentState` | `documentId, title, sourcePath, headVersionId?, sourceHash, dirty, editState, lastSuccessfulRender?, lastCheck?` |
| `Version` | `versionId, documentId, seq, parentVersionId?, sourceHash, origin, author, summary, createdAt` |
| `Selection` | `viewId, documentId, sourceHash, locators[]`；每个 locator 为谱表／小节范围和可选声部 |
| `LocatedRegion` | locator、`SourceSpan[]`、定位能力 `measure/anchor-only/none` 和原因 |
| `RenderManifest` | `sourceHash, engineBuildId, optionsHash, pages[], pdf?, midi?, diagnostics, formatErrors[]`；格式独立返回结果，文件引用含相对 path、字节数和 SHA-256，区分缓存键与产物字节哈希 |
| `EditSession` | `editId, documentId, ownerViewId, baseVersionId?, status, startedAt, sourcePath, author`；编辑凭据不广播给其他视图 |
| `Job` | `jobId, documentId?, kind, state, sourceHash?, phase, result?, error?`；状态为 queued/running/done/error/cancelled/interrupted |
| `RuntimeInfo` | `runtimeVersion, protocolVersion, schemaVersion, instanceId, home, webBaseUrl, capabilities` |
| `AppError` | 稳定 `code`、可读 `message`、可选结构化 `detail`；HTTP 和 MCP 共用领域错误 |

初始协议主版本与新 SQLite schema 版本均为 1；域外调用不能直接操作数据库。HTTP 字段使用 camelCase。版本节点用 `versionId` 寻址，渲染用源码哈希寻址，不混用二者。

最小 JSON 示例见 `tests/fixtures/local/contract-examples.json`。它们表达共享契约，不代表生产接口已实现。单文件判定必须区分引擎标准库与用户依赖：本机外部 include 能成功编译不构成支持证据。Codex／DSH 的本机 loopback、视图隔离和事件流已实测；插件打开 API、Web sandbox 与下载限制的文档声明及未验证项见 P00 报告，不能据此跳过 P06 验收。

稳定错误集合至少包含 `invalid-request`、`invalid-input`、`invalid-encoding`、`not-found`、`conflict`、`edit-busy`、`dirty-workspace`、`stale-selection`、`compile-failed`、`unsupported-source`、`import-confirm-required`、`import-refused`、`cancelled`、`runtime-incompatible`、`storage-error`、`library-busy`、`unauthorized`、`forbidden`、`runtime-unavailable`。`storage-error` 表示库文件／SQLite／CAS／产物损坏或读写失败；`library-busy` 表示另一进程持有新库写入锁。`unauthorized`／`forbidden` 表示缺少有效凭据／越权；`runtime-unavailable` 表示服务停止中、发现失败或端口不可用。`invalid-input` 表示损坏的分析输入，`unsupported-source` 表示未支持的有效格式或结构；合法 `.ly` 的索引降级不等于编译失败。各阶段新增错误必须同步类型、文档和调用端处理。

## 4. 每阶段的交付纪律

1. 读取目标阶段、前序交接和实际代码，先核对是否已部分实现；不要只依赖文档状态重复开工。
2. 实现本阶段范围并运行列出的检查。跨阶段契约确需调整时，同时修改索引和所有受影响阶段文档，说明原因。
3. 在本阶段文档末尾更新“实施交接”：代码位置、实际变更、已执行命令与结果、未验证平台、具体阻塞、下一阶段入口。
4. 状态使用“待实施／进行中／受阻／已验收”，无实测证据不得写“已验收”。阶段完成不自动授权发布、清理其他数据或执行后续阶段。
5. 不将个人乐谱、密钥、旧用户数据库或本机临时产物提交为 fixture。不开新的产品方向，不补云端、WASM 或 Agent 会话适配。

文档检查、编译、单元测试和真实工具链验收分别记录。特别是 Linux ARM64、自包含工具链、宿主侧边浏览器和真实安装包，不得由测试替身代替验收。

## 5. 当前开发入口

运行时、UI 和集成资源分别位于 `crates/`、`packages/`、`integrations/`。`tests/fixtures/local/` 保存合成输入、规范化期望、TS 观测与机器证据。

- `pnpm dev:runtime`：显式项目 home、3212、已准备工具链和静态 dist；只使用当前 `library.sqlite`。
- `pnpm build:headless`：无 GUI 依赖的桌面可执行文件，支持 serve／mcp／doctor／stop 和 JSON CLI。
- `pnpm generate:types`／`pnpm check:types`、`pnpm generate:contracts`／`pnpm check:contracts`：生成／检查 TS 协议和 MCP schema。
- `pnpm dev:web`／`pnpm build:web`：启动／构建 Vite 双入口 SPA；`index.html` 是谱面，`admin.html` 是管理窗口。
- `pnpm dev:desktop`／`pnpm build:desktop`：启动／构建 Tauri 管理窗口；开发脚本向共享运行时配置显式传入项目 home、`3212`、工具链与静态资源。
- `pnpm test:ui`／`pnpm test:ui:native`：运行浏览器 Playwright 与真实引擎／Tauri 共存原生场景。
- `pnpm test:integrations`：集成计划的生成物与冲突回归；真实 Codex／DSH profile 的 round-trip 测试见 crate README 的显式环境变量命令。
- `pnpm package --target=darwin-arm64`：构建 macOS arm64 测试制品、发布材料和校验和；其他目标按引擎锁与 CI 条件处理。
- `pnpm engine:install`／`pnpm engine:verify`：准备／校验本机锁定工具链；五目标配方见 `vendor/README.md`。
- `cargo test -p ossia-library`：临时库、编辑／版本事务与崩溃恢复；原生闭环见 P03 交接。
- `pnpm test:engine`、`cargo test -p ossia-engine`：开发资源与引擎离线回归；原生集成测试显式启用。
- `pnpm typecheck`、`pnpm check`、`pnpm test:package`：源码检查与打包脚本回归。

P04 提供 headless 服务与工具闭环；P05 把普通浏览器谱面和 Tauri 管理窗口接入同一 Rust 服务，并验证两者与 headless 实例共存。谱面页面只连接当前运行时 API，不加载 Tauri bridge 或模型作业流程。P07 已产出 macOS arm64 测试包；默认 profile、真实安装态宿主注入和跨平台原生验收仍未完成。
P06 已提供 `ossia integrate` 与管理窗口入口；宿主注入配置只在临时 profile 实测。P07 已完成 macOS arm64 本体 app／DMG、资源校验和发布材料；Windows／Linux、macOS x86_64、Linux ARM64 引擎、真实安装态宿主侧栏、签名公证与公开发布仍未验收。
