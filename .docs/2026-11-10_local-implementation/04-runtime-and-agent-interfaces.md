# P04：本地运行时、HTTP、CLI 与 MCP

- 状态：本机实现完成、未集成；受 P00 Linux ARM64 前序门槛阻塞，不标记整体已验收。
- 前置：[P03](03-library-editing-and-history.md)；模块、DTO 与错误码见[总索引](README.md)。
- 后续：[P05 页面与管理窗口](05-browser-score-and-desktop.md)。

## 1. 阶段目标

实现 `ossia-runtime`，让浏览器、CLI 和 Agent 通过同一个本地实例操作库。发布入口支持 headless serve／mcp 模式，未初始化 Tauri 也能完成完整工具闭环。GUI 的改造留到 P05。

本阶段提供命令、协议和集成测试，不注入真实宿主配置，不占用旧库。旧 Next.js 服务已按用户指令提前移除，本阶段须建立实际 Rust 服务入口并接入开发命令，不设置旧服务回退。

## 2. 启动、发现与调度

- 在桌面可执行文件的 main 入口先分派 `serve`、`mcp`、`doctor`、`integrate` 和文档／任务 CLI，再进入 GUI 分支。不能为使用 MCP 强制先打开窗口。
- 实现 `ensure_runtime(home)`：获取 OS 文件锁后启动持库实例，或通过本地端点连接既有实例。锁在进程整个生命周期持有；PID／端点文件只是发现信息，不替代锁。
- Unix socket 使用当前用户访问权限；Windows named pipe 限制为当前用户，名称由规范化 home 推导。两个同时启动的客户端最终只能产生一个写库实例。
- 握手检查协议主版本和 schema。旧 schema／不兼容实例返回明确错误，不接管、不扫描其他库，也不自动终止进程。
- HTTP 默认绑定 `127.0.0.1:3211`，开发脚本显式使用 3212；测试允许端口 0。显式端口被占用时报错，不递增猜测。IPC 总是返回本次实际 webBaseUrl。
- `serve` 独立于 MCP／页面存活；显式 stop 拒绝新任务、取消并回收进程树后退出。默认无开机启动、无自动更新或空闲关闭。
- 全实例最多两个引擎任务并行，同文档最多一个编译运行；自动预览将待运行任务合并为最新快照。显式 check 的结果必须能按 jobId 查询，不能被合并后无声消失。
- 文件保存使用 300 ms 防抖，兼容原子替换／重建 watcher。编译前读取稳定字节快照；事件丢失时在 context／check／commit 入口重新读取文件。
- 使用 P03 的 `queue_check` 固定输入并返回 jobId，再由调度器调用 `run_check`；不重新读取排队后的公共文件替换该任务输入，也不另建一套校验凭据。`check` 是同步便利入口。
- Job 持久状态接入 P03。重启将 running／queued 作业记为 interrupted，不声称旧子进程仍在执行或自动重放 Agent 修改。

## 3. 视图、鉴权与事件

- `open_view` 返回随机 viewId、短期单次启动 ticket 和包含该 ticket 的 URL。ticket 放 URL fragment，不进入请求 URL；页面交换后从地址中移除。
- 启动 ticket 有效期为 5 分钟，只能交换一次；页面令牌仅保存在该标签页 sessionStorage，有效期为 24 小时或至实例退出。失效后由 Agent 重新 open，而非无鉴权恢复。
- 编辑操作通过 `get_edit(editId)` 核对 ownerViewId；该接口只能供已持凭据的受控调用使用，不向其他视图枚举凭据。未知工作区用 begin_edit 的 adoptWorkspace 显式接纳；冲突恢复通过 `resolve_recovery_conflict` 确认期望哈希后继续，不能在传输层擅自重置文件。
- 页面持有的令牌绑定 viewId；可在该视图中选择文档，但不能读写其他视图的选区或编辑凭据。
- HTTP 校验实际 loopback Host、允许的 Origin 和 bearer 令牌；生产同源，开发仅显式允许配置的 Vite origin。拒绝通配 CORS、任意 Host 和远端接口暴露。
- `/health` 只返回存活与协议标识；不包含 home、源码路径或启动凭据。其他数据操作走鉴权入口。
- 事件以 SSE 流提供，使用可携带 Authorization 的 fetch 流读取。封包为 `instanceId, seq, type, viewId?, documentId?, sourceHash?, payload`。
- 初次和每次重连都先获取快照，再接增量事件；实例变化／游标过期时重新同步。事件不承担永久审计，不广播 ticket、令牌或其他视图的 editId。

## 4. HTTP 与工具映射

HTTP 使用 `/api/v1`，错误为 `{error:{code,message,detail?}}`；成功响应由共享 DTO 生成。冲突／dirty／busy 为 409，输入错误 400，鉴权失败 401／403，不存在 404，编译／不支持输入 422，已入队任务 202；`library-busy` 为 409，`storage-error` 为 500；`unauthorized`／`forbidden` 为 401／403，`runtime-unavailable` 为 503。

| HTTP | 行为 |
|---|---|
| `POST /sessions/exchange` | ticket 换页面令牌和初始视图信息 |
| `GET /status`、`GET /events` | 已鉴权状态与事件流 |
| `GET /documents`、`POST /documents` | 列表／multipart 文件导入；导入确认携带损失摘要 |
| `GET /documents/{id}` | 文档、工作区、历史概况 |
| `GET /views/{viewId}` | 当前视图上下文快照 |
| `PUT /views/{viewId}/document` | 切换该视图文档，清空旧选择 |
| `POST /views/{viewId}/locate` | `sourceHash,page,x,y` 返回 locator；不自动修改选择 |
| `PUT /views/{viewId}/selection` | 提交／清空与当前 sourceHash 一致的选择 |
| `POST /documents/{id}/edits` | 开始／恢复／结束／丢弃，调用 P03 核心动作 |
| `POST /documents/{id}/check`、`POST /documents/{id}/commit` | 快照校验与版本提交 |
| `GET /documents/{id}/versions`、`POST /documents/{id}/restore` | 历史和恢复 |
| `GET /jobs/{jobId}`、`POST /jobs/{jobId}/cancel` | 作业状态和取消 |
| `GET /renders/{renderKey}/pages/{page}` | SVG；版本归属由核心验证，不允许任意路径读取 |
| `POST /documents/{id}/export` | 指定 versionId／格式／可选输出路径，返回本机导出结果 |

打开视图、任意本地文件路径导入和宿主集成计划优先走受当前用户 IPC 保护的 CLI／MCP。页面上传文件使用字节流，不要求浏览器暴露原始绝对路径。

MCP 工具保留架构方案中的 `ossia_guide/status/list/import/open/context/edit/check/commit/history/restore/export`，再提供 `ossia_job` 查询或取消任务及 `ossia_doctor` 诊断（共 14 个工具）：

- `ossia_open(documentId?)` 返回 viewId 和可打开链接；context 必须带该 viewId。
- `ossia_edit(action, documentId, viewId, baseVersionId?, expectedSourceHash?, editId?)` 明确动作，不以可选字段隐式判断接管。
- `ossia_check(documentId, editId?, expectedSourceHash)` 返回作业引用；`ossia_commit` 必须携带 editId、baseVersionId、expectedSourceHash 和摘要。
- 长操作返回 jobId；结果／错误均可经 `ossia_job` 读取。取消是幂等操作，终态不会退回 running。
- MCP 标准输出只容纳协议消息；引擎和运行时日志写 stderr／本机日志，不污染 stdio。
- CLI 为同样能力提供 JSON 输出和非零错误退出码。mcp 是 stdio 到 runtime 的薄适配，不自建第二套库或引擎任务逻辑。

在 P04 建立 `pnpm generate:types`，从 ossia-types 生成前端协议类型；CI 检查生成后无差异。协议样本与每个 MCP 工具的输入 schema 一起固定。

## 5. 测试与退出条件

- [x] 并发启动只出现一个实例；进程崩溃、陈旧端点、端口占用和版本不兼容可诊断。
- [x] 未运行 GUI 时，CLI／MCP 可导入、open、edit、check、commit、history、restore、export。
- [x] 实际 MCP 客户端可完成 initialize／tools/list／tools/call；普通日志不污染传输。
- [x] 两个 view 的文档选择互不串用；过期 sourceHash／ticket／令牌不能继续写状态。
- [x] watcher 连续保存、删除重建、事件丢失、事件重连和旧编译完成后到达均有回归测试。
- [ ] 跨平台锁、socket／pipe、取消树和无显示环境启动：本机已验证，Windows／Linux 原生验证保留。
- [x] Rust 核心测试、HTTP 合约测试、MCP 集成测试、前端类型生成检查通过。

## 6. 实施交接

### 基准与接口

- 基于 `198bc18`，新增 ossia-runtime；复用 P03 持库锁、固定快照检查、版本事务与恢复。技术方案见 [04-technical-plan.md](04-technical-plan.md)，调用说明见 [crate README](../../crates/ossia-runtime/README.md)。
- 桌面入口在 Tauri 初始化前分派；`--no-default-features` 构建无需 GUI。GUI 页面改造仍属于 P05。资源默认从可执行文件所属 resources/engine 和 resources/ui 解析；macOS 应用包使用 Contents/Resources，开发用显式路径。
- Library 新增受文档／版本归属约束的产物查询、作业列表和版本导出；renderKey 由 documentId、sourceHash、engineBuildId、optionsHash 派生。源码导出直接取版本字节，PDF／MIDI 对版本快照编译；输出 create-new，不覆盖已有目标。
- 共享 DTO、HTTP 错误和 TS 类型已建立；协议与 schema 均保持 1。新增 unauthorized、forbidden、runtime-unavailable。工具输入 schema 固定在 `tests/fixtures/local/mcp-tools.json`；合成协议样本同步为可反序列化的当前 DTO。

### 发现、鉴权与调度

- `run/runtime.json` 是原子发布的发现信息，Library 的 OS 锁才是唯一写入所有权。Unix 使用私有当前用户目录与 0600 socket；Windows pipe 配置当前用户 SID 的 DACL。并发启动只有锁胜者提供服务；不接管不兼容实例。
- HTTP 仅绑定 127.0.0.1；默认 3211、开发 3212、测试 0，端口冲突直接失败。CLI 自动启动后台服务时隔离标准管道，避免 stdio 调用方等待后台服务退出；启动错误可传回 CLI。
- ticket 为 32 字节随机值、5 分钟、一次性、URL fragment；exchange 返回绑定 viewId 的 24 小时令牌。Host／Origin／bearer 校验、页面跨视图拒绝、编辑凭据所有权与受控恢复转移均在运行时执行。
- SSE 保留 512 条事件；快照与游标同步读取，旧实例／旧游标需要重新取快照。事件不携带 ticket／token／editId。运行中令牌过期会终止流；页面使用 fetch 流，P05 负责 sessionStorage 和清除 fragment。
- 全实例两个引擎任务，同文档一个；导入／恢复／导出也经过队列。显式检查按 P03 队列快照运行，预览只合并 pending 作业，旧 jobId 保留 cancelled。运行中取消先进入 cancelling，待原生进程回收后进入终态；已经完成的持久操作保留 done 结果。
- native watcher 配合每秒内容轮询，事件后 300 ms 防抖；本环境 FSEvents 无法启动，已实测轮询兜底下的原子保存与删除重建。context／check／commit 额外重读文件。重启保留工作区并将旧编辑／作业记 interrupted，不重放任务。
- `stop`、SIGINT／Unix SIGTERM 拒绝新任务、取消并等待 worker 清理，再释放端点和锁。默认无自启动、空闲退出或自动更新。

### 命令与验证

- `pnpm build:headless`；`pnpm dev:runtime` 显式使用项目目录、3212、已准备工具链及 UI dist，不启动 Vite、不自动安装资源。未运行该开发命令初始化真实项目库。
- CLI 示例：`ossia-desktop --home /absolute/library --engine-root /absolute/engine --port 3211 serve`；同一 home 下运行 `mcp`、`open '{}'`、`status`、`stop`。业务命令接受与 MCP 相同的 JSON 参数；doctor 可离线检查资源与发现信息，integrate 明确返回 P06 未实现。
- MCP 经真实 rmcp 客户端验证 initialize／tools/list／tools/call；14 个工具包含 13 个工作流工具和 doctor。stdout 仅承载 MCP 协议；MCP 断开不终止服务。
- `cargo test --workspace`、`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`；运行时离线测试覆盖 11 项鉴权／任务／监听／HTTP／事件回归，类型契约 3 项。默认忽略的原生套件单独运行，不计入离线通过项。
- 原生命令：`OSSIA_TEST_EXECUTABLE="$PWD/target/debug/ossia-desktop" OSSIA_TEST_LILYPOND_ROOT="$PWD/vendor/lilypond/2.26.0-darwin-arm64" OSSIA_NATIVE_EVIDENCE="$PWD/tests/fixtures/local/evidence/p04-native.json" cargo test -p ossia-runtime --test native_runtime -- --ignored`。证据分别记录于 `tests/fixtures/local/evidence/p04-native.json` 和 `p04-lifecycle.json`。两个原生测试覆盖三类输入自动预览→显式检查→提交→历史→恢复→三格式导出，以及并发启动、MCP 断开、SIGKILL、陈旧发现、编辑恢复、旧令牌失效、端口冲突和旧库字节不变。
- `pnpm check:types`、`pnpm check:contracts`、`pnpm baseline:local --check`、`pnpm test:local`（8 项）、`pnpm typecheck`、`pnpm check`、`pnpm build:web`。新增 CI 配置使用 Rust 1.89 检查生成差异与运行时回归；尚未触发远端 CI。本机 Rust 1.98.1，未单独运行 1.89；锁文件将 notify-types 固定到兼容最低版本的 2.0.0，interprocess 固定 2.4.4。

### 阻塞与下一阶段

仅实测 macOS ARM64。Windows 常规交叉检查在 bundled SQLite 编译处因缺少目标 C 头文件受阻；`LIBSQLITE3_SYS_USE_PKG_CONFIG=1 SQLITE3_LIB_DIR=/tmp cargo check -p ossia-runtime --target x86_64-pc-windows-msvc` 仅补充 Rust 类型检查，不链接、不验证 SQLite 或 Windows 运行。Linux、其他架构、真实 Windows pipe ACL／GUI 发行 stdio、签名资源与安装包均未原生验收；P00 Linux ARM64 门槛保留。

所有验证使用合成输入及临时库。未改真实用户库、未注入宿主、未发布。P05 接入已建立的 ticket／视图／SSE／任务／渲染与版本接口，并实现 sessionStorage、fragment 清除、浏览器谱面和独立管理窗口；不能把现有旧 SPA 当作新协议已接通。
