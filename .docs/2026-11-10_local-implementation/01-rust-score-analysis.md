# P01：Rust 乐谱分析

- 状态：待实施。
- 前置：[P00](00-baseline-and-contracts.md)；共享类型和目录约定见[总索引](README.md)。
- 后续：[P02 原生引擎与渲染产物](02-native-engine-and-artifacts.md)。

## 1. 阶段目标与边界

建立可独立测试的 Rust 乐谱分析模块，迁移 TS 分词、L1、SVG 几何、谱表关联、音高和 MIDI 分析。此阶段不接数据库、HTTP、Tauri 或模型，不删除旧 TS 实现；后者仍作为对照工具使用。

## 2. 工程与接口

- 建立根 Cargo workspace 和统一锁文件，将既有桌面 crate 纳入；创建实际需要的 `ossia-types` 与 `ossia-score`。
- 类型、标识符、位置单位遵守总索引。公共入口按能力划分为 `tokenize/serialize`、`build_index`、`build_layout/locate`、`resolve_pitches`、`parse_midi/diff_midi`。
- 这些入口接受内存中的源码／SVG／MIDI，返回值或结构化错误；不得隐式读取全局数据目录或运行编译器。
- 定位返回 `measure/anchor-only/none` 和原因；`unsupported` 与 `invalid-input` 分开。合法但无法索引的 `.ly` 不属于编译错误。

## 3. 实施任务

### 3.1 分词与位置

- 对照 `lexer.ts` 迁移模式栈、命令参数处理、注释、字符串、Scheme、和弦与原始 token。
- 保留原始 UTF-8 字节及全部空白；未知输入退化为 RAW，满足 `serialize(tokenize(source)) == source`。
- 为源码建立行起点和 UTF-8／Unicode 位置转换；不得逐字节截断多字节字符。
- `.ly` 非 UTF-8 输入在文件接入层拒绝，分析模块不通过有损解码悄悄替换字符。

### 3.2 L1 与谱表关联

- 迁移 musicxml2ly 和 midi2ly 的变量／context 声部扫描，保留 `% N` 对小节区间的权威关系。
- 小节最大号取范围终点，不以 span 数量替代；压缩休止跨度保持完整。
- 正确界定命名声部的表达式终点，不能将后续 `\score` 或其他声明并入最后一个声部。
- 增加普通单文件命名音乐变量和单个内联 Staff 的支持；无法确定声部／小节关系时返回降级状态。
- MIDI 导入的未加引号 context 名和复用 Staff 名必须保持正确映射。
- 显式 voice／staff 不存在时返回 no-hit／not-found，不回退到无关声部。

### 3.3 SVG 几何与全局小节

- 迁移变换栈、线段／矩形／path bbox、谱表分组、系统小节线与 hit test。
- 在全部页面上建立统一全局小节网格；同一系统并行谱表的列只计一次。
- 页面不存在时返回 no-hit，不回退到第一页；空白、系统间距和版面外点击不产生假选区。
- 某页解析失败时返回显式部分能力，不能静默累加错误列号并挪动后续页的小节。
- 解析锚点作为源码关联证据；不将其扩展成首版音符级选择。

### 3.4 音高与 MIDI

- 迁移 relative／absolute 的音高状态机、和弦参考音、休止不推进参考等已验证规则。
- transpose、动态 Scheme 和其他超出已验证集合的结构显式返回 unsupported，不猜测音高。
- MIDI 解析使用边界检查，覆盖变长 meta 长度、running status、截断轨道和速度为零的 note-on；坏输入返回错误而非 panic。
- 差异描述包含轨道、note-on 数量、音高和起始时间变化，明确不覆盖所有时值、力度或演奏语义。

## 4. 测试与验收

- [ ] `cargo test -p ossia-score` 通过；workspace 格式与静态检查通过。
- [ ] P00 fixture 在 Rust 与规范化 TS 对照之间一致；被有意修正的旧错误有独立回归样本。
- [ ] 任意文本无损往返，Unicode／emoji／CRLF 范围可反切出相同原文。
- [ ] 转换器输出的上下谱表、多个声部、压缩休止和跨页定位正确。
- [ ] 普通手写谱能选择小节；复杂语法不会输出伪精确源区间。
- [ ] 随机短文本、损坏 MIDI、异常 SVG 输入不导致越界或 panic。

可以使用基于性质的测试验证无损性和边界安全；不要只断言函数调用后的自生成快照。实际音高规则仍须使用 P00 LilyPond 探针交叉检查。

## 5. 实施交接（实施后填写）

- 代码基准、Rust 工具链和已锁定依赖：未实施。
- 对照结果与修正差异：未建立。
- 检查命令与结果：未执行。
- 已支持／降级语法清单：由实施者填写。
- 下一阶段：P02 使用类型、源码位置转换、MIDI 解析与 SVG 定位模块。
