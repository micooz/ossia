# P07 技术实现方案：macOS arm64 发布切片

- 日期：2026-11-10
- 阶段状态：受阻（macOS arm64 本机发布切片完成；五目标与真实宿主未验收）
- 基线：`32683b7`

## 1. 范围与判定

本轮领取 P07 中可在本机闭环的 macOS arm64 发布切片：安装资源、app/DMG、许可证与来源材料、旧架构退出和五目标 CI 骨架。完整的 P07 仍要求 macOS x86_64、Windows x86_64、Linux x86_64、Linux arm64 的原生安装与运行证据，以及真实 Codex／DSH 安装态侧栏闭环；这些目标本轮没有环境或引擎归档，不能标记验收。

本机没有 Developer ID 身份，产物按 ad-hoc linker 签名的测试制品处理，不公证、不创建公开 Release。`vendor/ENGINE.lock` 的 `linux-arm64` 仍为 unbuilt，打包脚本必须拒绝该目标。

## 2. 打包与安装资源

- 新增 `scripts/package.mjs`，按 `darwin-arm64`、`darwin-x86_64`、`mingw-x86_64`、`linux-x86_64`、`linux-arm64` 映射 Rust target 和 Tauri bundle。
- 打包前校验目标引擎树、UI dist、集成资源；拒绝缺失、损坏或 unbuilt 的引擎。生成临时 Tauri 配置，把 `engine/`、`ui/`、`integrations/`、`licenses/` 作为安装资源。
- macOS 使用 `tauri build --target aarch64-apple-darwin --bundles app --no-sign`。在 app 副本内按 `resources.json` 恢复执行位，并确保 `.go` 不早于对应 `.scm`；不改变任何资源内容哈希。
- 用 app 内可执行文件、且不传 `--engine-root`／`--static-dir`，运行 `doctor`、启动 `serve` 并读取首页。挂载 DMG、复制 app 到临时目录后重复 doctor，确认资源不依赖仓库 cwd。
- 用 `ditto` 生成 app zip，用 `hdiutil` 生成带 Applications 链接的 DMG；输出 `artifacts/<target>/`、`SHA256SUMS` 和 `release.json`。
- 打包脚本默认要求干净工作树，使 `git archive HEAD` 源码包与二进制对应；`--allow-dirty` 只用于开发诊断，不生成最终制品。

## 3. 许可证与来源材料

- 根 `LICENSE` 使用 GPL-3.0-or-later；安装资源包含 GPL 全文、引擎 `licenses/`、`THIRD-PARTY-NOTICES.md` 和来源清单。
- `SOURCE-MANIFEST.json` 记录 commit、目标、引擎 build/archive/source 校验和、Rust 与 npm 运行时依赖、许可证标识和缺失许可文本。
- Rust 依赖从 `cargo metadata` 与 `cargo tree -e normal` 收集，npm 从 `pnpm licenses list --prod --json` 收集；能定位到许可文件时复制全文，否则记录 SPDX 与来源，不伪造文本。
- 自有源码用 `git archive HEAD` 生成 `ossia-0.1.0-source.tar.gz`。引擎、Rust registry 与 npm 上游源码只记录 URL 和 SHA-256，作为可追溯来源承诺，不重复打入制品。

## 4. 数据目录

- 运行时使用所选 home 中的 `library.sqlite`。
- 当前 schema 标识未知或不匹配时返回 `runtime-incompatible`。

## 5. 旧路径退出与 CI

- 删除 `packages/ly` 和 `scripts/local`，移除只服务于 TS 对照的 pnpm 命令；保留 `tests/fixtures/local` 的输入、observed、manifest 和证据，Rust 测试继续直接消费已有期望，不由 Rust 覆盖参考。
- 更新 fixture README、索引、现行命令、旧架构文档顶部说明和检查忽略规则。历史 ADR 保留正文，只标注被本次本地架构替代。
- 新增 `.github/workflows/release.yml`，手动触发，五目标矩阵仅上传 workflow artifact；macOS、Windows、Linux x86 使用官方引擎归档，Linux arm64 任务显式 blocked，等原生引擎构建和锁定后再启用。不签名、不创建公开 Release。

## 6. 验证

- Rust：fmt、workspace 测试、Clippy、既有原生 runtime／engine 测试。
- 数据：临时目录初始化当前 `library.sqlite`；未知当前 schema 返回 `runtime-incompatible`。
- 前端与仓库：类型／契约生成检查、typecheck、Web build、Playwright 浏览器与原生场景；搜索确认没有现行 `packages/ly`、`scripts/local`、`OSSIA_MODEL_API_KEY` 或 Next 路由。
- 打包：macOS arm64 app/DMG、资源哈希、执行位、mtime、无 override doctor、HTTP 首页、GUI 启停、DMG 临时安装、源码包和校验和。

## 7. 未验证边界

macOS x86_64、Windows、Linux x86_64、Linux arm64、Linux ARM64 引擎构建、真实安装态 Codex／DSH 侧栏、Developer ID 签名与公证、公开 Release 和插件目录发布均未完成。CI 配置本轮不运行；交叉编译或静态检查不能替代目标机证据。
