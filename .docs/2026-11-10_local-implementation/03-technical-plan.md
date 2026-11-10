# P03 技术实现方案：文档、编辑与版本

- 基准：`a58e01c`。
- 交付：`ossia-library`，完成导入 → 编辑公共源码 → check → commit → history → restore。
- 实施结果：本机实现完成、未集成；P00 Linux ARM64 门槛保留。检查与边界见 [P03 实施交接](03-library-editing-and-history.md#5-实施交接)。

## 存储与模块

- 加入 Cargo workspace；使用 bundled SQLite、WAL、外键、FULL 同步，以及 gzip CAS 和 OS 持库锁。
- 显式传入 home；只识别当前 `library.sqlite` 标识，不检查、迁移或清理其他数据文件。
- schema 1 持久化文档、独立 UUID 版本、编辑、任务、渲染引用与文件恢复 intent。源码固定为 `documents/<documentId>/score.ly`，原输入保存在 original。
- CAS 完整落盘后才提交引用；P02 继续管理产物缓存。引擎执行不持数据库锁。

## 核心接口

- 共享 DTO 增加文档、版本、编辑、任务和导入／检查结果，保持 camelCase 和既有标识约定。
- 文档：import、list_documents、get_document、observe_source。
- 编辑：begin_edit、resume_edit、end_edit、discard；未知工作区需要显式接纳，中断恢复需要原 editId。
- 版本：check、commit、history、restore。check 固定快照；commit 核对凭据、head、源码及成功 SVG，不重写公共文件。无变化结束任务而不新增版本。
- 导入执行三态门槛；手写 ly 编译失败保留无 head 的文档和初始快照；转换输入失败不发布文档。
- queue_check 固定作业输入并返回 jobId，run_check 执行该快照；任务提供持久化和条件状态更新，调度由 P04 完成。成功 SVG 不受 PDF／MIDI 独立失败影响。

## 一致性与恢复

- end_edit 默认保留文件；discard 单独核对期望哈希，无 head 不自动重置。
- restore 要求无编辑且工作区干净，验证目标快照与 SVG，追加 restore 节点。
- 文件替换前提交 intent，随后同目录临时文件、同步、原子替换，最后完成事务。
- 重启按实际文件哈希与 head 取消未执行操作、完成已执行操作或报告冲突；不自动覆盖冲突文件。
- 重启将 queued／running jobs 和活动编辑转为 interrupted，保留源码与快照。

## 验证与交接

- 临时库测试覆盖导入三态、无 head 修复、编辑互斥／跨文档并行、过期提交、外部保存、无变化提交与历史链。
- 故障注入覆盖 CAS、intent、文件替换和事务；检查实际字节及持久引用，并用子进程中断验证恢复。
- macOS ARM64 显式运行真实 P02 工具链闭环；缺资源不能伪报通过。
- 运行库／workspace 测试、fmt、Clippy、pnpm 基线／类型／检查；更新 P03、索引、主方案与 P04 调用示例。
- 不接 UI／HTTP／MCP、不注入、不发布、不操作真实用户库。
