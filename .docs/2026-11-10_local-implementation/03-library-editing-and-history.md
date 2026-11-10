# P03：文档、编辑与版本

- 状态：待实施。
- 前置：[P02](02-native-engine-and-artifacts.md)；类型和目录选择见[总索引](README.md)。
- 后续：[P04 运行时与 Agent 接口](04-runtime-and-agent-interfaces.md)。

## 1. 阶段目标

交付 `ossia-library`，管理新库、公共源码、编辑任务和不可变版本。通过核心接口即可完成导入、编辑、校验后提交、历史查询和回退。此阶段不改变真实用户库，不实现旧数据迁移或自动清理。

旧 `store.ts` 仅作职责参考。新版本节点用 UUID 与 parentVersionId 区分，不能沿用重复内容哈希即同一历史节点的歧义。

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
- 不为本次建立旧 schema 兼容层。初始化发现旧库时返回 legacy-data-present，切换处理留给 P07。
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

- [ ] 所有测试使用临时库；旧 schema 探测不改动原目录。
- [ ] 三类导入、损失确认、阻断拒绝、无 head 的可修复 `.ly` 状态正确。
- [ ] 同谱写入互斥、不同谱并行、无变化提交、错误编译拒绝和过期 head／sourceHash 冲突均覆盖。
- [ ] A→B→恢复 A→C 的父节点和 seq 正确，不能因内容相同混淆版本。
- [ ] 结束编辑保留文件；dirty 工作区不被 restore／discard 无条件覆盖。
- [ ] 在 CAS 写入、文件替换和数据库提交间注入故障，重启后状态可解释且文件不丢失。
- [ ] 外部保存发生在编译期间或提交最后检查之后时，已提交版本仍对应已验证字节，后续变化保持 dirty。

运行 `cargo test -p ossia-library`，并用 P02 真实工具链至少完成一次导入→改公共文件→check→commit→restore。数据库事务和文件边界用故障测试验证，不能只断言最终行数。

## 5. 实施交接（实施后填写）

- schema、事务和恢复入口：未实施。
- 编译结果绑定和编辑生命周期测试：未执行。
- 数据目录隔离证据：未建立。
- 下一阶段：P04 为这些操作增加传输、任务调度和会话身份，不另写版本逻辑。
