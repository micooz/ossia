# P07：五目标发布

- 状态：受阻（macOS arm64 本机发布切片已实施；Windows／Linux／macOS x86_64、真实宿主与正式签名分发未验收）。
- 前置：[P06](06-host-integrations.md)；所有前序阶段需有可复查交付，目标范围见[总索引](README.md)。
- 最终结果：以本地 Rust 运行时取代旧后端路径；产出自包含本体安装包和实际验收证据。

## 1. 阶段边界与发布矩阵

本阶段完成打包、安装态验证和旧代码退出。默认产出本地／CI 制品；没有发布指令时不上传公开 Release 或插件目录。插件资源随本体内置，不生成单独的插件发布渠道。

| 目标 | Rust target | 本体制品 | 工具链 |
|---|---|---|---|
| macOS arm64 | aarch64-apple-darwin | 对应架构 app／DMG | 官方 2.26.0 arm64 |
| macOS x86_64 | x86_64-apple-darwin | 对应架构 app／DMG | 官方 2.26.0 x86_64 |
| Windows x86_64 | x86_64-pc-windows-msvc | x64 安装器 | 官方 2.26.0 Windows 包 |
| Linux x86_64 | x86_64-unknown-linux-gnu | deb／AppImage | 官方 2.26.0 x86_64 |
| Linux arm64 | aarch64-unknown-linux-gnu | deb／AppImage | P02 原生构建工具链 |

Windows ARM64 不构建、不发布；不将模拟执行标成原生目标。Linux 首版基线按 Ubuntu 22.04 及对应 WebKitGTK 4.1 运行依赖构建验证，系统 WebView 等平台依赖要写清，不能把“自包含 LilyPond”扩写成“应用无系统依赖”。macOS 最低版本保留现有 12.0；Windows 基线为 Windows 10 x64 或更新版本。

这些 OS 基线是本次实施的默认构建／测试目标；安装所需系统组件以实际制品验证为准，开发依赖参考 [Tauri 官方前置条件](https://v2.tauri.app/start/prerequisites/)。

具体工具版本和源码校验和按通过前序验证的版本锁定；构建过程不自动升级 LilyPond 或改用滚动系统包。

## 2. 安装包与许可证材料

- 打包前构建谱面与 admin 入口、Rust 可执行文件、集成资源及对应平台完整引擎树。清单验证目标／版本／工具入口全部匹配。
- 不截取单个 lilypond 可执行文件；转换器、Python／Guile 等必要运行依赖、字体和重定位文件一起交付。
- 采用保留时间戳、执行权限和符号链接语义的归档步骤；解包后校验 mtime 规则，不在签名后 touch 引擎文件。
- 本体从安装资源找到引擎与页面，改变 cwd 或移走源码仓库仍可运行。安装时不下载工具链，不执行 npm install。
- 自有代码补齐 GPL-3.0-or-later 授权说明；收集第三方版权／许可、对应版本源码、补丁和构建脚本，生成可追溯的来源清单。
- CI 先构建和测试，再产生校验和与制品。对签名／公证所需外部材料据实处理：没有材料时可交付测试制品，但不能标记为已完成正式签名分发。
- 工具链验证、应用编译、实际安装与宿主交互分别记录。不能以其他 CPU 的交叉编译成功替代目标机验收。

## 3. 当前代码边界

服务端由本地 Rust 运行时提供，前端是 Vite + React SPA，桌面入口是 Tauri 管理窗口。仓库不包含 Next.js 后端、服务端模型调用或 `OSSIA_MODEL_API_KEY` 路径。

`tests/fixtures/local/` 保存合成输入、TS 观测、独立期望和机器证据；Rust 测试直接消费这些文件，不由实现覆盖参考结果。根命令启动当前 Rust 服务、Vite 页面和管理窗口，不依赖 Next.js。

## 4. 数据目录

运行时使用所选 home 中的 `library.sqlite`。默认位置为 `~/.ossia/`，项目开发脚本显式使用仓库 `.ossia/`。首次启动初始化当前库，后续启动复用同一目录。

## 5. 最终验收

每个目标使用实际安装包，在无法依赖仓库、全局 Node／Python／LilyPond 的环境检查：

- [ ] doctor 能识别正确资源，运行时与 GUI 分开启动，关闭管理窗口不影响任务。
- [ ] `.ly`、MusicXML、MIDI 导入，SVG／PDF／MIDI 真实生成；标准库／中文字体／多页正常。
- [ ] 断网仍可进行本地导入、排版、历史和导出；模型是否联网由宿主决定，不作为 Ossia 引擎依赖。
- [ ] 默认与项目内库正确隔离；单实例竞争、重启恢复、跨宿主选区隔离和同谱互斥均有效。
- [ ] 连续保存、语法错误、超时、取消、异常退出后，公共文件和历史可恢复。
- [ ] 在可用的 Codex／DSH 实际宿主中从安装态本体注入，打开侧边页面并完成编辑闭环；纯 CLI 环境只验收明确支持的能力。
- [ ] 五目标制品的资源、依赖与许可证材料齐全，Linux ARM64 有真实目标运行证据。
- [ ] 首次启动正确初始化 `library.sqlite`，后续启动复用同一目录。
- [ ] `pnpm check`、UI typecheck／build、Cargo workspace 测试、HTTP／MCP／浏览器回归全部通过。

完整闭环为：安装本体 → 注入宿主 → Agent open → 侧边导入／选小节 → edit 取得公共路径 → Agent 改文件 → 自动预览 → check → commit → 历史／restore → export。

无目标环境、签名材料或宿主能力时列明对应缺口，保留“未验收”状态；不得因此悄悄缩减已确定的平台范围或宣称全量完成。

## 6. 实施交接

### 6.1 本机交付

- `scripts/package.mjs` 与 `scripts/package.test.mjs` 按 `ENGINE.lock` 选择引擎树，拒绝 unbuilt 目标；生成 Tauri 安装资源，macOS arm64 产出 app、app zip、DMG、`SHA256SUMS`、`release.json` 和 `materials/`。
- macOS 制品位于 `artifacts/macos-arm64/`，当前版本 `0.1.0`。安装资源包含 `engine/`、`ui/`、`integrations/`、`licenses/`；`licenses/` 含 Ossia GPL 全文、完整引擎许可树、Rust／npm 运行时许可材料、`THIRD-PARTY-NOTICES.md` 和 `SOURCE-MANIFEST.json`。源码包为 `materials/ossia-0.1.0-source.tar.gz`（`git archive HEAD`）。
- 制品是 ad-hoc linker 签名、未公证的测试制品；`release.json` 明确 `notarized=false`，没有创建公开 Release，也没有上传插件目录。
- `doctor`、`serve` 和 DMG 临时安装副本均从安装资源定位引擎与页面，不依赖仓库 cwd 或 `--engine-root`／`--static-dir`。app 内 `.go` mtime 与执行位在打包副本中按 inventory 修复，未改变资源字节哈希。

### 6.2 检查与未验证边界

- `cargo test --workspace`、`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings` 通过；原生 runtime 2 项、engine 1 项、library 1 项、score 1 项通过。
- `pnpm install --frozen-lockfile`、`pnpm check`、`pnpm check:types`、`pnpm check:contracts`、`pnpm typecheck`、`pnpm build:web`、`pnpm test:package`、`pnpm test:ui`（7 项）和 `pnpm test:ui:native`（1 项）通过。Playwright 首次在沙箱内因系统 Chrome 启动被拒绝而失败，退出沙箱重跑后通过，不是页面断言失败。
- macOS arm64 已完成 app/DMG 实际启动、资源 doctor、HTTP 首页、DMG 挂载副本与校验和检查。CI 配置 `.github/workflows/release.yml` 尚未运行。
- macOS x86_64、Windows x86_64、Linux x86_64、Linux arm64 没有目标机安装／运行证据；Linux ARM64 引擎仍未构建和锁定，CI 中保留为显式 blocked。真实 Codex／DSH 安装态侧栏、Developer ID 签名、公证和公开发布仍未完成，因此本阶段只能标记“受阻”，不能标记已验收。
