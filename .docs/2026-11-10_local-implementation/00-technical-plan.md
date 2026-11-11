# P00 技术实现方案：本机基线与可行性探针

本方案对应 P00。本轮交付合成样本、可重建的 TS 对照结果、macOS 引擎探针、宿主接入证据和共享契约示例。Linux ARM64 不启动虚拟环境，保留真实构建与运行验收阻塞。

## 实施顺序

1. 建立 `tests/fixtures/` 清单与合成输入；不使用个人乐谱。
2. 增加 `pnpm baseline:local`：调用现有纯 TS 分析函数，导出稳定 JSON；`--check` 在临时目录比较参考输出。根路径依据脚本位置解析。
3. 分别保存 TS 实际行为与人工／引擎核验期望。记录压缩休止、尾部 score block、未知 staff、无效页号及代理项拆分差异，不修改产品路径。
4. 增加 `pnpm probe:engine`：临时目录运行 2.26.0 转换器与 SVG／PDF／MIDI，核对诊断列、原始源码锚点、标准库和外部 include。
5. 增加 `pnpm probe:host`：独立 loopback 页面验证短期启动凭据、同源令牌、事件流和 viewId 隔离；在 Codex／DSH 实测，不修改宿主配置或安装插件。
6. 保存共享 JSON 示例、机器结果与基线报告，运行基线、类型和静态检查，更新 P00 交接。

## 数据与对照

样本覆盖双谱表、多声部、多页、转换器 `% N`、压缩休止、手写命名变量／内联 Staff、relative／absolute、Scheme／markup／标准库 include、中文／emoji／CRLF／BOM、同一行多个小节及不支持语法。MusicXML 覆盖正常、局部损失和 Negative skip；MIDI 覆盖正常、截断、变长 meta、running status 和零速度 note-on。

源码保留原始 UTF-8 字节；SourceSpan 使用字节半开范围，行列按一基 Unicode 标量。保留旧 lexer 的原始 token 观测，规范化时只合并拆开的相邻同模式 RAW 代理项；每个规范化范围必须可反切原文。

基线输出包含 token、声部／小节范围、谱表顺序、SVG 几何、定位、音高、MIDI 和导入报告。固定排序、路径占位、格式版本和样本 ID；可重建数据不包含随机临时路径、时间或凭据。

## 验收与退出

- 连续重建、换 cwd 结果一致，`--check` 可检测漂移；坏输入不阻止其他样本输出。
- 字节无损往返、Unicode 反切、音高／MIDI、转换损失／拒绝、跨页定位及锚点均有可核验样本。
- 宿主记录版本、截图／事件和失败原因；文档声明与运行验收分别标记。
- 运行新增检查、`pnpm typecheck` 和 `pnpm check`，已有失败单列。
- 本轮止于 P00 本机交付；未完成 Linux ARM64 真实验收时阶段为“受阻”，不将 P01 标记已集成。不发布。
