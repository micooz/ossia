# Ossia 代码与测试重构优化交接

- 日期：2026-11-11
- 状态：待实施
- 依据：[仓库编码规范](../docs/coding.md)、[项目架构约定](../AGENTS.md)
- 范围：Rust crates、React score UI、单元测试和集成测试的结构整理；保持既有产品行为和跨层契约。

## 1. 任务目标

在不改变外部行为的前提下，降低核心模块的职责密度、改善测试定位，并让测试所在层级符合项目架构。按真实职责拆分模块和测试；复用已有 owning module 的工具；不为行数或抽象而拆分简单逻辑。

本任务是维护性重构。没有性能剖析数据，不预设算法或缓存性能问题。遇到真实行为缺陷时，按 `docs/coding.md` 先建立失败复现，再修复；纯结构调整保留既有测试语义，不扩展产品功能。

## 2. 审查基线

本交接来自 2026-11-11 的只读静态审查。未修改源码，未运行测试。文件行数是当时仓库快照的粗略规模，不作为拆分的唯一理由。

审查时 `git status --short` 显示以下 workflow 文件已有 staged 修改。开始实施前必须重新检查工作区，并保留届时存在的用户修改：

- `.github/workflows/auto-tag.yml`
- `.github/workflows/check.yml`
- `.github/workflows/release.yml`

交接文档写入后的最后一次 `git status --short` 未再显示这些 workflow 修改，只显示本交接文档为 untracked。本轮只写入 `.docs/` 文档，未对 workflow 文件执行操作；该状态差异来源未确认，接手时以当前 Git 状态为准。

仓库已有的本地架构和阶段实施材料位于 `.docs/2026-11-10_local-architecture-plan.md` 与 `.docs/2026-11-10_local-implementation/`。本任务只做内部结构整理，不重新讨论产品形态、模块职责或已确定的协议。

## 3. 依据与必须保持的约束

### 编码规范

- 快速显式失败；业务层让错误传播，不新增未经请求的兜底。
- 复用 owning module 中已有工具。简单的一次性逻辑留在调用点；只为隐藏实际复杂度、维护真实接口或复用逻辑提取模块。
- 源码在合理情况下控制在 500 行以内；单元测试文件不受此行数要求约束。
- 单元测试按源码结构组织，每个单元测试文件对应一个源码文件；crate 集成测试放在 `packages/ossia-<crate>/tests/`，UI 测试放在 `packages/ui/tests/`。
- Rust 是共享 DTO 的唯一来源。若重构无需改变类型，不改生成文件；如确需改变，按 `AGENTS.md` 使用生成命令，不手改产物。

### 架构边界

- 依赖方向保持 `ossia-types → ossia-score → ossia-engine → ossia-library → ossia-runtime`；集成 crate 保持独立于 runtime 的 HTTP/IPC 实现。
- `ossia-score` 是纯内存分析 crate，不使用文件、数据库、网络、LilyPond 进程，也不依赖 Tauri。
- `ossia-runtime` 仍是每个数据目录唯一的运行时实例；HTTP、IPC、CLI、MCP 复用同一运行时操作。
- UI 保持展示和交互职责，不接管解析、版本或持久化状态。
- 保持现有协议、事件、错误码、数据库 schema、版本历史和宿主集成行为；不要无必要改动生成的 TS 协议或 MCP fixture。

## 4. 审查发现与建议

### 4.1 运行时核心

[`packages/ossia-runtime/src/core.rs`](../packages/ossia-runtime/src/core.rs) 共 1,093 行，其中生产实现约 1,032 行。`Runtime` 同时负责视图与 ticket/token、选择和定位、编辑权限、事件队列、任务排队与执行、停止清理和文件监听。`schedule`、`run`、`watch` 是可辨认的生命周期职责；`watch` 的事件分派还压在单行宏表达式中。

**方向：** 将实现按视图/会话、任务生命周期、文件监听拆为内部模块；保留 `Runtime` 对外接口和现有共享状态语义。共享状态仍由单一所有者管理，不要新增只转发调用的 façade。把 watcher 和 SSE 事件循环整理成可读分支，保留原有事件顺序和错误处理。

**行为保护点：** ticket 一次性消费和过期、token 过期、视图与编辑所有权、事件序号/游标冲突、同文档串行和全局并发上限、预览合并、取消清理、监听防抖与轮询补偿、停止时等待工作线程结束。

### 4.2 score 布局实现

[`packages/ossia-score/src/layout.rs`](../packages/ossia-score/src/layout.rs) 约 989 行，涵盖 SVG path bounds 与 transform、SVG XML 解析、源码 anchor、谱表/系统识别、小节列计算和点击定位。

**方向：** 把 SVG 解析和几何计算收进 score crate 内部模块；把谱表系统、小节映射和 hit testing 留在布局实现。维持已有 `build_layout`、`locate` 等公共入口及其保守降级语义，不将定位逻辑移到 UI 或 runtime。

**行为保护点：** 无效页不导致后续页重新编号；只有证据充分时才提供 measure 定位；保留 anchor-only/none 能力和原因；源码 span 继续使用 UTF-8 字节范围；保留 SVG transform、anchor 栈和压缩小节的现有结果。

### 4.3 Integrations 主机实现

[`codex.rs`](../packages/ossia-integrations/src/codex.rs) 和 [`dsh.rs`](../packages/ossia-integrations/src/dsh.rs) 分别约 1,014 行和 944 行（包含文件末尾测试）。两者都有文件计划、所有权判断、JSON 文件和 managed record 生成逻辑，但所有权规则不完全相同：Codex 的 `record_owns` 要求记录带 hash，DSH 的对应函数只核对路径。

**方向：** 先按 detect/plan/apply/status/remove 的真实职责评估是否需要拆文件；仅抽取已确认语义相同的公共文件计划逻辑。保留主机特有配置、冲突和所有权规则，不以“去重”为由把差异压进复杂参数或通用 host 框架。

### 4.4 其他长源码

以下文件超过或接近 500 行，可在对应职责重构时一并评估；先读实现和调用关系，不因行数自动拆分：

| 文件 | 当时行数 | 初步观察 |
|---|---:|---|
| `packages/ossia-engine/src/lib.rs` | 666 | 工具链验证、转换、编译和输出收集都集中在 Engine 实现 |
| `packages/ossia-score/src/index.rs` | 627 | 索引构建与 locator 解析属于同一分析域，判断是否存在清晰内部 seam |
| `packages/ossia-library/src/lib.rs` | 586 | Library 公开工作流入口；已有 `workflow`、`recovery`、`storage`、`artifacts`、`db` 内部模块 |
| `packages/ossia-integrations/src/lib.rs` | 589 | Integrator 编排和文件/记录公共逻辑混合，需防止把简单 dispatch 包装成多层 |
| `packages/ui/src/App.tsx` | 533 | 会话刷新、异步选区校验、确认操作和页面组合集中在 App |

没有明确职责接口的文件保持原样。前端调整须保持当前视图代际校验、过期响应防护、session 失效行为和浏览器/Tauri 权限隔离。

## 5. 测试代码审查与整理方向

测试总体已放在 crates 的 `tests/` 和 `packages/ui/tests/` 下。主要问题是少量测试归属跨层、部分源内测试文件覆盖多个源码模块，以及两个集成测试文件过于集中。

### P0：将 score 原生交叉检查移到含引擎职责的层

[`packages/ossia-score/tests/native_engine.rs`](../packages/ossia-score/tests/native_engine.rs) 会读写临时文件、启动 LilyPond，并检查 native MIDI/SVG。它验证 score 分析结果与原生引擎之间的交叉行为，与 `ossia-score` 的纯内存职责冲突。

**建议：** 移到 `packages/ossia-engine/tests/`，例如命名为 `score_native_crosscheck.rs`。Engine crate 已依赖 `ossia-score`，适合承载此集成验证。保持其 `#[ignore]`、`OSSIA_TEST_LILYPOND_ROOT` 前置条件、fixture 路径和可选 evidence 输出行为；不要让默认离线测试隐式启动引擎。

### P1：按源码归属整理 engine 单元测试

[`packages/ossia-engine/src/tests.rs`](../packages/ossia-engine/src/tests.rs) 约 565 行，包含 source 派生/校验、diagnostics 与源码位置映射、cache 锁/损坏恢复、Engine 转换/编译/资源清单等测试。

**建议：** 将只测单个内部模块的测试移到对应源码模块的测试子模块；针对公开 Engine 工作流、需要真实进程或多个模块协同的用例放在 `packages/ossia-engine/tests/`。已有 `process.rs` 的本地单元测试保持 process 所属，不要复制测试。

### P1：按模块和测试层拆分 library 单元测试

[`packages/ossia-library/src/tests.rs`](../packages/ossia-library/src/tests.rs) 约 1,386 行，场景涵盖 schema/写锁、导入、编辑与提交、恢复、并发检查、重启和进程死亡恢复。测试 fixture 中包含确定性 `TestEngine`、`Fixture` 和故障注入 hook；部分场景直接访问 `db()`、`storage::DATABASE` 或 `hooks`。

**建议：** 私有数据库/存储/恢复机制测试留在相应 owning module 的单元测试中；以公开 Library 行为为主的跨模块场景整理到 crate 集成测试。保留对内部故障点的覆盖，不要为了移动测试而把测试专用 hook 暴露成生产公共 API。仅在多个测试目标确实共享相同语义时，提取 `tests/common` 支持代码。

### P1：拆分 runtime 集成测试文件

[`packages/ossia-runtime/tests/runtime.rs`](../packages/ossia-runtime/tests/runtime.rs) 约 1,057 行，包含队列/取消、view ownership、watcher、事件游标、导出、HTTP 和 IPC 场景；[`native_runtime.rs`](../packages/ossia-runtime/tests/native_runtime.rs) 约 532 行，覆盖真实桌面程序、MCP、CLI、HTTP、运行时生命周期和崩溃恢复。

**建议：** 将离线集成测试按行为拆为 jobs/watch、views/events、HTTP/IPC 等测试目标；跨目标共用且确实一致的 fake engine、runtime fixture 放在 `tests/common/`。原生套件保持显式环境变量和 ignored 行为，并继续记录哪些平台未运行。跨模块测试仍可作为集成测试，不需要强行按单个生产文件拆分。

### 保持现有测试分组

- `packages/ossia-score/tests/layout.rs`（427 行）集中验证 SVG 布局和定位；`fixtures.rs`（314 行）覆盖分析对照数据；作为 crate 级行为验证可暂时保留。
- `packages/ui/tests/browser.spec.ts`（416 行）覆盖浏览器 mock 场景，`native.spec.ts`（239 行）覆盖真实引擎/UI 闭环。它们已位于正确目录；当前没有必要只为拆文件而重组。
- integrations、types、library、engine 的 native suites 已分别位于 crate 的 `tests/`。继续保留临时宿主/临时数据目录隔离，不用个人真实数据。
- 不把测试 helper 建成跨 crate 通用测试框架；仅抽取重复且语义稳定的 setup。

## 6. 建议执行顺序

### 阶段 0：接手检查

- [ ] 重新读取 `AGENTS.md` 和 `docs/coding.md`。
- [ ] 检查 `git status --short`、staged/unstaged diff 和未跟踪文件；保留其他会话修改。
- [ ] 对照本交接涉及的源码和测试，确认行数、接口及本机运行状态未漂移。
- [ ] 将本次工作限定为结构重构，不借机改变产品行为或协议。

### 阶段 1：先修正测试归属并建立可 review 的小 diff

- [ ] 将 score 的 native cross-check 迁到 engine integration tests。
- [ ] 将 engine 的跨模块源内测试按 owning module 归属拆分。
- [ ] 将 library 的测试按私有实现测试与公开工作流集成测试分层，保留故障注入语义。
- [ ] 将 runtime 的大集成测试按 jobs/watch、views/events、HTTP/IPC 分组，复用最小必要 fixture。
- [ ] 只移动/重组现有断言，不借重构之名删除覆盖或改弱断言。

### 阶段 2：重构 Runtime 核心

- [ ] 先标出 `Runtime` 共享状态读写点、锁范围、事件发射点及各异步 worker 生命周期。
- [ ] 将 view/session、job scheduler/execution、filesystem watcher 拆为内部实现模块。
- [ ] 保持 `Runtime` 的公开方法、HTTP/IPC/MCP wire contract 与 stop 生命周期。
- [ ] 展开 watcher 和 SSE 事件处理，确保锁不跨越长时间 IO 或不必要 await。

### 阶段 3：重构 score SVG 布局

- [ ] 把 SVG parser/transform/path bounds 的内部表示与 score layout/result types 的依赖关系明确下来。
- [ ] 选择最小的 private module seam；保持 public exports 和纯内存职责。
- [ ] 保持 path/transform/error/capability/page mapping 的现有行为和 fixture 结果。

### 阶段 4：按需整理其他模块

- [ ] 只有在职责边界清楚时才拆 engine、library、integrations、index 或 UI。
- [ ] Integrations 先明确 Codex/DSH 所有权差异，再决定公共文件计划模块的接口。
- [ ] App 拆分应产生真正独立的会话/页面职责与小接口，不以 hook wrapper 形式搬动代码。

## 7. 验收要求

- 外部行为、公开 Rust 类型/函数、序列化字段、HTTP/IPC/MCP 契约和 TS 生成产物无意外变化。
- 模块边界符合 crate 依赖方向；纯 `ossia-score` 不再包含进程/文件型原生引擎测试。
- 单元测试文件能明确对应到单个 owning source module；跨模块行为使用 crate integration tests。
- 测试迁移后没有丢失场景、削弱断言或无意改变 ignored/env-gated 前置条件。
- `cargo fmt` 格式化结果干净；按变更范围运行相关 crate tests 和 UI 检查，并记录实际命令/结果。涉及 Rust 公共契约时再运行 workspace 检查。
- 交接时列出未运行的原生环境测试和平台限制，不把离线 fake、编译成功或 ignored test 当成真实平台验收。
- 最终 diff 不包含本任务之外的 staged/unstaged 修改；不提交、不推送，除非后续指令明确要求。

## 8. 明确不做

- 不改变 Rust crate 职责、产品架构、数据格式、数据库 schema、协议版本或用户工作流。
- 不手改 `packages/ui/src/generated/protocol.ts` 或 `tests/fixtures/local/mcp-tools.json`。
- 不新增 wrapper/trait/manager 仅为缩短文件或消除表面重复。
- 不把 UI 解析、版本逻辑、Tauri 权限或宿主安装逻辑移入错误层级。
- 不进行无 profiling 依据的性能优化，不新增兜底，不清理其他会话文件/日志/工作树。

## 9. 接手摘要

先把测试代码纳入同一重构计划：最高确定性事项是将 `ossia-score/tests/native_engine.rs` 迁到 `ossia-engine/tests/`。随后按 `coding.md` 整理 engine/library 单元测试归属和 runtime 集成测试分组；之后从 `ossia-runtime/src/core.rs` 与 `ossia-score/src/layout.rs` 这两个职责最密集的生产模块开始拆分。每阶段小步实施、保留契约、记录检查结果，并在开工前重新核对工作区状态。
