# P06：Skill／MCP／插件本地注入

- 状态：待实施。
- 前置：[P05](05-browser-score-and-desktop.md)，接入参考使用 P00 的宿主探针结果；共享约束见[总索引](README.md)。
- 后续：[P07 发布与切换](07-release-and-cutover.md)。

## 1. 阶段目标

实现 `ossia-integrations`，让用户通过本体管理窗口或 CLI 接入 Codex／DSH。只有本体包含运行时和排版工具链；注入的 Skill、MCP 配置及薄插件均调用本体可执行文件。

不发布独立插件包、不复制引擎到宿主、不安装模型 SDK、不配置模型凭证，也不接管宿主 Agent 会话。

## 2. 资源与集成计划

在 `integrations/` 管理公共 Skill、Codex 本地插件模板和 DSH Host／Client 插件源资源。DSH 前端资源在本体构建时预编译，用户不需要在安装后构建插件源码。

集成模块提供 `detect`、`plan`、`apply`、`status`、`repair`、`remove`：

- detect 返回宿主安装位置、版本、配置根、可用模式和能力；找不到时返回未安装，不猜测另一个同名可执行文件。
- plan 明确宿主、目标 profile、数据目录、模式、将写入的条目及冲突，不修改文件。
- apply 只落实计划中的条目，记录管理 ID、模板版本、启动路径、写入内容哈希与目标位置。
- status 通过实际配置与可执行路径判定，不以标记文件存在代表接入成功。
- repair 比较现状后重新生成本工具拥有的内容；遇到用户已改写的条目报告冲突，不无条件覆盖。
- remove 只删除仍与管理记录对应的条目，保留宿主其他设置及所有乐谱数据。

模式为 `skill-mcp` 或 `plugin`，同一宿主／profile 只能启用一种。切换模式先撤销旧模式对应的受管条目，再安装新模式，失败时恢复配置变更；不做两个重复 MCP server 的并行兜底。

本体升级后，集成资源更新走相同的计划／应用流程。启动使用安装位置的绝对路径和显式 home 参数，不依赖 PATH、插件缓存的相对 cwd 或仓库源文件。

## 3. Skill 与 MCP

公共 Skill 使用 `ossia` 名称，明确以下工作流：

1. status／guide 核对本地库与实际能力，list／import 选择文档。
2. open 获取 viewId 和本机链接，在宿主侧边浏览器打开；没有该能力时返回可用链接。
3. context 读取该 view 的选区、源码哈希、定位能力和路径；不使用其他会话的最近选区。
4. edit 获得所有权后，用 Agent 自身文件工具直接修改公共 `score.ly`。
5. check 并查询 job，读取编译／音乐差异诊断；不能把 unavailable 差异解释为无变化。
6. commit 使用 editId、基版本和准确 sourceHash；冲突时重新读取状态，不自动覆盖或代替其他任务提交。
7. 报告实际版本、验证范围和导出路径；中断／放弃默认保留 dirty 文件。

Skill 不复制动态版本字段、当前库路径或工具输入 schema；guide 和 MCP 返回实际信息。若宿主文件沙箱拒绝访问源码路径，明确显示所需工作目录／权限，不以应用代写工具暗中绕过宿主限制。

MCP 配置调用本体的 `mcp --home <absolute-home>` 模式。命令路径和参数作为结构化数组写入，不拼 shell 字符串。注入器使用本体当前安装资源，不使用开发机特有路径。

## 4. Codex 与 DSH 适配

### 4.1 Codex

- skill-mcp 模式使用该宿主版本受支持的 Skill 目录和本地 stdio MCP 配置方式；注册名称固定为 ossia。
- plugin 模式将薄资源放入本体管理的集成目录，生成独立的本地 marketplace 根和插件 manifest，再通过受支持的本地 marketplace 入口注册。
- 优先使用宿主 CLI／官方安装入口，避免重写整份用户配置；不得向 Codex 的插件 cache 目录直接复制文件冒充安装。
- 宿主要求用户启用插件、信任本地目录或重启时，状态显示为待宿主完成，不写“已连接”。
- Skill 使用内置浏览器能力打开 P04 返回的 URL；插件不注入另一套谱面 UI，也不依赖控制 Codex 内部聊天 DOM。

适配依据：[本地插件和 marketplace](https://developers.openai.com/plugins/build/plugins)、[本地 MCP 插件](https://developers.openai.com/plugins/deploy/desktop-only)、[Browser](https://learn.chatgpt.com/docs/browser?surface=app)。实施时核对所支持宿主版本，不把公共目录提交资格作为本地注入前提。

### 4.2 DSH

- skill-mcp 模式通过宿主支持的 Skill 发现和 stdio MCP 配置接入，保持用户原有其他服务。
- plugin 模式提供预构建本地 bundle，声明正确的 Host／Client 入口和兼容范围；通过宿主本地目录安装机制接入，选择明确的 web／desktop profile。
- Host 使用当前公开的 MCP manager／配置机制注册 ossia stdio server，生命周期随插件启停释放。宿主依赖作为 peer，不能将另一份 DSH 核心打包进去。
- Client 使用已验证的 Sidebar Browser 打开 URL 接口；打开动作仍指向同一 SPA。工具返回链接与 viewId，不能仅凭浏览器标签页存在就猜测 Agent 当前选区。
- 可提供“在侧边打开谱面”动作，但必须通过宿主支持的交互将明确 URL／viewId 交给对应会话；不监听无公开保证的内部工具日志来强制弹窗。
- 明确 browser 功能是否启用；若宿主需启用其自带模块，计划中展示变更，只改对应受管配置，不替换整个 profile。

适配依据：[MCP](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/subsystems/mcp.md)、[本地安装与 bundle](https://github.com/deepseek-ai/deepseek-harness/blob/master/docs/user/develop/basic/publish.md)、[Sidebar Browser](https://github.com/deepseek-ai/deepseek-harness/blob/master/packages/client/ui-sidebar-browser/README.md)。

## 5. 管理窗口、测试与退出条件

管理窗口显示每个宿主的检测结果、模式、目标 profile／home、安装状态和计划操作。CLI 提供 JSON 计划、应用、状态、修复及移除入口。不存在的宿主不能显示注入成功。

- [ ] 无配置、已有其他服务、同名用户配置、损坏配置和只读配置均有测试。
- [ ] 重复 apply／repair 幂等，切换模式不重复注册，remove 不损坏用户其他条目。
- [ ] 路径带空格／中文、应用移动、升级和运行时版本不兼容可诊断。
- [ ] Skill 与插件指向安装态本体，不复制引擎，无全局 Node／Python 也能启动 Ossia；宿主自身依赖另行识别。
- [ ] 在独立 Codex／DSH 测试配置或 profile 中实际列出工具、打开侧边页面并完成一次编辑提交。
- [ ] 同时使用两个宿主共享同一测试库，只有一个持库实例，view 选区隔离且同谱写入串行。
- [ ] 宿主重启、插件卸载、页面关闭不会误删乐谱或终止其他客户端任务。

配置修改测试先使用临时目录和显式测试 profile。真实宿主验证记录实际版本和步骤；缺少宿主或公开入口变化时标记阻塞，不绕过宿主策略或伪报连通。

## 6. 实施交接（实施后填写）

- 兼容宿主版本、模板和安装入口：未实施。
- 管理 ID、配置计划与冲突测试：未建立。
- 实际宿主验证结果和限制：未执行。
- 下一阶段：P07 将本体和这些预构建资源一起打包，在全新安装态重做注入验证。
