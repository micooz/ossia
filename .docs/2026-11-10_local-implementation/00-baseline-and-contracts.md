# P00：基线、契约与可行性

- 状态：受阻；本机基线、原生引擎与 Codex／DSH Desktop 探针已交付，Linux ARM64 原生构建与运行尚未验收。
- 前置：无；开始前读[总索引与共享约定](README.md)及[架构方案](../2026-11-10_local-architecture-plan.md)。
- 后续：[P01 Rust 乐谱分析](01-rust-score-analysis.md)。

## 1. 阶段目标与范围

固定可重建的对照输入，确认跨阶段接口，并验证 Linux ARM64 引擎与宿主侧边页面接入。产出可运行的基线工具和证据。

根 Cargo workspace 包含 `ossia-types`、`ossia-score`、`ossia-engine`、`ossia-library`、`ossia-runtime`、`ossia-integrations` 和桌面 crate。`vendor/ENGINE.lock` 固定五个发布目标，Linux ARM64 仍为 unbuilt。

## 2. 现有实现

- `crates/ossia-score/`：lexer、索引、SVG 几何、音高和 MIDI 分析。
- `crates/ossia-engine/`：原生工具链、转换、编译和产物缓存。
- `scripts/engine.mjs`、`vendor/ENGINE.lock`：工具链安装和时间戳规则。
- `packages/ui/src/components/ScoreView.tsx`：页面坐标及选区覆盖层。
- `tests/fixtures/local/`：合成输入、TS 观测、正确期望和机器证据。

## 3. 实施任务

### 3.1 合成样本与对照导出

在 `tests/fixtures/local/` 建立清单，为每个样本写明来源、覆盖能力、应成功或应降级的结果。样本均自行构造，至少包括：

- 双谱表、多声部、跨页、带 `% N` 的转换器产物和压缩休止。
- 手写单文件，分别使用命名变量、内联 Staff、`\relative`、`\absolute`；含注释、Scheme／markup 字符串和标准库 include。
- 中文、emoji、CRLF、BOM；同一行包含两个小节的边界样本。
- `\chordmode`、无法可靠展开的 repeat／transpose 等降级样本。
- 有效 MusicXML、有效 MIDI、局部转换损失和阻断性 `Negative skip` 样本。
- 截断 MIDI、变长 meta event、running status、note-on 速度为零等二进制解析样本。

`tests/fixtures/local/observed.json` 保存参考 token、声部／小节范围、谱表顺序、几何、音高和 MIDI note-on；其中的 UTF-16 位置已转换为索引使用的 UTF-8 字节范围。

参考输出不是正确性证明。压缩休止的最大小节号、尾部 score block、未知 staff 等差异由独立期望和 LilyPond 输出核对。

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

基线报告保存命令、样本和结果指针；机器输出进入测试目录或报告附件，避免只有聊天里的成功断言。

## 4. 验收与交接

- [x] Rust 测试直接消费 fixture，换 cwd 后结果不漂移。
- [x] 不包含用户乐谱、数据库、凭证或随机临时路径。
- [x] Unicode／CRLF 位置探针有输入、原始输出和转换结论。
- [x] Linux ARM64 探针尚未运行，阶段受阻及具体原因已记录；该平台验收未通过。
- [x] Codex／DSH 接入证据与未验证环境分别列明。
- [x] 共享类型示例与总索引一致，所有命令均有实际脚本归属。

## 5. 实施交接

- 交付内容：25 个合成输入、22 份原生渲染参考、TS 观测、已知差异、共享 JSON 示例和基线报告。
- 回归：`cargo test -p ossia-score -p ossia-types` 和 `cargo test --workspace` 直接读取 `tests/fixtures/local/`，不重建 TS 观测。
- 工具链／宿主环境证据：[本机基线报告](00-baseline-report.md)，机器输出在 `tests/fixtures/local/evidence/`。macOS ARM64 原生 2.26.0，以及 Codex 26.1103.60933／DSH Desktop 0.2.2-rc.1 已实测。
- 阻塞及未验证项：没有启动原生 Linux ARM64 构建环境，缺同版本构建配方、依赖和转换／编译证据。其他原生目标、插件 API 实际调用、DSH Web、下载策略与安装包未验证；详见报告。
- P01 消费合成 fixture、位置转换结论和正确期望。
