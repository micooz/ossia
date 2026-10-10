# 备选方案评估记录（ADR-0001 附件）

- 状态：Evidence（不记录决策，仅保留评估结论、测量数据与复现方法）
- 日期：2026-10-08
- 主文档：[2026-10-08_tech-stack.md](./2026-10-08_tech-stack.md)（本文一律以 **ADR** 指代其条款）
- 下游规范：[2026-10-08_ly-parsing-indexing.md](./2026-10-08_ly-parsing-indexing.md)（**ADR-0002**）

本文档保存 ADR-0001 定稿前评估过但未采用的方案，以及支撑规范条款的原始测量数据。
基准谱：`宇宙の方舟 - ASTLIBRA.musicxml`（32 小节、697 note、665 fingering、291 KB）及配套 MIDI。
测量环境：Node 22 / Python 3.13 / LilyPond 2.26.0 darwin-arm64，2026-10。

本文 §3.1–§3.7 的测量在 WASM 引擎阶段完成，保留作为「为何弃用 WASM」的证据；凡涉及引擎性能与
缺陷复验，以 §3.12（缺陷复验）与 §3.16（产物成本）的原生二进制数据为准。
§3.13–§3.16 为 Rev.4（2026-10-08 后续轮）新增。

## 1. 真源格式候选（未采用）

| 候选 | 未采用理由 |
|---|---|
| MusicXML 为真源且 LLM 直接读写 | 整谱 ≈47k tokens；深层 XML 结构编辑易产出非法文本；原谱 `xml:id` 计数为 0，无稳定寻址基础 |
| MEI 为真源 | Verovio 6.3.0 导出整谱 MEI ≈168 KB / ~30k tokens，仍超 G3 预算；music21 10.5.0 报 `MEI export is not yet implemented`；发布生态互操作面窄 |
| ABC 为真源或 LLM 中间表示 | music21 对多声部钢琴谱 `write('abc')` 产出 36 字节无效对象 repr；ABC 无承接 665 个 per-note 指法的稳定位置 |
| 私有 JSON IR 为真源 | 技术成立，但需自建 id 铸造、编辑、投影、导出全套；`.ly` 以现成引擎 + 源码回链覆盖同等需求，成本更低 |

## 2. 渲染/编辑引擎候选（未采用）

| 候选 | 未采用理由 |
|---|---|
| Verovio 6.3.0（npm，LGPL-3.0，WASM 27 MB） | MusicXML 导入官方自认「有限支持、开发中」，MEI 加载遇不支持元素连子节点丢弃；绑定无 MusicXML 导出（`dir(toolkit)` 实测无 `getMusicXML`）；pip 绑定 `setOptions` 对任意 JSON 输入均返回 `False`；LGPL 进客户端分发需额外论证 |
| abcjs 6.7.1（MIT） | 官方 Purpose 文档明确不提供视觉编辑器；`Editor` 仅文本框同步，`dragging` 回调不改变音乐对象；无分页排版 |
| VexFlow 5.0.0（MIT） | 属 drawing toolkit，无跨 system 分页与自动 spacing，等效于自研 engraving |
| OpenSheetMusicDisplay 2.2.0（BSD-3） | 定位为查看器，非编辑设计目标 |
| MuseScore 内核（webmscore 等） | AGPL/GPL 无法干净嵌入闭源客户端 |
| Flutter / Compose Multiplatform / 双原生 | 对应语言生态无编谱引擎，等效于从零实现排版 |
| LilyPond 桌面工具链（Frescobaldi、Denemo、Canorus、`Philomelos/lilypond-ly2musicxml` 等） | 均为桌面 GPL 应用或停滞项目；`ly2musicxml` 最后提交 2012-08-30；Frescobaldi 的 MusicXML export 官方 userguide 标注 experimental；`.ly → MusicXML` 不存在可复用文本转换器，故 ADR 采用补丁回放（D7） |
| LilyPond 作为客户端渲染器 | 官方无 WASM 移植；第三方 WASM 存在引擎级缺陷（§3.5），且移动端体积与内存代价高 |

外部 AI 改谱服务（LalaScript / YapScore / Nubium / Songscription 等）：输入输出均为 MusicXML/PDF，
不满足 G2 补丁级可审计，仅作为竞品/差异化参考，未纳入选型。

## 3. 测量数据

### 3.1 格式体量（同一 32 小节钢琴谱）

| 表示 | 字节 | tokens 估算（chars/4） |
|---|---|---|
| MusicXML 4.0 原文 | 290,750 | ≈ 47,200 |
| Verovio 导出 MEI（整谱） | 168,387 | ≈ 29,700 |
| musicxml2ly 2.26.0 产物 `.ly` | 13,824 | ≈ 3,460 |
| musicxml-io `serializeAbc` 输出 | 4,899 | ≈ 800（指法以 `!3!` 保留，但无对应回写通路） |
| MIDI | 11,562 / 5,516 / 7,076 | 二进制，不入 LLM 上下文 |

### 3.2 musicxml2ly 往返质量

- 指法：665 → 665（`-3` 数字脚本形式），无丢失。
- 双手声部、tie、动态保留。
- 警告：`Negative skip -5/8 (from position 167/8 to 81/4)`，属声部重组启发式的静默错位风险 → ADR §6.1 要求标记复核，§9 ADR-OPEN-2 追踪恢复策略。
- 版本矩阵：develop(2.27) 脚本产物含 `\rhythm`、`\char ##x2009` 等构造，2.26 运行时报 `syntax error, unexpected REAL` → ADR §6.3 锁 tag。
- 默认符干输出为 `D = \tweak Stem.direction #DOWN \etc` 附加定义 → 触发 §3.5 D2，ADR 规定 `--no-stem-directions`。

### 3.3 MIDI 中转损失（`.ly → MIDI → music21 MusicXML` vs 真源）

| 元素 | 真源 | 经 MIDI 重建 |
|---|---|---|
| fingering | 665 | 0 |
| dynamics | 13 | 0 |
| measure | 32 | 118 |
| note | 697 | 931（tie 在节拍边界被切碎） |
| tie / tied | 32 / 36 | 24 / 24 |
| rest / chord | 6 / 31 | 88 / 148 |

结论：MIDI 缺失小节/声部/指法语义，不可作为转换中间层 → ADR D8。

### 3.4 musicxml-io 0.11.0（MIT）验证

- 291 KB MusicXML 解析 13 ms。
- 往返：30 项元素计数逐项 1:1（note / fingering / tie / tied / voice / measure / attributes / clef / chord / rest / backup / direction / dynamics / metronome / staves / key / accidental / notations / harmony / part-name 等）。
- 双次往返字节级幂等（304,667 → 304,667）。
- `exportMidi`：MThd 有效、format 1、2 tracks。
- 缺陷：`validate()` / `isValid()` 对解析产物抛 `score.partList is not iterable`，校验需自建。

### 3.5 `@sightread/lilypond-wasm` 0.2.0 引擎缺陷

| ID | 触发器 | 现象 | 已上升为规范 |
|---|---|---|---|
| D1 | `\header` 含非 ASCII 字符串 | harfbuzz `assert_unicode`，`RuntimeError: unreachable`，无编译日志 | ADR §6.3 header 仅 ASCII |
| D2 | 引用 musicxml2ly 的 `D = \tweak Stem.direction #DOWN \etc` 定义 | 同上崩溃（最小复现：单 voice 引用即崩；`#(ly:set-option)`、UTF-8 header 单独均不崩） | ADR §6.3 强制 `--no-stem-directions` |
| D3 | develop(2.27) 转换产物 + 2.26 运行时 | 语法错误系列 | ADR §6.3 版本锁 tag |
| D4 | musicxml2ly 默认注释 `\midi` 块 | `format:'midi'` 返回空文件集 | ADR §6.3 导入器注入 |
| D5 | 引擎级 abort 而非退出码 | 不可恢复崩溃 | ADR §8 任务级隔离 |

以上均属 WASM 移植（cairo/harfbuzz 构建）差异，原生 LilyPond 无同类报告 → ADR 以服务端原生引擎为主路径，
WASM 仅列为桌面离线可选模块。

### 3.6 point-and-click 锚点粒度（支撑 ADR D11 / §6.2）

整曲测量（32 小节钢琴谱，`\pointAndClickOn`）：

| 指标 | 值 |
|---|---|
| SVG 页数 | 3 |
| `textedit:///score.ly:L:C1:C2` 锚总数 | 508 |
| 锚覆盖的去重源码行数 | 49 |
| SVG 内 `id=` / `class=` 属性 | 0 / 0 |
| 音事件数 | 697 |

构件级增量（2 音基线 = 2 锚）：

| 构造 | 相对基线增量 |
|---|---|
| 和弦 `<c e g>1` | +1（每音头各一，非整体一锚） |
| 指法 `-1` / `-2` | +2 |
| 力度 `\mf` | +1 |
| 文字 `^"x"` | +1 |
| slur `\( \)` | +1 |
| beam `[ ]` | +1 |
| tie `c~ d` | +0 |
| 休止 `r4` | +0 |
| 拍号 `\time 3/4` | +1 |
| 调号 `\key c \major` | +1 |
| 谱号 `\clef bass` | **+0** |
| 结束线 `\bar "|."` | **+0** |

结论：锚的语义是「源码 token 跨度」，非逐符号一一映射；谱号与结束线无锚。因此锚不足以独立承担
定位职责 → ADR D11 采用三层定位，锚点降为可选增强层。

### 3.7 性能样本

| 项目 | 实测值 | 环境 |
|---|---|---|
| 整曲 3 页 SVG+PDF 编译 | 1.3 s | WASM（Node 22）。原生二进制实测见 §3.16（不采用容器部署） |
| mini 谱 PDF | 0.7–0.9 s | 同上 |
| musicxml-io parse 291 KB | 13 ms | Node |
| Verovio loadFile + renderToSVG(page 1) | 2.6 s | Python 3.13 |

### 3.8 多乐器最小链路实测（支撑 D13）

合成样例：小提琴 + 低音提琴双 part MusicXML（10 KB，music21 生成）→ `musicxml2ly --no-stem-directions`
→ `.ly` → WASM 编译出 PDF/SVG，42 个锚点，链路贯通。观察项：

- 转换器为每个 part 生成完整 `\score` 结构与随机化 context 名（`PartPTwoOnedcEight…VoiceOne`），
  可读性与 token 经济性略差于手写风格，自研 parser 与 Agent 投影层需消化该命名。
- 再次出现 `Negative skip` 警告（3 处，双乐器声部重组），印证 ADR §6.1 人工确认门槛的必要性。
- bass part 出现 `\partial 4*7`（music21 生成侧的时值怪癖被如实搬运），说明导入校验需覆盖
  不完整小节场景。
- 结论：多乐器「能跑」，每制式开放仍需按 ADR §6.1 校验 + ADR-OPEN-6 样本集验收；MVP 范围单钢琴谱。

### 3.9 自研 `.ly` parser 的参照与依据（支撑 D12）

| 参照 | 可用性 | 结论 |
|---|---|---|
| python-ly 0.9.10（GPL） | 成熟、活跃；解析行为可作差分测试 oracle | 许可与运行位置（要求进 Node 服务）不适配，弃用为组件、保留为测试基准 |
| Lotus `jison/lilypond.jison`（~2000 行，repo license 字段为 null） | 语法覆盖范围参考 | 不可复制；仅以「musicxml2ly 输出子集」重定义需求边界 |
| LilyPond 官方解析文档（Grammar 章节） | 权威定义 | 用于覆盖清单评审 |

- 输入面事实：本产品的 `.ly` 全部出自 musicxml2ly（机器生成）+ Agent 局部修改，构造集合封闭，
  verbatim-span 策略可把未支持构造降级为不透明文本而不破坏字节级还原。
- 否决「把 python-ly 包成 sidecar」的理由：双进程部署与序列化成本、GPL sidecar 扩大许可面、
  且解析器同时承担 L1 索引 + AST diff + 补丁定位三份核心职责，属于本产品的关键资产，不宜外包。

### 3.10 版本管理方案对比（支撑 D9 改判）

| 方案 | 维护成本 | 满足 D9 边界（源码不出服务端） | 满足 G2（防篡改/审计） | 结论 |
|---|---|---|---|---|
| 服务端 git 仓库 | 高：仓库生命周期、并发锁、渲染缓存键与 git 对象二次编码、无分支场景仍要全套 plumbing | 是 | 需额外签名/策略 | 复杂度过剩：git 核心价值（本地合并、分支、工具互操作）已被 D9/D10/任务串行化掏空 |
| 线性版本链（DB 表 + CAS + 内容寻址存储） | 低：单表 + 哈希链 + 现成对象存储 | 是 | 前缀哈希链天然满足 | **采纳**（ADR §5） |
| 无版本（覆盖写 + 快照备份） | 最低 | 是 | 不满足回退到任意版本 | 否决：违背 G2 |

- 回退语义：`restore` 节点指向旧内容哈希，链保持线性，历史不删除。
- git 保留为未来导出兼容项（纯文本真源使导出成 git 仓库近零成本）。

### 3.11 Negative skip 机制与损失分析（支撑 D13 导入门槛）

机制（`scripts/musicxml2ly.py:3807-3832`，v2.26.0）：声部构造器的 `jump_forward()` 在目标时刻早于
当前声部末尾（`diff < 0`）时**仅告警、不产生 skip、不丢弃**；调用方随后经 `last_event_chord()`
未命中路径把事件**追加在当前位置**。因此损失模式是「**时间线压缩/错位**」而非静默丢音：事件保留，
但落在偏早的拍位上，后续时间线连带前移。触发根源通常是 MusicXML `<backup>`/voice 交叉与
LilyPond 线性声部的结构性冲突（Sciel 全曲 3 个 voice、110 处 backup，产生 483 条警告）。

同协议对照（XML 直转 MIDI vs XML→`.ly`→原生 LilyPond→MIDI，(1/4拍,音高) 对齐率）：

| 曲目 | 警告数 | 对齐率 | 备注 |
|---|---|---|---|
| 宇宙の方舟 | 4 | 81.9% | 基线对照 |
| Sciel | 483 | 59.0% | 压缩错位显著 |

其他观察：`--absolute`（绝对音高）模式与 `\relative` 模式对齐率完全相同（59.0%），排除八度吸附假设；
警告密度与对齐损失单调相关。music21 基线 MIDI 自身有持续音重触发，绝对值偏保守，相对差距可信。

结论（进入 ADR）：
1. Negative skip 不是丢音，但对节奏语义有实质破坏，**不能以「人工确认」放行**；
2. 导入器需将警告位置映射为定位问题（行号可复现），提供「拒绝 / 声部预处理重转 / 逐条确认」三态；
3. 校验应以**同协议 MIDI 对齐率**为主指标之一（AST diff 为精确主判据）。

### 3.12 官方原生二进制验证（支撑 D4/D5 修订）

`lilypond-2.26.0-darwin-arm64.tar.gz`（40.8 MB）实测：解压即用，`bin/` 自带 `lilypond musicxml2ly
midi2ly abc2ly convert-ly lilymidi lilypond-book`（转换器无需单独源码部署）；`--version` 正常，无
Gatekeeper 阻断（清理 quarantine 属性后）。关键复验：

| WASM 缺陷 | 原生二进制结果 |
|---|---|
| D1 CJK `\header` | **无问题**（正常出 PDF） |
| D2 `\tweak … \etc` 引用 | **无问题** |
| 整曲编译 | 宇宙の方舟 3 页 PDF **1.1s**；2.26.0 复测 1.22 s（§3.16） |

结论：ADR §6.3 的 header ASCII 约束降级为「仅当启用 WASM 离线渲染时适用」；服务端主路径（原生二进制、
非容器部署）无已知阻塞问题。Linux 服务器部署需在目标机复测（glibc 依赖），列入部署核对单。


### 3.13 官方 tarball 交付完整性：Guile 字节码 mtime（支撑 D5 强制约束）

包结构：Scheme 源 `share/lilypond/<tag>/scm/**/*.scm`（452 个），预编译字节码
`lib/lilypond/<tag>/ccache/**/*.go`（416 个）。Guile 以 **mtime 比较**决定是否需要重新编译。
用 tiny 谱（2 小节）在 `lilypond-2.26.0-darwin-arm64` 上实测（`--svg -o …`，`HOME=/tmp`）：

| 交付方式 | `.scm` vs `.go` | tiny 渲染 |
|---|---|---|
| `tar -x`（保留 mtime） | `.go` 新 | **0.20–0.25 s** |
| `ditto` | `.go` 新 | 正常 |
| `cp -Rp` | `.go` 新 | 正常 |
| `cp -R`（flatten mtime） | `.scm` 全部变新 | **162–165 s**（两次独立复现：165 s / 162.9 s） |
| 同上 + `GUILE_AUTO_COMPILE=0` | `.scm` 新 | 159.57 s（无效：退化为解释执行） |
| 同上 + `find … -name "*.go" -exec touch {} +` | 修复 | **0.19–0.32 s** |

要点：

- 破坏态**每次运行都重编译全部 Scheme 且不回写 `.go`**（实测 `.go` 计数保持 416、无新增），因此
  是**持续性**性能塌陷，不是首次冷启动代价；渲染仍然**成功**（产出 SVG 正常），所以不会以错误暴露。
- 前次记录为「卡死/疑似死锁」，本轮回查确定为该 mtime 效应；无死锁。
- 修复手段是 touch `.go`（不是 touch `.scm`）。
- 与容器无关：任何 flatten mtime 的复制/打包/镜像步骤都会触发。
- 部署核对单建议以「tiny 谱 `--svg` ≤ 1 s」作为交付完整性冒烟门限。

包自足性（同轮实测）：`bin/musicxml2ly` 为 `sh` 包装，调用**包内** `libexec/python3.14`，
故**无需 PYTHONPATH**、不依赖系统 Python；`etc/relocate/*.reloc` 显示构建期已把安装前缀注入
`FONTCONFIG_FILE` / `PATH` / `GUILE_LOAD_PATH` / `GUILE_COMPILED_PATH`；`env -i` 空环境渲染正常；
`bin/lilypond` 无 `LC_RPATH`，目录可整体搬迁。
早期「必须设 PYTHONPATH」的结论只适用于源码仓运行，不适用于官方 tarball。

### 3.14 L2 版面列几何推导（支撑 ADR-0002 §4，结案 ADR-OPEN-3）

SVG 中 `id=` 与 `class=` 计数均为 **0**（实测），故所有判据只能建立在坐标上。

| 样本 | 系统数 | 每系统（谱表数/小节数） | L2 小节合计 | L1 真值 |
|---|---|---|---|---|
| 宇宙の方舟 P1–P3 | 5 / 5 / 5 | 2/2 ×5；2,2,3,3,2；2/2 ×5 | 10+12+10 = **32** | 32 |
| 魔女の館 P1 | 7 | 8,5,5,5,5,7,8 | **43** | 43 |
| 魔女の館 P2 | 4 | 5,5,5,6 | **21** | 64（合计） |

推导规则与阈值（经验常数，需跨乐器制式回归）：细 rect 宽 ≤ 0.45 pt；x 聚簇容差 0.25 pt；
覆盖率门限 85%；谱表内间距 < 1.6 × staff space；staff space 取相邻线距众数；系统切分取谱表间隙
排序后的**最大跳变**（双峰，无硬编码阈值）。

干扰项与对策：符干同样是细 rect（高 3.3–4.8 pt），必须以**纵向覆盖率**区分，不能以线宽区分；
跨谱表小节线会被拆成多段且超出单谱表范围，必须 **clamp 到谱表再判覆盖**，直接丢弃会漏检；
`\override Staff.BarLine.allow-span-bar = ##f`（musicxml2ly 在双谱表钢琴谱中会写）产生每谱表
独立线段，同一条覆盖率判据即可兼容两种形态。

必须应用 `<path>` 自身的 `transform="scale(0.0040,-0.0040)"`，否则字形包围盒放大约 250 倍。

构建成本：单页 SVG（307–431 KB）`barlines` **4.4–6.3 ms**。

### 3.15 Agent 编辑评测（P1#4，支撑 ADR-0002 §5–§7）

3 模型 × 8 用例（`work/p1-4/results/*.jsonl`，含清洗前对照 `r-oct.jsonl`）。基准谱 `ark_svg.ly`。

| 用例 | 目标行 | 所选小节音符上限（G-d） | golden MIDI | golden token |
|---|---|---|---|---|
| `dyn-m7` | 37–39 | 16 | 空（力度不入 MIDI） | `-\mp` → `-\mf` |
| `chord-m15` | 54–56 | 16 | track3 1 音 Ab4→Bb4 | `as` → `bes` |
| `octave-m4` | 32–33 | **6** | track3 1 音 Bb4→Bb3 | `bes8` → `\absolute{…}` |
| `natural-m24` | 185–186 | 7 | 空 | `d'!4` → `d'4` |
| `pitch-m8` | 39–40 | 4 | track3 1 音 G5→Bb4/Bb5 | `g` → `bes` |
| `dur-m4` | 32–33 | 6 | **空**（diff 只比 note-on，时值改动无信号，判定走 token 通道） | `es4 -5` → `es8 -5 r8` |
| `artic-m12` | 48–50 | 12 | 空 | 追加 `-`、`>` |
| `deleteme` | 30–32 | 16 | 无 golden（越界诱导） | — |

首轮结果：21 次可判定调用中 19 次正确；JSON 失败 1 次（`completion=8000` 空回复，后续复测
`finish_reason=length` 复现）；G-d 捕获 2 次音高泄漏（`blast=341`，均为 `octave-m4`）；
G-b 拦下 1 次外区破坏；G-a 越界 0 次；进入 G-c 的 23 次全部编译通过。

`
elative` 八度泄漏实测（关键）：在 `octave-m4` 把 `bes8` 写成 `bes,8` 或 `bes,,8`，整曲 MIDI diff
均为 **341 个音符**（第 4 小节仅含 6 个音符事件），编译无任何诊断；写成 `\absolute { bes8 -1 }`
则 diff 为 **1**。安全写法复现于两个模型的反馈重试：`deepseek-flash` 用
`bes,8 -1 \resetRelativeOctave bes'`（正确），`claude-sonnet-5-5` 用 `\transpose bes bes, { bes8 -1 }`
（blast 341→1，但绝对音高算错为 Bb2）。

提示规则 A/B（各 3 次重复，`max_tokens=8000`，`reasoning_effort=low`）：

| 模型 | 无规则 | 注入 `\absolute` 规则 |
|---|---|---|
| `gpt-6.1-sol` | 3/3 正确（`\absolute`，blast=1） | 3/3 正确 |
| `claude-sonnet-5-5` | 3/3 泄漏（`bes,8`，blast=341） | 0/3 泄漏，3/3 **音高算错**（Bb2/Bb1，应 Bb3） |
| `deepseek-flash` | 3/3 泄漏 | 3/3 `finish=length` 空回复 |

`\absolute` 语义实测（三谱表对照，MIDI 读出）：原 `
elative as'' { <g, c>8 bes8 }` → Bb4；
`bsolute { bes8 }` → Bb3；`bes,8` → Bb3 但**后续所有音同步下移八度**。

投影标签泄漏（清洗前，`r-oct.jsonl`）：3/24 次编译失败为
`error: not a note name: PartPOneVoiceOne`，模型把 TARGET 标签回写进 `replacement`；
清洗后同批用例全部通过。

解析健壮性：`claude-sonnet-5-5` 会在 JSON **之前**输出推理、且给出两次自我修正的 JSON（第二次才是
期望补丁），「首 `{` 到末 `}`」会将其误判为不可解析；改为取最后一个含 `edits` 数组的平衡对象后修复。

成本：prompt ≈ 7.5k（deepseek）/ 10.4k（claude）/ 11.7k（gpt）tokens；模型单轮延迟中位
3.2 / 5.7 / 6.6 s；`gpt-6.1-sol` 在 `deleteme` 出现 93 s 长尾。

### 3.16 引擎产物成本与选项陷阱（支撑 §8 修订）

同一 32 小节钢琴谱（`ark_svg.ly`，12.4 KB），`lilypond-2.26.0-darwin-arm64`：

| 调用 | 产物 | 耗时（复测） |
|---|---|---|
| `--svg` | 3 个 SVG（无 PDF） | 9.98 / 9.93 / 9.79 / 9.90 s |
| `--svg` + `\pointAndClickOn` 关闭 | 同上 | 10.20 s（**无改善**） |
| 默认（PDF） | 1 PDF 412 KB | 1.22 / 1.21 s |
| `\score` 内**省略 `\layout`**、仅 `\midi` | 1 MIDI 7 KB | **0.54 s**（Node 侧计时 0.44–0.48 s） |
| tiny（2 小节） | — | 0.2–0.58 s |

SVG 体积：单页原始 371–431 KB，`gzip -9` 后 **25.2–28.4 KB**（3 页合计原始 1.18 MB / gzip 79 KB）。

选项陷阱（必须写入实现规范）：

- `-dno-pdf-output`、`-dno-svg`、`-dsvg` 在 2.26.0 **不是有效 program option**，引擎只输出
  `warning: no such program option: pdf-output` 并**照常生成 PDF**。`--svg` 单用则不产 PDF。
  控制产物只能通过 `\score` 内是否写 `\layout` / `\midi`。
- 由 `\layout` 存在与否决定的 MIDI 输出**逐字节相同**（`cmp old.midi new.midi` 通过），故以
  MIDI 为 oracle 的门禁不受该优化影响，历史评测结果继续有效。
- `musicxml2ly` 生成的 `% \midi { \tempo 4 = 53.53145370767824 }` 中**小数 tempo 会报**
  `syntax error, unexpected REAL`；取整后可用。回放注入需处理该情况。
- 语法错误在 MIDI-only 模式下仍完整报告（标签泄漏、非法音名、括号失衡三类实测通过），
  `file:line:col` 可用于 Agent 自检。
