# DSH MusicXML 导入契约

## 会话证据

DSH Desktop 会话「删除并重新导入心湖乐谱」：
`5181990c-8657-4e57-82d9-b7e12977f56f`，2026-11-13 12:46–12:51
（Asia/Shanghai）。本轮共 77 次模型步骤、86 次工具调用，耗时约 290 秒。
会话记录位于
`~/.dsh/sessions/--Users-micooz-Documents-deepseek-harness-default-workspace--/session-5181990c-8657-4e57-82d9-b7e12977f56f/session.v4.jsonl.zstd`。

原始 MusicXML 的直接导入返回
`cannot safely override point-and-click in opaque source`。导入被阻断的结构是
musicxml2ly 为方框排练记号生成的 Scheme 内嵌 `#{ … #}`，不是 MusicXML
文件损坏。会话中的手动转换、删除 `pointAndClickOff`、改写 square 命令和补
MIDI 都由这条链路触发。

## 当前契约

- 结构扫描识别 Scheme 内的嵌套音乐、字符串和注释，保留转换器生成的排练记号定义。
  point-and-click 的派生替换保持字节长度，原始源码和定位映射不受影响。
- MusicXML 转换激活转换器生成的单 score MIDI 模板，保留模板中的初始速度。
  不使用生成独立播放 score 的 `musicxml2ly --midi`。
- 读取器仍保守拒绝不支持的语法、嵌套输出块、文件依赖和 point-and-click
  开关；递归嵌套上限为 64。多输出归属检查和依赖校验保持。
- 渲染缓存包含派生版本，结构扫描规则变化后不会复用旧派生产物。

## 验证依据

合成回归文件 `tests/fixtures/inputs/xml-rehearsal.musicxml` 包含方框排练记号、
96 BPM 和一个 C4 全音符。原生库测试验证直接导入、SVG 定位链接、MIDI
音高和 625,000 微秒每拍的 MIDI 速度事件。引擎原生测试覆盖 SVG、PDF、MIDI、
重启缓存与损坏产物恢复。

会话原始文件
`/Users/micooz/Documents/Codex/2026-11-07/pa/outputs/心湖-钢琴谱-节拍整理版.musicxml`
在独立临时库中直接导入：2 页、334 个发音音符、96 BPM，保留 Scheme 标记定义
和源码定位，PDF 导出成功。1600 像素宽 PNG 的 SHA-256 与会话最终输出一致：

- 第 1 页：`ba9e3e868bc64a72e0fc2ac6903f22c30a879b5d8b2acc24756d252f20bb46eb`
- 第 2 页：`f0355659e3acaafbb43d35c03ae0b48f918afa5971991af9742cf1d37d9cb7d8`

这些结果证明本次文件的导入、播放速度和输出一致性；源码音乐分析仍可对
`mark`、自定义 `U`/`D` 等构造报告 partial。音符数量不单独证明音乐内容正确。
