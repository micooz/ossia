# P01：Rust 乐谱分析

- 状态：受阻（前序门槛）；Rust 分析模块已接入本地运行时；P00 Linux ARM64 原生验收仍未通过。
- 技术方案：[01-technical-plan.md](01-technical-plan.md)。
- 前置：[P00](00-baseline-and-contracts.md)；共享类型和目录约定见[总索引](README.md)。
- 后续：[P02 原生引擎与渲染产物](02-native-engine-and-artifacts.md)。

## 1. 阶段目标与边界

建立可独立测试的 Rust 乐谱分析模块，覆盖分词、索引、SVG 几何、谱表关联、音高和 MIDI 分析。模块不依赖数据库、HTTP、Tauri 或模型。

## 2. 工程与接口

- 建立根 Cargo workspace 和统一锁文件，将既有桌面 crate 纳入；创建实际需要的 `ossia-types` 与 `ossia-score`。
- 类型、标识符、位置单位遵守总索引。公共入口按能力划分为 `tokenize/serialize`、`build_index`、`build_layout/locate`、`resolve_pitches`、`parse_midi/diff_midi`。
- 这些入口接受内存中的源码／SVG／MIDI，返回值或结构化错误；不得隐式读取全局数据目录或运行编译器。
- 定位返回 `measure/anchor-only/none` 和原因；`unsupported` 与 `invalid-input` 分开。合法但无法索引的 `.ly` 不属于编译错误。

## 3. 实施任务

### 3.1 分词与位置

- 实现模式栈、命令参数处理、注释、字符串、Scheme、和弦与原始 token。
- 保留原始 UTF-8 字节及全部空白；未知输入退化为 RAW，满足 `serialize(tokenize(source)) == source`。
- 为源码建立行起点和 UTF-8／Unicode 位置转换；不得逐字节截断多字节字符。
- `.ly` 非 UTF-8 输入在文件接入层拒绝，分析模块不通过有损解码悄悄替换字符。

### 3.2 L1 与谱表关联

- 实现 musicxml2ly 和 midi2ly 的变量／context 声部扫描，保留 `% N` 对小节区间的权威关系。
- 小节最大号取范围终点，不以 span 数量替代；压缩休止跨度保持完整。
- 正确界定命名声部的表达式终点，不能将后续 `\score` 或其他声明并入最后一个声部。
- 增加普通单文件命名音乐变量和单个内联 Staff 的支持；无法确定声部／小节关系时返回降级状态。
- MIDI 导入的未加引号 context 名和复用 Staff 名必须保持正确映射。
- 显式 voice／staff 不存在时返回 no-hit／not-found，不回退到无关声部。

### 3.3 SVG 几何与全局小节

- 实现变换栈、线段／矩形／path bbox、谱表分组、系统小节线与 hit test。
- 在全部页面上建立统一全局小节网格；同一系统并行谱表的列只计一次。
- 页面不存在时返回 no-hit，不回退到第一页；空白、系统间距和版面外点击不产生假选区。
- 某页解析失败时返回显式部分能力，不能静默累加错误列号并挪动后续页的小节。
- 解析锚点作为源码关联证据；不将其扩展成首版音符级选择。

### 3.4 音高与 MIDI

- 实现 relative／absolute 的音高状态机、和弦参考音、休止不推进参考等规则。
- transpose、动态 Scheme 和其他超出已验证集合的结构显式返回 unsupported，不猜测音高。
- MIDI 解析使用边界检查，覆盖变长 meta 长度、running status、截断轨道和速度为零的 note-on；坏输入返回错误而非 panic。
- 差异描述包含轨道、note-on 数量、音高和起始时间变化，明确不覆盖所有时值、力度或演奏语义。

## 4. 测试与验收

- [x] `cargo test -p ossia-score` 通过；workspace 格式与静态检查通过。
- [x] P00 fixture 在 Rust 与规范化 TS 对照之间一致；被有意修正的旧错误有独立回归样本。
- [x] 任意文本无损往返，Unicode／emoji／CRLF 范围可反切出相同原文。
- [x] 转换器输出的上下谱表、多个声部、压缩休止和跨页定位正确。
- [x] 普通手写谱能选择小节；复杂语法不会输出伪精确源区间。
- [x] 随机短文本、损坏 MIDI、异常 SVG 输入不导致越界或 panic。

可以使用基于性质的测试验证无损性和边界安全；不要只断言函数调用后的自生成快照。实际音高规则仍须使用 P00 LilyPond 探针交叉检查。

## 5. 实施交接

### 模块与交付

- 根 Cargo workspace 的默认成员为 `ossia-types`／`ossia-score`；桌面 crate 同为工作区成员，不依赖分析核心。根锁文件统一管理依赖；桌面源码统一 rustfmt。
- 本机为 macOS ARM64；Rust／Cargo 1.98.1，edition 2021。新 crate 声明 GPL-3.0-or-later。锁定的直接依赖为 serde 1.0.229、serde_json 1.0.151、sha2 0.10.9、uuid 1.27.0、regex 1.13.1、quick-xml 0.42.0。未验证旧版 Rust 工具链。
- `ossia-types` 提供不可变 UTF-8 快照及反序列化哈希校验、UUID 新类型、位置／范围、locator、定位能力及稳定错误。`invalid-input` 区分损坏分析输入与 `unsupported-source`。运行时／版本业务 DTO 与前端类型由生成流程维护。
- `ossia-score` 接口见 crate README：`tokenize/serialize`、`SourceMap`、`build_index/resolve_locator`、`build_layout/locate`、`resolve_pitches`、`parse_midi/diff_midi`。`build_layout` 消费同一源码索引及显式页号的内存 SVG，`locate` 校验源码哈希。所有分析入口均不运行工具或访问数据目录。

### 对照与语法边界

- 合成输入和原生参考保持原始字节；`tests/fixtures/analysis/` 仅保留测试消费的规范化 token 和双谱表几何。规范化 token 全量对照，几何按容差对照，MIDI 与音高消费独立期望及原生数据；不将孤立 UTF-16 代理项解码为 Rust 字符串。
- 压缩休止最大号、尾部 score、quoted context 引用、复用 Staff、未知 staff／voice、无效页回退和截断 MIDI 由独立期望覆盖。独立断言覆盖的差异还包括：单谱表缺左侧小节线，以及覆盖整谱表的长符干。相同源码休止可对应多个展开小节，真正压缩显示则保留 2–5 等完整范围。
- 支持显式 `|`／转换器 `% N` 的平衡命名音乐表达式、单个内联 Staff、独立声部映射、多页、relative／absolute 嵌套与和弦参考；字节范围保留自己所属的编号注释，不吞后续声明。源码无小节边界、变量组合展开、同一表达式内并行音乐、重复／移调、动态音乐 Scheme、未知音乐命令等返回降级，不伪造精确范围。
- SVG 支持 translate／scale／matrix、线段／矩形／多边形及 M/L/H/V/C/S/Q/T/Z 路径。曲线控制点构成保守 bbox；未知元素／变换与坏 XML 保留不同结构化页错误。坏页或缺页切断顺序编号，仅由可靠源码锚点恢复；多谱表只计一次。几何失败时仍保留能解析的源码锚点。
- 音高还会拒绝文档级 `\language` 和未验证的音名 include，防止同一拼写被错误解释。MIDI 保留 PPQN／SMPTE 时间基准，严格检查事件和轨道终点；解析支持 SMF 0/1/2，比较仅支持格式 0/1 的 PPQN 时间线，按四分音符位置归一化。差异不覆盖 note-off、完整时值、力度、速度图或音乐意图。

### 检查与原生证据

- `cargo test -p ossia-score -p ossia-types`：24 项离线集成测试通过（另有 README 文档测试）；原生测试默认显式 ignored，不算作原生验收通过。
- `cargo test -p ossia-desktop`：既有 7 项通过。`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings` 通过。
- 原生检查：显式设置 `OSSIA_TEST_LILYPOND_ROOT` 指向仓库 2.26.0 macOS ARM64 工具链，运行 `cargo test -p ossia-engine --test score_native_crosscheck -- --ignored`；4 个合成用例覆盖 absolute／同音、嵌套状态恢复、中文 markup／和弦、真正压缩休止，MIDI 音高和 SVG 小节列交叉检查通过，源码字节保持不变。输入在 `tests/fixtures/inputs/pitch-layout/`，可重建证据在 Git 历史中的 `p01-native.json`；命令与可选证据输出参数见 crate README。
- `pnpm build:web` 构建 SPA；仓库不依赖 Next.js。
- 验证：`cargo test -p ossia-score -p ossia-types`、`cargo test --workspace`、`pnpm check` 和 `git diff --check`。

### 阻塞与 P02 入口

以上勾选表示 P01 本机代码检查完成，阶段没有标记“已验收”。Rust 分析已接入本地运行时和 macOS arm64 测试包；Linux ARM64、其他本体目标和真实宿主仍待验收。

P02 可通过 path dependency 消费两 crate。源码字节先用 `SourceSnapshot::new` 校验；诊断位置用 `SourceMap` 转换，固定快照编译后把同一源码索引与显式页号交给 `build_layout`，使用页级错误及能力报告部分结果。`resolve_pitches` 必须传完整音乐表达式的 `music_span`；MIDI 通过 `parse_midi/diff_midi` 分析。外部 include 判定、转换器、产物绑定、超时／取消与缓存由 P02 实现，不属于本模块。
