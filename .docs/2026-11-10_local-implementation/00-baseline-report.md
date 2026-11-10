# P00 本机基线报告

- 日期：2026-11-10。
- 代码基准：`3f486c5`。
- 环境：macOS arm64，Darwin 27.2.0；Node 22.22.0，pnpm 12.4.1；原生 LilyPond 2.26.0。
- 状态：本机交付完成，P00 整体受阻；Linux ARM64 原生构建与执行未验收。

> P07 更新：本文记录的 `pnpm baseline:*`、`probe:*` 和 `test:local` 命令属于 P00 历史证据；对应 TypeScript 对照脚本和 `packages/ly` 已在 P07 移除。合成输入、历史观测和证据仍保留在 `tests/fixtures/local/`，当前回归由 Rust 测试直接读取这些文件。本文实测值不回填。

## 1. 交付与复现

[技术方案](00-technical-plan.md)已落盘。脚本位于 `scripts/local/`；合成输入、规范化输出、差异及机器证据位于 [tests/fixtures/local](../../tests/fixtures/local/)（相对仓库根路径；具体入口见下表）。

| 命令 | 实测结果 |
|---|---|
| `pnpm baseline:fixtures` | 生成 25 个原始合成输入，覆盖清单中的文本、MusicXML 和 MIDI 场景 |
| `pnpm probe:engine` | macOS ARM64 原生通过；22 份源码渲染成功，另有预期失败和诊断探针 |
| `pnpm baseline:local` | 重建 TS 观测 JSON；不读写现有 `.ossia/` |
| `pnpm baseline:local --check` | 参考一致；换临时 cwd 仍一致，故意修改临时参考可检测漂移 |
| `pnpm test:local` | 8 项通过，包括真实引擎 MIDI、跨页几何、Unicode、共享示例、凭据与事件流 |
| `pnpm typecheck` | 现有包及新增本地脚本全部通过 |
| `pnpm check` | 0 errors；5 个既有 warning，位于未修改的 `agent.ts` 和 documents route |

清单为 `tests/fixtures/local/manifest.json`；TS 观测为 `observed.json`；正确期望和已知差异分别在清单与 `known-differences.json`。输入可通过脚本重建；引擎生成的 SVG 回链只替换 scratch 绝对路径，保留位置尾部。PDF 成功生成并记录大小和字节哈希，但不要求重复运行字节一致。

## 2. 原生引擎证据

机器记录：`tests/fixtures/local/evidence/engine.json`，包括工具版本、二进制架构、ENGINE.lock、每次命令、退出码、诊断和产物清单。`bin/lilypond` 实际为 Mach-O 64-bit arm64。

- `.ly`、MusicXML、MIDI 三类输入均经过原生执行；SVG、PDF、MIDI 产物可用。ChordNames-only 样本合法但不要求 MIDI。
- MusicXML 正常输入得到 clean；无效 metronome 文本触发 `Invalid bpm` 和 needs-confirmation；同声部重叠输入实际触发 `Negative skip` 和 refused。被拒绝源码仍可编译，证明“编译成功”不能替代导入时间线检查。
- `polyphonic-pages` 实际生成两页、每页两个谱表；全局小节为 1–4，平行谱表没有重复计数。
- `pitch-state` 的 relative 和弦／休止状态机与原生 MIDI 一致：note-on 为 60、64、67、65、67、60，tick 为 0、384、384、1152、1536、2304。
- 从独立临时 cwd 编译 `english.ly` 标准库 include 成功；外部 `user-music.ily` 缺失时失败，临时补齐时成功。外部文件恰好存在只能证明引擎能读取，不能将其算成单文件支持；P02 仍须显式拒绝用户依赖。

所有直接 `.ly` 编译保持原始字节，通过 `-dpoint-and-click=#t` 启用回链，不插入源码。转换器的原始输出另存；派生 render fixture 开启 anchors／MIDI 后以自身源码哈希和位置为基准，不把其锚点冒充原转换结果的位置。

## 3. 位置单位结论

输入与原始位置尾部保存在 `engine/columns-*/score.ly`、`engine/*-diagnostic.ly` 和机器日志中。以下均为同一行 `c4` 的起点：

| 样本 | UTF-8 字节列，零基 | UTF-16 列，零基 | Unicode 标量列，零基 | 展开 tab 后显示列，零基 | 原始回链尾部 |
|---|---:|---:|---:|---:|---|
| columns-plain | 55 | 49 | 48 | 48 | `:2:48:49` |
| columns-lf（一个 tab） | 55 | 49 | 48 | 49 | `:2:48:50` |
| columns-bom-crlf（两个 tab） | 56 | 50 | 49 | 57 | `:2:49:58` |

在这些 2.26.0 探针中，回链 `line:char:column` 的 line 从 1 开始；char 是行内 Unicode 标量偏移，从 0 开始；column 是 tab 按 8 列展开后的显示列，从 1 开始。未知命令诊断使用相同显示列（49／50／58），不是 UTF-8 字节列或 UTF-16 列。BOM、CRLF 不被索引层改写。

P01 应用 char 定位原始行中的标量边界，再计算 UTF-8 字节位置；P02 保留原始诊断位置，对显示列按源码反算。无法唯一反算时返回空位置，不能直接将显示列当字节偏移。上述探针未覆盖组合字符、宽字符终端宽度或所有 LilyPond 诊断类别，不据此扩大结论。

裸 emoji 暴露旧 lexer 将 UTF-16 代理对拆成两个 RAW token 的行为。基线保留原始观测，另将相邻同模式 RAW 代理对合并为一个 UTF-8 span；所有规范化 token 都经过字节反切断言。

## 4. 宿主实测与文档边界

机器证据为 `tests/fixtures/local/evidence/host-session.json`、`host-validation.json` 和 `codex.png`。服务只监听 loopback，页面短期凭据交换为内存令牌；同源请求和 fetch SSE 均不使用 Cookie。

| 宿主 | 实测环境 | 观察 |
|---|---|---|
| Codex desktop／ChatGPT app | 26.1103.60933，build 25894，macOS arm64 | 内置浏览器打开 loopback；view 1 选择小节 2，事件流重连后保持 |
| DeepSeek Harness Desktop | 0.2.2-rc.1，macOS arm64 | 通过右侧 Browser 打开 loopback；view 2 初始为空，选择小节 3并重连，Codex 仍保持小节 2 |

自动检查另外验证过期／重放启动凭据、过期访问令牌、错误 Origin、跨 view 访问拒绝、无 Cookie 及事件证据无令牌。不修改宿主配置，不安装插件。

[OpenAI Browser 文档](https://learn.chatgpt.com/docs/browser)说明内置浏览器支持本地 Web 应用；[DSH Sidebar Browser 文档](https://github.com/deepseek-ai/deepseek-harness/blob/master/packages/client/ui-sidebar-browser/README.md)给出 `ctx.sidebarRight.openTab('browser', { params: { url } })`。后者只作接口依据，本轮未安装 client plugin 调用该 API。

DSH 文档规定 Web 默认 iframe sandbox 不含 allow-downloads，Desktop guest 禁止下载；这些是文档声明，未实测下载拒绝。Codex 下载默认路径也仅作文档记录。本轮没有验证 DSH Web、Windows／Linux 宿主、插件自动打开页面、完整谱面 UI 或安装包行为；相应真实门槛留给 P05／P06／P07。

## 5. P01 交接与阻塞

优先阅读 `known-differences.json`，不能机械复制 TS 输出：压缩休止最大号错误、尾部 score 被算入声部、未知 staff 回退其他声部、无效 page 回退第一页、context ID 被误当变量名、普通命名／absolute 音乐未索引，以及截断 MIDI 被静默接受，均有合成输入与独立期望。

共享最小 JSON 示例在 `tests/fixtures/local/contract-examples.json`，源码字节与 SVG 哈希由自动检查交叉验证；它们尚不是生产 DTO。本阶段没有建立 Rust workspace，也没有修改现有产品分析／存储路径。

Linux ARM64 未运行：本机为 macOS ARM64，Docker 的 OrbStack endpoint 不可连接；按本轮范围没有启动 OrbStack／UTM，也未尝试源码构建。仍缺 2.26.0 同版本源码和固定依赖的构建配方、真实 ARM64 Linux 二进制，以及转换器和 SVG／PDF／MIDI 运行证据。没有可引用的编译失败日志，因为尚未开始构建。下一步由有原生 Linux ARM64 环境的实施者补齐；不得以本机探针或模拟器替代。

P01 可以读取这些基线开展未集成开发；P00 的 Linux 门槛通过之前，不将后续阶段标记已集成或已验收。
