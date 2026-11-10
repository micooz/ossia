# P05 技术实现方案：侧边谱面与管理窗口

- 基准：`32c953c`；范围为完整 P05，本机实现与平台验收分开记录。
- 普通浏览器承载谱面，Tauri 承载管理；保留纸张、墨色视觉风格，覆盖 390px 与 1440px。

## 浏览器入口与状态

Vite 建立 index.html 和 admin.html 双入口，共享 dist；谱面不依赖 Tauri API。同源 /api/v1 接入 Rust，开发代理到 3212。使用生成协议类型，删除旧端点、令牌及模型作业流程。

单次交换 fragment ticket 后立即移除，sessionStorage 仅保存当前标签页凭据，地址保留非敏感 viewId。刷新恢复视图，过期由 Agent 重新打开。首次及重连先读视图快照、列表与任务，再从快照游标订阅携带 Authorization 的 fetch SSE；实例变化、游标失效与鉴权失效分别处理。异步请求按视图、文档与加载代次隔离，核心状态始终为事实来源。

## 核心接口补充

- GET /renders/{renderKey}/geometry：源码哈希、页面 viewBox、定位能力及可靠小节的 locator、systemIndex、谱表矩形，复用 Rust 算法。
- POST /documents/{id}/versions/{versionId}/render：返回任务，其结果为 RenderReference；使用不可变版本、调度器与缓存，不改变工作区或当前预览。
- 视图快照增加公开编辑摘要（作者、状态、是否属于当前视图），不暴露 editId。
- POST /jobs/{jobId}/reveal：仅打开成功导出任务记录的结果位置，不接受任意路径。
- DTO 归 ossia-types，生成 TS，同步契约、能力和阶段文档，不改数据库 schema。

## 谱面工作流

迁移导入、谱面、历史、对比及导出，删除指令提交、候选接受和五道门。首批支持 .ly/.xml/.musicxml/.mid/.midi，不开放 .mxl。导入跟踪任务，展示具体损失并携摘要确认，拒绝结果不可绕过。

SVG 经鉴权 fetch 后以 Blob 图片显示；覆盖层沿用 viewBox，通过 getScreenCTM 逆变换定位。支持多选、切换、清空及键盘选择，矩形限制对应谱表，跨页编号与压缩休止保留。源码变化清空选择，旧谱面明确标注且禁止选择；空白忽略，能力降级解释原因。

历史按 versionId 展示与对比。恢复和丢弃携 expectedSourceHash，冲突后同步，不自动接管或清空。恢复编辑由 Agent 处理，页面展示作者、dirty、恢复状态及诊断。按明确版本导出三格式，显示路径与打开位置。

## 管理窗口

admin.html 展示服务状态、当前目录、运行时/工具链版本、doctor 与启停。窄范围 Tauri commands 复用配置解析、ensure_runtime 和 IPC；CLI 只解析一次，GUI 遵守 --home > OSSIA_HOME > 默认。启动时连接/启动独立服务，刷新与 doctor 不重启。关闭窗口不停止服务，显式停止遵守 P04。GUI 不打开数据库。

收紧 capability/CSP，移除旧远程端点。P06 注入显示未接入。开发命令显式设置项目 home、3212、工具链及静态资源；旧库仅显示 P07 阻断诊断。

## 验证与交接

核心回归覆盖几何/版本归属、快照、不变工作区、凭据隔离及 reveal。Playwright 覆盖会话、双视图、多页缩放、键盘与选择、过期响应、失败编译和冲突。临时库真实引擎闭环：导入→CLI/MCP 编辑→页面预览→检查提交→历史对比→恢复→三格式导出。实测 macOS 管理窗口及独立服务共存，检查窄宽布局。

运行 workspace 测试、fmt、Clippy、生成检查、本地回归、typecheck、Web 构建和桌面编译。分别记录自动测试与原生证据。仅合成数据和临时库，不实施 P06/P07。更新索引、架构和 P05 交接；本机通过后标记本机实现完成、受前序门槛阻塞，保留 Linux ARM64 及其他未实测平台缺口。
