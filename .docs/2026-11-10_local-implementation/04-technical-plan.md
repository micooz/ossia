# P04 技术实现方案：本地运行时与 Agent 接口

- 基准：`198bc18`；本机 headless 导入→open→编辑公共文件→check→commit→history→restore→export。
- 实施状态见 P04 交接；P00 Linux ARM64 门槛保留，不将本机实现写为整体已验收。

## 运行时

新增 ossia-runtime，统一持有 Library、Engine、任务队列、视图、事件与 watcher。Tokio／Axum／notify；interprocess 提供当前用户 Unix socket／Windows named pipe。复用持库锁裁决并发启动；原子发现文件、规范化 home、协议与 schema 握手；陈旧端点仅所有者清理。默认 HTTP 127.0.0.1:3211，开发 3212，测试端口 0；冲突直接失败。

全实例最多两个引擎任务，同文档一个；检查复用 queue_check/run_check 固定快照。300ms 防抖，只合并排队预览；显式检查始终可查询。context/check/commit 重读文件补偿 watcher 丢失。取消同时作用于任务与 RunContext，stop 等待子进程回收；重启沿用 interrupted 恢复。

## 协议与入口

桌面 main 在 Tauri 前分派 serve/mcp/doctor/stop 与文档任务 CLI；headless 构建无 GUI 依赖。HTTP 实现 P04 路由，IPC 承载路径导入、open 和控制。open 创建独立视图、5 分钟单次 fragment ticket；exchange 返回 24 小时绑定视图令牌。校验 Host／Origin／bearer，health 不泄漏路径。选区绑定视图、文档和哈希，编辑核对 owner；只有受控 IPC 持原凭据可恢复转移。

SSE 快照带游标，重连增量；实例变化／缓冲区溢出重新取快照，不广播凭据。rmcp stdio 提供全部 14 工具，CLI 共用请求与领域错误，stdout 不输出日志。integrate 留 P06，doctor 提供诊断。共享 DTO 从 ossia-types 生成 TS，并检查生成差异。

## 核心与开发

库增加文档／版本归属的渲染与产物查询；renderKey 是源码、引擎、选项组合。按 versionId 导出不可变源码／PDF／MIDI，默认下载目录 Ossia 子目录，唯一文件名且不覆盖显式目标。导入／恢复／导出进入同一调度器，传输不直接写 SQLite。Rust 服务静态 dist，开发命令显式设置 home／3212／工具链。页面迁移留 P05，宿主注入留 P06，不发布、不清理用户数据，测试只用临时库。

## 验证

进程覆盖并发启动／崩溃／陈旧端点／端口冲突／不兼容／无 GUI／MCP 退出后服务存活／stop。协议覆盖全部路由、鉴权、双视图、ticket、SSE 重连。调度覆盖限额、连续保存、删除重建、事件丢失、合并、固定快照、取消与旧结果。真实 rmcp 客户端及本机原生引擎用三类合成输入完成版本和三格式导出。运行 workspace 测试、fmt、Clippy、生成差异及 pnpm 基线／类型／检查／Web 构建；交接记录实际证据、平台缺口和 P05 入口。
