# P05：侧边谱面与 Tauri 管理窗口

- 状态：浏览器谱面和 Tauri 管理窗口已接入本地运行时与 macOS arm64 测试包；受 P00 Linux ARM64 前序门槛阻塞，未验收。技术方案见 [05-technical-plan.md](05-technical-plan.md)。
- 前置：[P04](04-runtime-and-agent-interfaces.md)；产品交互约束见[架构方案](../2026-11-10_local-architecture-plan.md)。
- 后续：[P06 宿主集成](06-host-integrations.md)。

## 1. 阶段目标与入口

将现有 React SPA 改为普通本机网页，并将 Tauri 窗口改为独立管理入口。用户在 Agent 侧边浏览器中操作谱面，页面不启动 Agent、不提交模型请求、不增加人工接受候选流程。

Vite 使用两个 HTML 入口：`index.html` 为谱面，`admin.html` 为管理窗口，产出同一 dist。Tauri 的窗口地址和 devUrl 指向 admin 入口；Rust HTTP 仅将谱面入口作为默认页面。管理逻辑和 Tauri API 不能进入谱面入口的启动依赖。

## 2. 页面结构

- `App.tsx/state.ts/api.ts/endpoint.ts` 使用本机同源接口、视图上下文与事件订阅，不包含 API 部署发现、VITE_API_TOKEN 或模型作业流程。
- 保留 ImportPanel、ScoreView、CompareView、HistoryPanel、ExportBar 中适用的交互，调整为新 DTO。移除 EditPanel 中指令提交和 GateReport 中五道门接受流程。
- 状态至少区分：连接中、无文档、导入确认、可浏览、编译中、编译失败但有旧谱面、dirty、编辑占用、待恢复、版本已提交和连接失效。
- 后端状态是事实来源；React reducer 只表达页面状态。不能因页面切换或重连将 dirty／编辑锁重置。
- 页面只接入 Rust 运行时。

## 3. 页面具体行为

### 3.1 会话和导入

- 从 fragment 读取启动 ticket，调用 exchange，清除地址中的 ticket；按 P04 保存本标签页令牌。页面无凭据时展示由 Agent 重新打开的说明，不猜测其他 viewId。
- 展示当前库的文档列表和 `.ly/.xml/.musicxml/.mid/.midi` 文件入口；只宣告 P02 实际验证的扩展名和格式能力。`format=mxl` 明确返回不支持，压缩 MusicXML 不由页面绕过。
- 导入损失展示具体条目，再携带相应摘要确认；阻断性错误没有“仍然导入”按钮。
- 切换文档只更新本 view，清空旧选区；其他页面和会话不随全局“最近文档”被动切换。

### 3.2 谱面、小节和选区

- 通过带令牌的请求获取 SVG，以引擎 viewBox 建立覆盖层。CSS 像素使用 SVG 的坐标变换映射为 user units，不依赖固定缩放比例。
- 点击调用 locate，再按同一 sourceHash 提交选择；支持相同小节切换选中、多选和清空。
- 页号、谱表、小节范围均遵守共享位置约定。高低谱表选择框只画到对应谱表，跨页编号不重置。
- 未命中为空白点击，不展示为系统故障；anchor-only／none 区域展示能力说明，不假造小节选择。
- 源码变化即使小节号相同也使旧选区失效。清空旧选择并展示需要重新选择的状态，不把旧坐标关联到新谱面。
- 提供可键盘操作的选择／清除入口和可读的选区摘要，不把鼠标点击作为唯一可操作方式。

### 3.3 编译、工作区和历史

- 文件变化后自动更新预览。显示正在处理的 sourceHash 和最后成功结果；旧结果只能作为明确标注的旧谱面保留。
- 编译失败展示源码位置和诊断，用户仍可切页查看上次成功结果；成功事件只替换对应最新快照。
- 页面显示编辑者、dirty 和待恢复状态。Agent 提交成功后刷新版本与 head，不显示人工接受候选按钮。
- 前后对比按 versionId／快照明确选择，不把内容相同的恢复节点合并成一个版本。
- 历史回退和丢弃工作区使用 P03 既定操作与 expectedSourceHash。dirty／busy 冲突展示核心错误，不由页面擅自先清空文件。
- PDF／MIDI／源码导出调用本地核心，展示实际路径和打开位置动作；不依赖宿主 webview 允许浏览器下载。

## 4. 管理窗口

admin 入口仅展示：运行／停止状态、所选数据目录、运行时与工具链版本、doctor 结果、宿主集成状态和操作入口。

- GUI 用 Tauri IPC 连接相同 RuntimeInfo 和配置操作；不持有自己的数据库写句柄。
- 默认不打开谱面窗口，不注册开机启动。关闭管理窗口不应误杀正在运行的 Agent 任务；显式退出服务按 P04 关闭。
- 宿主注入入口通过 P06 集成命令工作；未安装、待启用或待重启时显示实际状态，不显示假成功。
- 更新 Tauri capability／CSP 时只授予管理窗口实际需要的能力，不给网页谱面注入 native bridge。
- 开发流程使用项目 `.ossia/` 和开发端口；MVP 只识别当前 `library.sqlite`，其他文件不影响启动。

## 5. 测试与退出条件

- [x] 普通浏览器中无 Tauri globals 也能打开谱面；Tauri 窗口只出现管理页。
- [x] 两个 view 同时开不同乐谱，选择与状态互不串用；刷新、重连、runtime 重启均可恢复或明确要求新链接。
- [x] 多页、缩放、上下谱表、小节多选、键盘操作和 stale-selection 覆盖浏览器测试。
- [x] 失败编译不抹掉最后成功结果，旧事件不覆盖新谱面，dirty／busy 冲突不被吞掉。
- [x] 用真实引擎完成导入→Agent 文件编辑→页面预览→提交→历史→恢复→导出，无人工接受步骤。
- [x] Tauri 打开、关闭与 headless runtime 共存时不会启动第二个写库实例。

运行 UI typecheck／构建、核心回归和浏览器交互测试。截图或真实页面验证至少覆盖窄侧栏和宽面板两种布局；不能只用组件测试代替实际谱面坐标验证。

## 6. 实施交接

### 6.1 入口与实际变更

- `index.html` 是谱面入口，`admin.html` 是独立管理入口；两者由同一 Vite 多页面构建产出。谱面按同源 `/api/v1` 连接 Rust，开发代理固定到 `3212`，启动路径不加载 Tauri API。只有管理入口加载 native bridge。
- 谱面完成 fragment ticket 单次交换、立即移除 ticket、`sessionStorage` 标签页凭据、带 `Authorization` 的 fetch SSE、先快照后订阅、重连和实例／游标／鉴权失效处理。请求按 view、document 和 generation 隔离，旧响应不会覆盖当前视图。
- 谱面改用鉴权 SVG Blob 和同 viewBox 的交互覆盖层；点击经 `getScreenCTM()` 逆变换到 SVG user units。服务端几何提供可靠小节的 locator、systemIndex 和谱表矩形，页面不复制定位算法。
- 导入、多选、键盘选择、旧谱面、历史节点、双版本对比、恢复、丢弃和显式版本导出已接入 P04 协议；界面不提供指令提交、候选接受或五道门流程。普通编译日志默认折叠，警告与错误保持可见。
- Rust 提供鉴权几何查询、不可变版本渲染、公开编辑摘要和 hash-verified 导出位置动作。历史渲染使用固定版本快照与既有调度／缓存，不改公共源码、head 或当前预览；reveal 只接受成功导出任务的结果路径。
- Tauri 展示服务状态、数据目录、运行时／引擎版本、doctor、启停，以及 P06 的宿主检测与集成操作入口。CLI 参数统一解析一次；GUI、serve 和 MCP 遵守 `--home`、`OSSIA_HOME`、默认值顺序，管理 GUI 不打开数据库。开发脚本显式传入项目 home、`3212`、工具链和静态资源。
- 首批导入开放 `.ly`、`.xml`、`.musicxml`、`.mid`、`.midi`；`.mxl` 当前转换链未验证，接口和页面均明确拒绝，文档不把它列为已开放能力。

### 6.2 已执行检查

- `cargo fmt --all -- --check`、`cargo test --workspace`、`cargo clippy --workspace --all-targets -- -D warnings` 与 `cargo build -p ossia-desktop` 通过；`pnpm build:desktop` 另行构建并打包出 `target/release/bundle/macos/Ossia.app`（20.04 MiB）。
- 验证：`cargo test --workspace`、`pnpm check`、`pnpm check:types`、`pnpm check:contracts`、`pnpm typecheck`、`pnpm test:engine`、`pnpm test:ui`、`pnpm test:ui:native` 和 `pnpm build:web`。
- `pnpm test:ui` 的 7 个 Playwright 浏览器场景通过，覆盖 ticket／刷新／重连、双视图隔离、多页与缩放、上下谱表、多选与键盘、stale-selection、失败编译保留旧谱面、过期响应及 dirty／busy 冲突。
- `pnpm test:ui:native` 的真实引擎场景通过。另行重跑默认忽略的原生套件，`native_crash_stale_endpoint_edit_recovery_port_conflict` 与 `real_mcp_cli_http_native_loop_and_runtime_lifecycle` 共 2 项通过。
- Impeccable detector 对 `packages/ui/src` 返回空结果。390px 与 1440px 截图检查了长路径、禁用态、焦点外观、键盘入口和谱面坐标；没有用组件测试代替真实页面。

### 6.3 原生证据与平台边界

- 本机 macOS ARM64 的运行时证据在 Git 历史中的 `p05-native.json`：三类合成输入均完成 3 个版本与 `.ly`／PDF／MIDI 导出；真实 LilyPond 闭环确认自动预览、固定快照编辑检查、版本对比、恢复和导出。
- 双页合成谱面的浏览器证据在 `.impeccable/review/p05-native.json`，覆盖 2 页、每页 4 个可靠区域、`390`／`1440` 宽和 `100%`／`150%` 缩放，记录 `3` 个版本与三种导出。实际截图在 `.impeccable/review/desktop.png` 和 `.impeccable/review/mobile.png`。
- macOS 本地 `.app` 已实际打开 `admin.html`；doctor 显示运行时与 LilyPond 可用。关闭窗口后，同一 headless 运行实例继续存活并保持服务，未启动第二个写库实例。
- macOS ARM64 已验证。Linux ARM64 仍受 P00 原生构建与运行门槛阻塞；Windows、Linux x86_64、macOS x86_64、跨平台安装包、Codex／DSH 实际宿主侧栏和打包资源签名均未完成原生验收。macOS arm64 测试包的启动冒烟不能替代其他平台验收。
