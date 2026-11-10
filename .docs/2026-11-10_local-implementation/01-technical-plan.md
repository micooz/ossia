# P01 技术实现方案：Rust 乐谱分析

- 基准：`1f9974c`。
- 实施状态：代码与本机验证完成，详见 P01 阶段文档的实施交接；前序门槛未解除。
- 交付：独立 Rust 分析模块、对照测试和 P02 交接。
- 前序门槛：P00 Linux ARM64 未通过；本轮仅完成未集成开发，不标记已验收。

## 工程与接口

建立根 Cargo workspace，纳入既有桌面 crate，以及实际需要的 `ossia-types`、`ossia-score`；默认成员为两个核心 crate，迁移现有锁文件。新增 crate 使用 GPL-3.0-or-later。

`ossia-types` 提供 UUID 新类型、SHA-256、不可变源码快照、UTF-8 字节范围、locator、定位能力和领域错误，JSON 使用 camelCase。分析接口为 `tokenize/serialize`、`build_index`、`build_layout/locate`、`resolve_pitches`、`parse_midi/diff_midi`。入口只消费内存，不读取全局目录、运行编译器或依赖 GUI、数据库、网络。

## 实现约束

- 分词无损，未知字符按 Unicode 标量退化 RAW，保留 BOM、CRLF 和空白；字节范围可反切原文。位置使用一基 Unicode 标量行列，LilyPond 锚点保留原字段并优先使用零基 char。
- 平衡结构界定表达式终点；支持转换器、普通命名变量、单个内联 Staff，解析真实声部引用和复用 Staff。转换器 `% N` 与显式 `|` 确定小节，不做省略边界的时值推导；压缩休止保留完整跨度，未知 staff/voice 不回退。
- XML 解析 SVG，迁移变换、bbox、谱表、小节线和 hit test。多页全局小节只计一次并行谱表；空白、系统间距及版面外无命中。页面失败后停止无依据累加，仅可由可靠源码锚点恢复编号，否则降级。
- relative/absolute 音高状态机保留和弦首音参考、同音去重及休止规则；transpose、repeat、动态音乐 Scheme 等返回 unsupported。
- MIDI 有轨道边界、VLQ、running status 和事件校验；零速度 note-on 不计发声，保留时间基准。差异只解释轨道、note-on 数量、音高与起始时间，不证明完整演奏或音乐意图。

## 验证与交接

Rust 测试消费现有合成输入、规范化 TS 观测和独立期望，已知旧错误分别回归。覆盖 Unicode、同一行多小节、压缩休止、尾部 score、谱表复用、多声部、跨页、无效页面和空白命中；随机文本、深层结构、坏 SVG/MIDI 不 panic。

以 P00 原生 MIDI 交叉核对音高，补测 absolute 与嵌套恢复。执行核心测试、workspace 格式与 Clippy、桌面回归和既有 pnpm 检查，记录既存问题。更新索引和 P01 交接，注明支持/降级语法、命令结果、未验证平台及 P02 入口。P01 分析实现止于本阶段；用户随后授权提前移除方案外后端，因此旧 Next.js／模型／HTTP 实现已删除，P00 纯分析对照迁入开发参考目录。产品运行时与页面接入仍留给 P04／P05，用户数据处理留给 P07。
