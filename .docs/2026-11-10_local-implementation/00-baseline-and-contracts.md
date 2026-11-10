# P00：基线、契约与可行性

- 状态：受阻；本机基线、原生引擎与 Codex／DSH Desktop 探针已交付，Linux ARM64 原生构建与运行尚未验收。
- 前置：无；开始前读[总索引与共享约定](README.md)及[架构方案](../2026-11-10_local-architecture-plan.md)。
- 后续：[P01 Rust 乐谱分析](01-rust-score-analysis.md)。

## 1. 阶段目标与范围

为 Rust 迁移固定可重建的对照输入，确认跨阶段接口，并尽早验证 Linux ARM64 引擎与宿主侧边页面接入。产出可运行的基线工具和证据，不更改现有产品路径，不清理旧数据。

当前仓库没有根 Cargo workspace。后端为 Next.js，分析库为 TypeScript，Tauri 只暴露 `get_endpoint`；`vendor/ENGINE.lock` 只锁定 darwin-arm64。不能把历史文档中的已删除 verify 脚本当作现存测试。

## 2. 先读的现有实现

- `packages/ly/src/`：lexer、L1、layout、pitch、SMF。
- `packages/backend/src/server/`：locate、layout-join、render、engine、import-gate、store、score。
- `scripts/engine.mjs`、`vendor/ENGINE.lock`：当前安装和时间戳规则。
- `packages/ui/src/components/ScoreView.tsx`：页面坐标及选区覆盖层。
- 旧 ADR-0002 只作解析不变量参考；整份源码交给模型、JSON 行替换、五道门和源码不下发不再是目标架构要求。

## 3. 实施任务

### 3.1 合成样本与对照导出

在 `tests/fixtures/local/` 建立清单，为每个样本写明来源、覆盖能力、应成功或应降级的结果。样本均自行构造，至少包括：

- 双谱表、多声部、跨页、带 `% N` 的转换器产物和压缩休止。
- 手写单文件，分别使用命名变量、内联 Staff、`\relative`、`\absolute`；含注释、Scheme／markup 字符串和标准库 include。
- 中文、emoji、CRLF、BOM；同一行包含两个小节的边界样本。
- `\chordmode`、无法可靠展开的 repeat／transpose 等降级样本。
- 有效 MusicXML、有效 MIDI、局部转换损失和阻断性 `Negative skip` 样本。
- 截断 MIDI、变长 meta event、running status、note-on 速度为零等二进制解析样本。

新增开发用基线导出命令，调用当前 TS 库生成规范化 JSON：token 原文与位置、声部／小节范围、谱表顺序、几何、音高和 MIDI note-on。TS 的 UTF-16 位置转换成索引约定的 UTF-8 字节范围后再比较。

基线不是“旧代码永远正确”的证明。尤其核对压缩休止的最大小节号、尾部 score block 是否误入声部范围、未知 staff 是否误落到另一声部。发现旧实现错误时，用 LilyPond 输出或手工可核验样本确定期望，并在样本说明中记录差异。

### 3.2 原生引擎探针

- 在临时目录验证 2.26.0 的转换、SVG、PDF、MIDI 和原始诊断列值；用含中文／emoji 的源码测出字节、字符、UTF-16 和 anchor 列的关系。
- 验证启用 point-and-click 后，SVG 回链仍能映射到未改写的原始源码；额外注入导致行列偏移时必须有明确的 source map。
- 验证标准库 include 在脱离仓库 cwd 后可用，外部用户依赖不能因本机文件恰好存在而被算作单文件支持。
- Linux ARM64 使用同版本上游源码和依赖建立原生构建探针，至少实际运行转换器和三种产物编译。输出依赖、构建步骤、二进制架构、运行环境与结果。

Linux ARM64 探针失败时记录具体编译／运行错误和下一步；不以 x86_64 模拟结果或换用系统任意版本替代既定交付。

### 3.3 宿主接入探针

使用独立临时 HTTP 页面验证，不改现有产品 SPA 或用户宿主配置：

- Codex 内置浏览器可打开 loopback 页面；Agent 可持有一个 `viewId`，用户点击后由本机接口读取同一视图的选择。
- DSH 对应配置下可打开 Sidebar Browser；记录插件打开页接口、默认 sandbox 和保存下载限制。
- 验证页面使用短期启动凭据、同源请求和事件流时不依赖跨站 cookie。

只能验证某个宿主／平台时明确记录版本和环境。无法在当前环境完成的宿主验收保留为 P06 的真实验证门槛，不虚构通过结果。

### 3.4 固定交付契约

将探针发现反映到总索引：位置单位、标准库依赖判定、宿主能力。为 SourceSnapshot、RenderManifest、版本节点和编辑状态写出最小 JSON 示例。例子使用合成 ID 和临时路径。

新增 `.docs` 中的基线报告，保存命令、样本和结果指针；机器输出进入测试目录或报告附件，避免只有聊天里的成功断言。

## 4. 验收与交接

- [x] 一个命令能重建全部 TS 对照结果，换 cwd 后结果不漂移。
- [x] 不包含用户乐谱、旧数据库、凭证或随机临时路径。
- [x] Unicode／CRLF 位置探针有输入、原始输出和转换结论。
- [x] Linux ARM64 探针尚未运行，阶段受阻及具体原因已记录；该平台验收未通过。
- [x] Codex／DSH 接入证据与未验证环境分别列明。
- [x] 共享类型示例与总索引一致，所有命令均有实际脚本归属。

本阶段新增命令统一以 `pnpm baseline:local` 为基线导出入口，独立探针置于开发脚本中。脚本尚未实现前不可记录命令通过。

## 5. 实施交接

- 代码基准与实际变更：基于 `3f486c5`，新增 `scripts/local/`、25 个合成输入、22 份原生渲染参考、TS 观测、已知差异、共享 JSON 示例和报告；增加根开发命令。未修改产品路径。
- 检查命令与结果：`pnpm probe:engine`、`pnpm baseline:local --check`、`pnpm test:local`（8 项）、`pnpm typecheck` 通过；`pnpm check` 为 0 errors、5 个未修改后端代码中的既有 warning。
- 工具链／宿主环境证据：[本机基线报告](00-baseline-report.md)，机器输出在 `tests/fixtures/local/evidence/`。macOS ARM64 原生 2.26.0，以及 Codex 26.1103.60933／DSH Desktop 0.2.2-rc.1 已实测。
- 阻塞及未验证项：没有启动原生 Linux ARM64 构建环境，缺同版本构建配方、依赖和转换／编译证据。其他原生目标、插件 API 实际调用、DSH Web、下载策略与安装包未验证；详见报告。
- 下一阶段：P01 消费合成 fixture、位置转换结论和正确期望。前序门槛未通过前只能作为未集成开发，不能标记已验收。
