# DSH 会话中的 Ossia Agent 工作流成本

Agent 工作流的主要成本集中在四项：evidence 响应的边界、源码定位、局部图像和只在视觉层生效的标注。Ossia 的契约同时保留 reference 校验、源码 hash、编辑所有权和恢复基线保护。

## 会话证据

样本为 2026-11-13 的会话 `f0688538-4ff0-4392-8773-b1a58f672d80`。标注一轮耗时 167.717 秒、21 次模型步骤、24 次工具调用。工具耗时合计 19.382 秒，两次 check 合计 10.201 秒。

最直接的缺陷是单小节 evidence 响应：分页只包含 16 个音符，但 envelope 同时带回全曲 549 个演奏音符和 615 个源码事件。完整响应 207,917 字节，其中无界 job 占 204,936 字节。

普通 `ChordNames` 写法新增了 3 个发声音符；移除 `Staff_performer` 后，编辑版与原版的 performance 比对为 549 对 549、零差异。三次 Python 裁图都依赖猜测像素坐标。文档长查询因所有关键词必须同时匹配而返回零条，短查询 `ChordNames` 在同一索引中有 93 条结果。

## 工作流契约

`ossia_evidence` 只通过 `page` 返回有限音乐证据；job envelope 的 `result` 不携带音乐数组。`ossia_read_source` 的 `includeLineSpans` 返回可直接交给 `write_source` 的逐行 span。

`ossia_output` 支持小节范围或页面系统裁图。跨页请求按页生成文件；同页匹配范围合并。输出先写入同一父目录下的临时批次，取消或失败不会发布不完整目录。每个文件携带 page、实际 bounds、source hash、大小和内容 hash。HTTP 可按文件索引下载或定位。

guide 提供 markup、静音 ChordNames 和罗马数字标注方式。标注任务以编辑前版本做 performance reference，并要求检查局部图片；文字明确说明 performance 一致不能证明音符拼写、谱表归属、标注内容或位置正确。

restore 的 schema 区分目标 `versionId`、当前 `baseVersionId` 和当前 `expectedSourceHash`。缺少基线在请求解码时拒绝，执行中的并发和 workspace 检查保持。

文档搜索支持显式 `matchMode:"all"|"any"`，返回规范化关键词和零命中建议。选区保存保守音乐身份；标注和常规排版修改后，在新源码 hash 下重新解析 locator，音乐或谱表结构改变及分析不完整时清空。

## 测量边界

耗时数据来自一个会话，不能换算成固定的性能提升比例。多标签页同时编辑同一文档的长期压力场景仍未覆盖。
