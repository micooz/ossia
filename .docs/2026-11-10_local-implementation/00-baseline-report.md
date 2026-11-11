# P00 本机基线报告

- 日期：2026-11-10。
- 环境：macOS arm64，Darwin 27.2.0；Node 22.22.0，pnpm 12.4.1；原生 LilyPond 2.26.0。
- 状态：Linux ARM64 原生构建与执行未验收；macOS arm64 证据可用。

## 1. 交付与复现

[技术方案](00-technical-plan.md)已落盘；当前合成输入和测试基线见 [fixture 说明](../../tests/fixtures/README.md)；原始迁移验证记录见 Git 历史。

| 命令 | 覆盖 |
|---|---|
| `cargo test -p ossia-score -p ossia-types` | fixture 解码、位置范围、几何、音高和 MIDI 约定 |
| `cargo test --workspace` | 全部 Rust crate、契约和原生入口 |
| `pnpm check` | TypeScript 与仓库检查 |

Rust 测试直接读取已提交的输入、分析与渲染基线；目录和更新规则见 `tests/fixtures/README.md`。

输入清单为 `tests/fixtures/inputs/manifest.json`；测试消费的规范化 token 和几何期望在 `tests/fixtures/analysis/`，正确行为的独立断言位于 Rust 测试。其余历史 TS 观测、差异清单与日志见 Git 历史。引擎生成的 SVG 回链只替换 scratch 绝对路径，保留位置尾部。PDF 成功生成并记录大小和字节哈希，但不要求重复运行字节一致。

## 2. 原生引擎证据

机器记录：Git 历史中的 `engine.json`，包括工具版本、二进制架构、ENGINE.lock、每次命令、退出码、诊断和产物清单。`bin/lilypond` 实际为 Mach-O 64-bit arm64。

- `.ly`、MusicXML、MIDI 三类输入均经过原生执行；SVG、PDF、MIDI 产物可用。ChordNames-only 样本合法但不要求 MIDI。
- MusicXML 正常输入得到 clean；无效 metronome 文本触发 `Invalid bpm` 和 needs-confirmation；同声部重叠输入实际触发 `Negative skip` 和 refused。被拒绝源码仍可编译，证明“编译成功”不能替代导入时间线检查。
- `polyphonic-pages` 实际生成两页、每页两个谱表；全局小节为 1–4，平行谱表没有重复计数。
- `pitch-state` 的 relative 和弦／休止状态机与原生 MIDI 一致：note-on 为 60、64、67、65、67、60，tick 为 0、384、384、1152、1536、2304。
- 从独立临时 cwd 编译 `english.ly` 标准库 include 成功；外部 `user-music.ily` 缺失时失败，临时补齐时成功。外部文件恰好存在只能证明引擎能读取，不能将其算成单文件支持；P02 仍须显式拒绝用户依赖。

所有直接 `.ly` 编译保持原始字节，通过 `-dpoint-and-click=#t` 启用回链，不插入源码。转换器的原始输出可从 Git 历史查询；派生 render fixture 开启 anchors／MIDI 后以自身源码哈希和位置为基准，不把其锚点冒充原转换结果的位置。

## 3. 位置单位结论

输入与 SVG 位置尾部保存在 `tests/fixtures/rendered/columns-*/`；原始诊断探针和机器日志见 Git 历史。以下均为同一行 `c4` 的起点：

| 样本 | UTF-8 字节列，零基 | UTF-16 列，零基 | Unicode 标量列，零基 | 展开 tab 后显示列，零基 | 原始回链尾部 |
|---|---:|---:|---:|---:|---|
| columns-plain | 55 | 49 | 48 | 48 | `:2:48:49` |
| columns-lf（一个 tab） | 55 | 49 | 48 | 49 | `:2:48:50` |
| columns-bom-crlf（两个 tab） | 56 | 50 | 49 | 57 | `:2:49:58` |

在这些 2.26.0 探针中，回链 `line:char:column` 的 line 从 1 开始；char 是行内 Unicode 标量偏移，从 0 开始；column 是 tab 按 8 列展开后的显示列，从 1 开始。未知命令诊断使用相同显示列（49／50／58），不是 UTF-8 字节列或 UTF-16 列。BOM、CRLF 不被索引层改写。

P01 应用 char 定位原始行中的标量边界，再计算 UTF-8 字节位置；P02 保留原始诊断位置，对显示列按源码反算。无法唯一反算时返回空位置，不能直接将显示列当字节偏移。上述探针未覆盖组合字符、宽字符终端宽度或所有 LilyPond 诊断类别，不据此扩大结论。

裸 emoji 在参考 lexer 中会被拆成两个 UTF-16 代理 token。Rust 规范化将相邻同模式 RAW 代理对合并为一个 UTF-8 span；所有规范化 token 都经过字节反切断言。

## 4. 宿主实测与文档边界

机器证据见 Git 历史中的 `host-session.json`、`host-validation.json` 和 `codex.png`。服务只监听 loopback，页面短期凭据交换为内存令牌；同源请求和 fetch SSE 均不使用 Cookie。

| 宿主 | 实测环境 | 观察 |
|---|---|---|
| Codex desktop／ChatGPT app | 26.1103.60933，build 25894，macOS arm64 | 内置浏览器打开 loopback；view 1 选择小节 2，事件流重连后保持 |
| DeepSeek Harness Desktop | 0.2.2-rc.1，macOS arm64 | 通过右侧 Browser 打开 loopback；view 2 初始为空，选择小节 3并重连，Codex 仍保持小节 2 |

自动检查另外验证过期／重放启动凭据、过期访问令牌、错误 Origin、跨 view 访问拒绝、无 Cookie 及事件证据无令牌。不修改宿主配置，不安装插件。

[OpenAI Browser 文档](https://learn.chatgpt.com/docs/browser)说明内置浏览器支持本地 Web 应用；[DSH Sidebar Browser 文档](https://github.com/deepseek-ai/deepseek-harness/blob/master/packages/client/ui-sidebar-browser/README.md)给出 `ctx.sidebarRight.openTab('browser', { params: { url } })`。DSH Web 尚未安装 client plugin 调用该 API。

DSH 文档规定 Web 默认 iframe sandbox 不含 allow-downloads，Desktop guest 禁止下载；这些是文档声明，未实测下载拒绝。Codex 下载默认路径也仅作文档记录。DSH Web、Windows／Linux 宿主、插件自动打开页面、完整谱面 UI 或安装包行为未验证。

## 5. P01 交接与阻塞

不能机械复制 TS 输出；现有回归断言分别覆盖：压缩休止最大号错误、尾部 score 被算入声部、未知 staff 回退其他声部、无效 page 回退第一页、context ID 被误当变量名、普通命名／absolute 音乐未索引，以及截断 MIDI 被静默接受，均有合成输入与独立期望。

共享最小 JSON 示例在 `tests/fixtures/contracts/examples.json`，源码字节与 SVG 哈希由自动检查交叉验证。

Linux ARM64 未运行：本机为 macOS ARM64，Docker 的 OrbStack endpoint 不可连接；没有启动 OrbStack／UTM，也没有源码构建。仍缺 2.26.0 同版本源码和固定依赖的构建配方、真实 ARM64 Linux 二进制，以及转换器和 SVG／PDF／MIDI 运行证据。该平台需要原生环境补齐，不能以本机探针或模拟器替代。
