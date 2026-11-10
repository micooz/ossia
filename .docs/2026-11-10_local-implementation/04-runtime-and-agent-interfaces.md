# P04：本地运行时、HTTP、CLI 与 MCP

- 状态：待实施。
- 前置：[P03](03-library-editing-and-history.md)；模块、DTO 与错误码见[总索引](README.md)。
- 后续：[P05 页面与管理窗口](05-browser-score-and-desktop.md)。

## 1. 阶段目标

实现 `ossia-runtime`，让浏览器、CLI 和 Agent 通过同一个本地实例操作库。发布入口支持 headless serve／mcp 模式，未初始化 Tauri 也能完成完整工具闭环。GUI 的改造留到 P05。

本阶段提供命令、协议和集成测试，不注入真实宿主配置，不占用旧库，也不删除旧 Next.js 服务。

## 2. 启动、发现与调度

- 在桌面可执行文件的 main 入口先分派 `serve`、`mcp`、`doctor`、`integrate` 和文档／任务 CLI，再进入 GUI 分支。不能为使用 MCP 强制先打开窗口。
- 实现 `ensure_runtime(home)`：获取 OS 文件锁后启动持库实例，或通过本地端点连接既有实例。锁在进程整个生命周期持有；PID／端点文件只是发现信息，不替代锁。
- Unix socket 使用当前用户访问权限；Windows named pipe 限制为当前用户，名称由规范化 home 推导。两个同时启动的客户端最终只能产生一个写库实例。
- 握手检查协议主版本和 schema。旧 schema／不兼容实例返回明确错误，不接管、不扫描其他库，也不自动终止进程。
- HTTP 默认绑定 `127.0.0.1:3211`，开发脚本显式使用 3212；测试允许端口 0。显式端口被占用时报错，不递增猜测。IPC 总是返回本次实际 webBaseUrl。
- `serve` 独立于 MCP／页面存活；显式 stop 拒绝新任务、取消并回收进程树后退出。默认无开机启动、无自动更新或空闲关闭。
- 全实例最多两个引擎任务并行，同文档最多一个编译运行；自动预览将待运行任务合并为最新快照。显式 check 的结果必须能按 jobId 查询，不能被合并后无声消失。
- 文件保存使用 300 ms 防抖，兼容原子替换／重建 watcher。编译前读取稳定字节快照；事件丢失时在 context／check／commit 入口重新读取文件。
- Job 持久状态接入 P03。重启将 running／queued 作业记为 interrupted，不声称旧子进程仍在执行或自动重放 Agent 修改。

## 3. 视图、鉴权与事件

- `open_view` 返回随机 viewId、短期单次启动 ticket 和包含该 ticket 的 URL。ticket 放 URL fragment，不进入请求 URL；页面交换后从地址中移除。
- 启动 ticket 有效期为 5 分钟，只能交换一次；页面令牌仅保存在该标签页 sessionStorage，有效期为 24 小时或至实例退出。失效后由 Agent 重新 open，而非无鉴权恢复。
- 页面持有的令牌绑定 viewId；可在该视图中选择文档，但不能读写其他视图的选区或编辑凭据。
- HTTP 校验实际 loopback Host、允许的 Origin 和 bearer 令牌；生产同源，开发仅显式允许配置的 Vite origin。拒绝通配 CORS、任意 Host 和远端接口暴露。
- `/health` 只返回存活与协议标识；不包含 home、源码路径或启动凭据。其他数据操作走鉴权入口。
- 事件以 SSE 流提供，使用可携带 Authorization 的 fetch 流读取。封包为 `instanceId, seq, type, viewId?, documentId?, sourceHash?, payload`。
- 初次和每次重连都先获取快照，再接增量事件；实例变化／游标过期时重新同步。事件不承担永久审计，不广播 ticket、令牌或其他视图的 editId。

## 4. HTTP 与工具映射

HTTP 使用 `/api/v1`，错误为 `{error:{code,message,detail?}}`；成功响应由共享 DTO 生成。冲突／dirty／busy 为 409，输入错误 400，鉴权失败 401／403，不存在 404，编译／不支持输入 422，已入队任务 202。

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

MCP 工具保留架构方案中的 `ossia_guide/status/list/import/open/context/edit/check/commit/history/restore/export`，再提供 `ossia_job` 查询或取消任务：

- `ossia_open(documentId?)` 返回 viewId 和可打开链接；context 必须带该 viewId。
- `ossia_edit(action, documentId, viewId, baseVersionId?, expectedSourceHash?, editId?)` 明确动作，不以可选字段隐式判断接管。
- `ossia_check(documentId, editId?, expectedSourceHash)` 返回作业引用；`ossia_commit` 必须携带 editId、baseVersionId、expectedSourceHash 和摘要。
- 长操作返回 jobId；结果／错误均可经 `ossia_job` 读取。取消是幂等操作，终态不会退回 running。
- MCP 标准输出只容纳协议消息；引擎和运行时日志写 stderr／本机日志，不污染 stdio。
- CLI 为同样能力提供 JSON 输出和非零错误退出码。mcp 是 stdio 到 runtime 的薄适配，不自建第二套库或引擎任务逻辑。

在 P04 建立 `pnpm generate:types`，从 ossia-types 生成前端协议类型；CI 检查生成后无差异。协议样本与每个 MCP 工具的输入 schema 一起固定。

## 5. 测试与退出条件

- [ ] 并发启动只出现一个实例；进程崩溃、陈旧端点、端口占用和版本不兼容可诊断。
- [ ] 未运行 GUI 时，CLI／MCP 可导入、open、edit、check、commit、history、restore、export。
- [ ] 实际 MCP 客户端可完成 initialize／tools/list／tools/call；普通日志不污染传输。
- [ ] 两个 view 的文档选择互不串用；过期 sourceHash／ticket／令牌不能继续写状态。
- [ ] watcher 连续保存、删除重建、事件丢失、事件重连和旧编译完成后到达均有回归测试。
- [ ] 跨平台锁、socket／pipe、取消树和无显示环境启动分别验证，未验证平台明确保留。
- [ ] Rust 核心测试、HTTP 合约测试、MCP 集成测试、前端类型生成检查通过。

## 6. 实施交接（实施后填写）

- 可执行入口、发现／鉴权／事件协议：未实施。
- CLI 和 MCP 示例、类型生成命令：未建立。
- 真实引擎完整闭环及进程验证：未执行。
- 下一阶段：P05 只消费本阶段接口，不在页面补版本、锁或编译判断。
