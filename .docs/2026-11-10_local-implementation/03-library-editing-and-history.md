# P03：文档、编辑与版本

- 状态：受阻（前序门槛）；库模块已接入本地运行时，P00 Linux ARM64 原生验收仍未通过。
- 技术方案：[03-technical-plan.md](03-technical-plan.md)。
- 前置：[P02](02-native-engine-and-artifacts.md)；类型和目录选择见[总索引](README.md)。
- 后续：[P04 运行时与 Agent 接口](04-runtime-and-agent-interfaces.md)。

## 1. 阶段目标

交付 `ossia-library`，管理新库、公共源码、编辑任务和不可变版本。通过核心接口即可完成导入、编辑、校验后提交、历史查询和回退。此阶段不修改用户数据。

版本节点用 UUID 与 parentVersionId 区分，不以重复内容哈希合并历史节点。

## 2. 数据模型与存储

新库 schema 初始版本为 1，SQLite 开启 WAL；只有持库进程可创建和写入数据库。至少建立以下逻辑表：

| 表 | 责任 |
|---|---|
| documents | 标题、输入类型、公共源码相对路径、可空 headVersionId、时间 |
| versions | 独立 versionId、文档内唯一 seq、parentVersionId、sourceHash、origin、作者和摘要 |
| edits | 单文档编辑所有权、ownerViewId、baseVersionId、active／interrupted／committed／ended 状态 |
| jobs | 队列和执行终态、输入快照、错误与结果引用；P04 接入调度 |
| render manifests / assets | 编译缓存键和不可变产物引用，消费 P02 格式 |
| worktree intents | 恢复／丢弃等需要改写公共文件的崩溃恢复意图 |

- 源码固定为 `documents/<documentId>/score.ly`，原输入副本在同文档 original 目录中；数据库不接受任意外部源码路径作为文档真源。
- 不可变内容按字节哈希写 gzip CAS；先完整写入 CAS 再提交引用，允许失败时出现可清理的无引用内容，不允许数据库指向未写完的内容。
- 初始化使用当前 `library.sqlite`。
- 库初始化、schema 检查和内容恢复均显式传入数据根目录，测试不得默认写家目录。

## 3. 工作流程与不变量

### 3.1 导入

1. 将输入复制到临时导入区，调用 P02 转换及导入报告。
2. refused 不建立文档；needs-confirmation 在显式确认对应损失后继续。
3. 获得 `.ly` 后建立文档和公共源码，原输入保持副本；首次成功编译后建立 origin=import 的第一个版本。
4. 手写 `.ly` 暂有编译错误时保留可修复的未提交文档，headVersionId 为空，页面展示诊断；不能创建“已编译成功”的 import 版本。第一次修复提交以该初始状态建立版本。

导入失败的临时区可以清理；成功导入后的文档和用户工作区不能作为临时文件清理。

### 3.2 编辑所有权

- `begin_edit(documentId, viewId, baseVersionId, expectedSourceHash)` 验证当前状态，返回 editId 和公共源码绝对路径。
- 同一文档只有一个 active／待恢复编辑状态。第二个任务返回 edit-busy；不同文档互不阻塞。
- 若公共文件已有未知未提交内容，返回 dirty-workspace；调用者必须明确恢复现有编辑或接纳该工作区，不能开始新任务并覆盖它。
- editId 是控制编辑的能力凭据，不向其他视图广播。恢复中断任务是显式操作，不根据最新会话自动接管。
- 应用锁约束受控入口，无法识别每次任意文件工具写入的进程身份。无活动任务时的改动标记为未归属；有任务时靠期望哈希和版本检查检测冲突，不宣称能证明写入者身份。
- 文件监听和调度由 P04 驱动；本模块提供 `observe_source`／状态重算入口，读取稳定快照，不把事件次数当作版本次数。

### 3.3 编译与提交

- check 始终读固定 SourceSnapshot；成功记录与 sourceHash、engineBuildId、optionsHash 绑定。
- `commit(editId, baseVersionId, expectedSourceHash, summary)` 检查所有权、head、磁盘内容与成功 SVG 结果。
- 所有验证通过后，在 SQLite 事务中追加版本、推进 head 并结束编辑。提交保存已验证快照，不再重写公共文件。
- 最后检查后又发生的外部写入作为新的未提交工作区处理，不能覆盖它或把新内容错误归入已编译版本。
- 无内容变化时结束任务并返回当前版本，不新增重复节点。编译错误不推进 head。
- 语义差异可以 unavailable；只要 SVG 和一致性要求满足，不因缺少 MIDI 或无法索引而拒绝合法修改。

### 3.4 放弃、恢复和崩溃

- 结束编辑默认保留公共文件，文档继续为 dirty。`discard` 必须单独传入预期源码哈希，才将公共文件恢复为 head 快照；无 head 的文档不擅自删除源码。
- `restore(targetVersionId, baseVersionId, expectedSourceHash)` 只在无活动编辑且工作区干净时进行；追加 origin=restore 节点，允许内容与历史版本相同。
- 文件替换使用同目录临时文件与原子重命名。修改前持久化 worktree intent，记录旧／目标哈希和计划版本；不能假设文件操作与 SQLite 事务原子提交。
- 重启恢复时：公共文件仍为旧哈希则取消未执行 intent；为目标哈希且 head 仍为基版本则完成对应事务；其他情况标记冲突并保留文件，不自动覆盖。
- 重启将运行中的 jobs 标为 interrupted，活动 edit 标为待恢复；未提交源码和已完成快照保留。

## 4. 测试与退出条件

- [x] 所有测试使用临时库；旧 schema 探测不改动原目录。
- [x] 三类导入、损失确认、阻断拒绝、无 head 的可修复 `.ly` 状态正确。
- [x] 同谱写入互斥、不同谱并行、无变化提交、错误编译拒绝和过期 head／sourceHash 冲突均覆盖。
- [x] A→B→恢复 A→C 的父节点和 seq 正确，不能因内容相同混淆版本。
- [x] 结束编辑保留文件；dirty 工作区不被 restore／discard 无条件覆盖。
- [x] 在 CAS 写入、文件替换和数据库提交间注入故障，重启后状态可解释且文件不丢失。
- [x] 外部保存发生在编译期间或提交最后检查之后时，已提交版本仍对应已验证字节，后续变化保持 dirty。

运行 `cargo test -p ossia-library`，并用 P02 真实工具链至少完成一次导入→改公共文件→check→commit→restore。数据库事务和文件边界用故障测试验证，不能只断言最终行数。

## 5. 实施交接

### 模块与交付

- `ossia-library` 位于 workspace，依赖 types／score／engine；不接 HTTP、MCP、GUI 或用户真实库。
- 工程与调用示例见 [crate README](../../crates/ossia-library/README.md)。共享 DTO 包含 DocumentState、Version、EditSession、Job、ImportOutcome 和 CheckOutcome；错误码包含 `storage-error`、`library-busy`，P04 已映射。
- bundled SQLite 使用 `rusqlite 0.41.1`／`libsqlite3-sys 0.39.1`，gzip 使用 flate2；保留并更新根锁文件。SQLite 依赖要求最低 Rust 1.89，因此同步 workspace 声明；本机实际使用 Rust 1.98.1，未单独运行 1.89 工具链。最低版本提升引出的四处 Clippy 建议在 score 中作等价改写，原有分析回归通过。

### Schema、文件与恢复

- 数据根目录必须显式传入；持库锁为 `library.lock`，数据库为 `library.sqlite`，SQLite application_id 为 `0x4f535349`，schema/user_version 为 1；WAL、外键和 FULL 同步已验证。
- schema 包含 documents、versions、edits、jobs、contents、renders、worktree_intents，以及私有 check_requests／check_receipts。检查任务中的编辑凭据不进入公开 Job；文档状态也不返回 editId。
- 公共文件为 `documents/<documentId>/score.ly`；原输入保存在 original/input.ly、input.musicxml 或 input.mid。gzip CAS 对未压缩原始字节计算哈希，先同步文件与新建目录再提交引用；读取时校验内容哈希。P02 产物位于稳定的 artifacts 根目录，不另建引擎缓存。
- 未知的 `library.sqlite` 标识或 schema 返回 `runtime-incompatible`；库内路径拒绝越界和符号链接。
- discard／restore 先记录 intent，再以同目录临时文件原子替换，最后完成事务。重启按旧／目标哈希和 head 判定取消、完成或冲突；冲突通过 DocumentEditState::RecoveryConflict 暴露。`resolve_recovery_conflict` 只在期望哈希匹配后清除冲突标记，不覆盖文件、不推进 head。
- 重启保留源码、CAS 和历史，将 active 编辑及 queued／running 作业标为 interrupted，并清除旧提交凭据。中断编辑需原 editId 显式 resume；未建立 head 的文档不自动删除或重置。

### 检查、提交与 P04 入口

- import 执行 P02 三态门槛。手写 ly 编译失败保留初始快照、原件和失败作业诊断；首次修复提交建立 import 版本。转换输入编译失败不发布文档。
- begin_edit 核对 head、当前哈希和编辑互斥；未知工作区需 adopt_workspace 显式接纳。end_edit 保留文件；discard 独立校验能力凭据、基版本和哈希。
- `queue_check` 在调度前固定输入并返回持久 jobId，`run_check` 执行该作业的 CAS 快照；`check` 复用这两个入口。P04 不应在执行排队任务时用新公共文件替换旧输入。
- 成功凭据绑定 editId、sourceHash、engineBuildId 和 optionsHash。commit 验证凭据、head、磁盘内容、CAS 和实际 SVG 产物，事务内追加版本并结束编辑，不改公共文件。无变化不新增节点；PDF／MIDI 独立失败不阻断成功 SVG。
- 旧检查结果仍按 jobId 查询，current=false 时不替换较新的当前渲染，也不授权提交新字节。lastCheck 仅表达当前源码的最近检查／导入；lastSuccessfulRender 可能是旧源码，调用端必须比较 sourceHash。
- create_job／get_job／job_snapshot／transition_job 提供其他任务的持久化；状态更新比较期望值，允许 queued／running 更新阶段，终态不会回到运行。P04 负责并发限额、取消 RunContext、视图鉴权和事件流；get_edit(editId) 供受控调用核对 ownerViewId，不枚举其他任务的凭据。

### 验证命令与结果

- `cargo test -p ossia-library`：22 项事务／并发／恢复回归通过；1 个标记 ignored 的子进程入口由测试显式启动，覆盖跨进程锁和直接退出后的 SQLite／文件恢复。
- `cargo test --workspace`：66 项测试、3 项文档测试通过。默认忽略的原生集成与子进程入口单独说明，不计作本次默认测试通过项。
- `OSSIA_TEST_LILYPOND_ROOT="$PWD/vendor/lilypond/2.26.0-darwin-arm64" OSSIA_NATIVE_EVIDENCE="$PWD/tests/fixtures/local/evidence/p03-native.json" cargo test -p ossia-library --test native_library -- --ignored`：通过。临时中文／空格库中三类输入分别完成导入→编辑→SVG/PDF/MIDI 检查→commit→restore；另覆盖真实转换损失确认、Negative skip 拒绝、错误 ly 修复为首版本和重启后的 5 个文档。证据见 `tests/fixtures/local/evidence/p03-native.json`。
- 故障注入覆盖 CAS 落盘后、版本事务提交前、intent 落盘后、文件替换后及最终事务前后；校验实际文件字节、可恢复的 head/seq/parent 和 CAS，不只检查行数。覆盖编译期间新保存、排队后新保存及提交最后检查后的保存。
- 验证：`cargo test -p ossia-library`、`cargo test --workspace`、`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`pnpm typecheck`、`pnpm check`。

### 阻塞与边界

库模块已接入本地运行时和 macOS arm64 测试包，仍受 P00 Linux ARM64 门槛阻塞。macOS ARM64 已验证；其他平台文件锁／SQLite／原子替换、真实断电、签名资源和跨平台安装包未验证。进程退出故障测试不能替代真实断电测试。应用锁不能排斥绕过协议的外部文件写入者。

库验证使用临时目录，不写默认用户库。运行时调度和传输复用本 crate 的编辑、版本和队列检查事务，不在调用层重写版本逻辑。
