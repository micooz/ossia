# P02：原生引擎与渲染产物

- 状态：受阻（前序门槛）；引擎模块已接入本地运行时，P00 Linux ARM64 原生验收仍未通过。
- 技术方案：[02-technical-plan.md](02-technical-plan.md)。
- 前置：[P01](01-rust-score-analysis.md)，并核对 P00 工具链探针；共享约定见[总索引](README.md)。
- 后续：[P03 文档、编辑与版本](03-library-editing-and-history.md)。

## 1. 阶段目标

交付 `ossia-engine`：在没有 Next.js、Node.js 服务或已启动 GUI 的情况下，完成转换、固定快照编译、诊断和产物管理。建立五目标工具链构建配方；安装包集成和最终平台验收由 P07 完成。

## 2. 工具链清单与资源定位

将单目标 ENGINE.lock 扩展为按目标描述的清单，固定 LilyPond／转换器版本 2.26.0，并记录每个产物的来源、SHA-256、归档格式、工具入口和 `engineBuildId`。后者包含目标架构与本项目构建标识，不只记录上游版本号。

| 目标 | 交付来源 |
|---|---|
| macOS arm64 / x86_64 | 对应官方包 |
| Windows x86_64 | 官方 Windows 包，识别实际 `.exe`／转换器入口和依赖 |
| Linux x86_64 | 官方包 |
| Linux arm64 | 同版本上游源码及固定依赖的原生构建产物 |

- 安装态根据自身可执行文件／bundle 资源位置定位工具链，不依赖仓库 cwd 或系统 PATH。
- 开发态允许显式传入工具链根目录，但必须通过相同版本和完整性校验；不会无提示调用系统其他版本。
- 保留目录结构、可执行权限和 `.go`／`.scm` 所需 mtime。时间戳修复如有需要在构建／开发产物上完成，不能修改签名后的安装资源。
- Windows 转换器所需 Python、Linux 的非系统运行依赖和字体必须纳入交付清单。用户不承担手工补依赖任务。
- 更新构建脚本可用开发 Node.js，发布运行时不调用这些脚本。

## 3. 核心接口与执行规则

实现 `Engine::verify`、`convert`、`compile`。调用者传入明确的工具链、不可变输入、编译选项、可取消上下文和可写产物根目录。返回 `ImportReport` 或 `RenderManifest`，不读写文档 head 或公共 `score.ly`。

### 3.1 导入转换

- `.ly` 读取后验证 UTF-8 和单文件依赖范围；标准库 include 可用，外部用户 include 返回 unsupported-source。复杂语义不能索引不等于文件不能编译。
- MusicXML／MIDI 在临时目录中使用固定输入名转换；上传文件名不能成为路径穿越入口，也不通过 shell 拼接执行命令。
- 原始输入由 P03 保存。生成 `.ly` 中的随机临时路径注释在首次入库前规范化，确保同一输入不因 scratch 路径不同而改变源码。
- 保留 clean／needs-confirmation／refused 三态。`Negative skip` 阻断且不可通过确认绕过；局部损失返回稳定摘要供明确确认后重试。
- 不将手写 `.ly` 的“无已识别声部”作为拒绝导入条件。

### 3.2 快照编译

- 每次运行使用独立临时目录、可写 HOME／缓存目录和输出前缀。退出、失败、超时和取消均清理临时资源；保留需要的脱敏日志与诊断。
- 调用原生进程，不通过 shell；取消和超时终止整个进程树，Windows 需要相应的进程组／job 管理。
- SVG 预览、PDF 和 MIDI 是明确的产物请求。SVG 成功即可支持后续提交，不强制附带 PDF／MIDI 都生成。
- 保留 SVG 与 PDF 使用不同引擎 pass 的现实约束。MIDI 必须有有效 `\midi` 块；基于 token／结构识别创建派生文本，不用“在最后一个右括号前插入”覆盖任意源码。
- 派生文本仅在编译临时目录使用。启用锚点或 MIDI 后仍保留原始源码映射，必要时附 source map；绝不写回公共 `score.ly`。
- 无法安全构造 MIDI 时，对该格式返回 unsupported-source；SVG／PDF 仍可使用。不生成空 MIDI 成功结果。
- 输出页按数值排序；缺页、无 SVG 或多个无法唯一归属的 PDF／book 输出显式返回不支持状态，不静默取第一个文件。

### 3.3 诊断与产物缓存

- 保留引擎原始诊断位置和 P00 验证后的规范化源码位置；未知位置允许为空，不能指向错误文件。
- 对外日志去除临时目录和机器路径；SVG 存储前规范化 textedit 回链，保留映射所需尾部。
- 缓存键为 `sourceHash + engineBuildId + optionsHash + requestedFormat`；每个文件另记自身字节哈希。
- RenderManifest 和实际文件均持久化，写入采用临时文件加原子替换；进程重启可恢复缓存，文件缺失／损坏时重建。
- PDF 可能包含生成时间与 UUID，不要求重复运行字节哈希一致。缓存复用依靠输入与构建标识，而非强制 PDF 字节确定性。
- 产物收集和路径解析限制在本次任务输出目录内，不允许清单中的绝对路径或 `..` 逸出。

## 4. 测试与退出条件

- [x] 验证错误版本、缺工具、损坏资源；macOS ARM64 的工具链与 scratch 中文／空格路径通过。
- [x] 本机三种输入覆盖转换、SVG、PDF、MIDI；局部损失确认与阻断性拒绝都有测试。
- [x] macOS 的真实进程夹具覆盖正常退出／崩溃／超时／取消，无遗留后代进程或 scratch；Windows Job Object 仅完成静态检查。
- [x] 本机源码无变化；中文和多页锚点映射到原始源位置，派生诊断位置通过核对。
- [x] 构建／选项／格式隔离、跨实例命中、并发同键互斥、损坏／缺失重建与清单路径限制通过。
- [x] 原生与替身结果分开；五目标配方、Linux ARM64 源码／打包脚本／20 项依赖归档校验和齐备，ARM64 构建未执行。

验证使用 `cargo test -p ossia-engine`；真实工具链测试使用显式测试环境变量指向已验证资源，普通测试缺少引擎时不得伪报通过。取消树清理需要真实进程夹具，不能只测试模拟的状态变化。

## 5. 实施交接

### 模块与交付

- `ossia-engine` 位于 workspace；共享导入／渲染／诊断 DTO 位于 `ossia-types`，不接数据库、HTTP 或 GUI。
- 入口为 `Engine::verify`／`from_resources`、`convert`、`compile` 和可取消、带总截止时间的 `RunContext`；P03 使用方式见 [crate README](../../crates/ossia-engine/README.md)。
- `vendor/ENGINE.lock` 现为 JSON 五目标清单。四个官方归档已实际下载、计算 SHA-256、解包并生成完整资源清单；`engineBuildId` 包含目标、项目配方版本和归档标识。实际工具入口、依赖和配方见 [vendor README](../../vendor/README.md)。
- Linux ARM64 仍为 `archive: null`、unbuilt 标识。`vendor/ENGINE.dependencies.json` 固定上游源码打包脚本和 20 项 Linux 依赖／字体归档的 SHA-256；`scripts/engine-linux-arm64.py --fetch-only` 校验通过，尚未生成 ARM64 二进制。该目标需要记录构建工具／基础环境、依赖审计及原生运行结果。
- 开发安装采用校验归档→暂存目录准备→资源清单／验证→替换，失败保留旧开发工具链。mtime 修复只发生在暂存目录；Rust 运行时不写工具链、不调用开发 Node 脚本、不静默使用系统版本。

### 公共行为与边界

- 三态导入保留损失与阻断信息。确认哈希绑定输入、转换源码、引擎和规范化损失，重试必须显式提供；拒绝结果不提供可入库源码。即使 Negative skip 位于进度日志之后，仍阻断且不能确认绕过。P03 负责执行确认后才建库的业务门槛，并保存原输入。
- 编译按格式独立返回产物／错误；SVG 是后续提交门槛，MIDI 不是。MIDI 只对一个显式、平衡、可安全识别的 score 块派生；不改速度或节奏，使用 no-print-pages 避免依赖 PDF pass。隐式 score、多个 score/book、bookpart 或不透明 Scheme reader 可使相关格式不支持。
- 静态 include 限定在经过校验的引擎标准库目录；用户依赖、动态 include 和已知外部图片／文件命令不支持。此判定不构成 Scheme 安全沙箱。
- 原始 UTF-8 字节不变；锚点覆盖在派生文件中用等长空白处理，插入 MIDI 文本无原始位置。SVG 回链与诊断经原文映射，标准库／无法确定的位置不指向用户源码；日志去除机器路径。
- 每次原生调用使用独立 cwd、HOME／cache／tmp；固定 C.UTF-8 locale、禁止 Python 字节码生成、清除 Python／Guile 路径覆盖。实测 `LC_ALL=C` 会导致中文临时路径下的 PDF 失败，UTF-8 locale 后通过。
- Unix 进程组与 Windows suspended spawn→Job Object→resume 实现全树终止；实际进程夹具在 macOS 覆盖正常、崩溃、超时、取消。总截止时间包含缓存锁等待；日志上限 16 MiB。
- 产物 CAS 与格式缓存清单由 P02 持久化；清单最后原子发布，OS 文件锁防止同键并发。新 Engine 实例可复用；哈希／大小损坏或文件丢失则重建。清单中的绝对路径、`..` 和越界符号链接被拒绝。P03 不应另写引擎或缓存实现。

### 检查与证据

- Rust 离线：P02 12 项回归，P01/types 24 项、桌面 7 项；另有文档测试。两个标记 ignored 的进程入口由生命周期回归显式启动，不代表跳过进程树验证。原生集成默认单独 ignored，显式调用缺资源会失败。
- 原生：`OSSIA_TEST_LILYPOND_ROOT="$PWD/vendor/lilypond/2.26.0-darwin-arm64" OSSIA_NATIVE_EVIDENCE="$PWD/tests/fixtures/local/evidence/p02-native.json" cargo test -p ossia-engine --test native_engine -- --ignored`。完整资源先复制到中文／空格目录后运行，7 个导入用例（6 个渲染、1 个阻断拒绝）覆盖 `.ly`、MusicXML、MIDI、所有输出格式、多页、BOM／CRLF／中文锚点、诊断映射、重启缓存及损坏／缺失重建；证据见 `tests/fixtures/local/evidence/p02-native.json`。
- 开发脚本：`pnpm test:engine` 3 项通过；四目标归档准备通过；Linux 配方 `--fetch-only` 校验通过。普通单元测试没有伪报其他平台原生验收。
- `cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、Windows 目标 `cargo check -p ossia-engine --all-targets --target x86_64-pc-windows-msvc`，以及现有 pnpm 基线／测试／类型／检查／Web 构建全部通过；`pnpm check` 为 0 errors、0 warnings。Python 配方语法、阶段文档链接和 `git diff --check` 通过。

### 阻塞与下一阶段

引擎模块已接入本地运行时和 macOS arm64 测试包，仍受 P00 Linux ARM64 原生门槛阻塞。其他 macOS 架构、Windows、Linux 的原生执行、Windows 进程树回收、签名资源和跨平台安装包仍未验收。静态检查与解包完整性不能替代原生证据。

P03 消费固定快照的 `RenderManifest`，在自身事务中额外核对编辑所有权、head 和当前文件哈希。失败编译不建立成功版本；独立的 PDF/MIDI 错误不得覆盖成功 SVG。重启后再次 verify 工具链、使用相同产物根目录即可恢复缓存；编辑凭据、数据库、原输入保存与工作区文件归 P03 管理。
