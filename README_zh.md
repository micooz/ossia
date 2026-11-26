<div align="center">
  <img src="website/assets/cover-zh.png" alt="Ossia：AI 原生乐谱制作软件">
  <p><strong>体验极佳的 AI 原生乐谱制作软件</strong></p>
  <p>
    <a href="https://github.com/micooz/ossia/actions/workflows/check.yml"><img src="https://img.shields.io/github/actions/workflow/status/micooz/ossia/check.yml?branch=main&amp;label=CI&amp;style=flat-square" alt="CI"></a>
    <a href="https://www.rust-lang.org/"><img src="https://img.shields.io/badge/Rust-1.91%2B-b7410e?style=flat-square&amp;logo=rust&amp;logoColor=white" alt="Rust 1.91 或更新版本"></a>
    <a href="LICENSE"><img src="https://img.shields.io/badge/License-GPL--3.0--or--later-111111?style=flat-square" alt="许可证：GPL-3.0-or-later"></a>
  </p>
  <p>
    <a href="#ossia-的功能">功能</a> | <a href="#ossia-的使用案例">案例</a> | <a href="#快速开始">快速开始</a> | <a href="#小贴士">小贴士</a> | <a href="#qa">Q&amp;A</a> | <a href="#开发指引">开发</a> | <a href="#许可证">许可</a><br>
    <a href="README.md">English</a> · 简体中文
  </p>
</div>

Ossia 是一款体验极佳的 AI 原生乐谱制作软件，它为 AI Agent 提供恰当的工具和指引，你只需要与 Agent 对话即可轻松制作乐谱。

> Ossia 基于 [GPL-3.0-or-later](LICENSE) 开源，商业使用、销售、分发时需遵守该协议规则，包括：保留版权及许可声明、向接收者提供对应源代码、分发修改版须继续以 GPL 授权。详细条款请参阅 [许可证](LICENSE)和 [GNU GPL 常见问题](https://www.gnu.org/licenses/gpl-faq.html)。

| 乐谱库                                                                        | 与 Agent 协作                                                                           |
| ----------------------------------------------------------------------------- | --------------------------------------------------------------------------------------- |
| <img src="docs/readme-assets/library-zh.webp" alt="Ossia 乐谱库" width="575"> | <img src="docs/readme-assets/agent-session-zh.webp" alt="Agent 与乐谱页面" width="575"> |

## Ossia 的功能

Ossia 可以一键接入常见 AI Agent（完整列表见[接入 Agent](#2-接入-agent)），除此之外，它还提供如下功能：

- 支持但不限于制作钢琴、吉他、尤克里里、小提琴，以及它们的多重奏乐谱。
- 支持导入 MusicXML/MIDI/PDF 以及图片乐谱（借助 Ossia OMR 服务）。
- 支持导出 PDF、PNG、SVG、MusicXML、MIDI 和 WAV 格式的乐谱或音频文件。
- 支持细粒度乐谱元素标注和自由绘画。
- 支持在触控屏幕上浏览乐谱。
- 支持局域网共享乐谱。

## Ossia 的使用案例

Ossia 可以帮你从现有乐谱或简单想法开始，一步步完成乐谱改写或创作，无论是用于学习、教学还是专业制谱，都是极佳的场景。

### 创建乐谱

<img src="docs/readme-assets/case-find-score.webp" alt="创建巴赫乐谱" width="720">

创建乐谱：《羊儿可以安静地吃草》，BWV 208。

### 改编乐谱

<img src="docs/readme-assets/case-arrange.webp" alt="改编钢琴四手连弹乐谱" width="720">

把《羊儿可以安静地吃草》改编成钢琴四手连弹，写入新乐谱。

### 创作旋律与伴奏

<img src="docs/readme-assets/case-compose.webp" alt="创作旋律并配上和声与伴奏" width="720">

写一段优美的旋律，为旋律配和声并加入伴奏声部。

### 导入图片乐谱

<img src="docs/readme-assets/case-import-image.webp" alt="导入图片乐谱并校对" width="720">

导入这张图片乐谱，仔细核对原谱并改正错误。

### 分析调性并移调

<img src="docs/readme-assets/case-transpose.webp" alt="分析调性并移调" width="720">

《空洞骑士》是什么调性？移调到 A 小调。

### 改编吉他六线谱

<img src="docs/readme-assets/case-guitar-tab.webp" alt="改编吉他六线谱并标注和弦图" width="720">

将《空洞骑士》转成吉他六线谱，保存为新乐谱并标注和弦图。

> AI 可能出错，请仔细甄别，鼓励多轮打磨成果。

## 快速开始

### 1. 安装 Ossia

> ⚠️ 当前安装包尚未签名或公证，macOS 和 Windows 可能阻止安装，确保从可信源下载安装包，按指引解锁安装限制（仅首次安装时需要）。

Ossia 当前提供 macOS（Apple Silicon）和 Windows x64 平台的安装包，可前往 [Releases](https://github.com/micooz/ossia/releases) 页面下载。

### 2. 接入 Agent

1. 启动 Ossia，确认窗口右上角显示「服务运行中」，如果没有则先手动点击启动服务。
2. 在「接入 → Agent」面板中，点击按钮为你的 Agent 接入 Ossia。

<img src="docs/readme-assets/agent-zh.webp" alt="Agent 接入面板" width="720">

当前 Ossia 支持的 Agent 有：

- Codex
- DeepSeek Harness
- Antigravity
- Antigravity CLI
- Claude Code
- Claude Desktop
- WorkBuddy
- Pi

### 3. 创建、导入、编辑乐谱

回到 Agent 新建一个对话，以 `$ossia` 或 `/ossia` 开头发起任务（建议每个新对话使用一次），例如：

```
/ossia 创建一首适合初学者的 C 大调钢琴练习曲，共 8 小节。
/ossia 创建巴赫 C 大调前奏曲
/ossia 导入这份 MusicXML 乐谱，完成后打开让我检查一下。
```

> 通过 Agent 给出的乐谱链接，或前往 Ossia 乐谱库直接打开乐谱。

<img src="docs/readme-assets/create-score-zh.webp" alt="通过 Agent 创建乐谱" width="720">

### 4. 查看乐谱

Ossia 乐谱页面提供体验极佳的交互功能，你可以在这里：切换乐谱、回退版本、播放、标注、绘画、导出和设置乐谱。

<img src="docs/readme-assets/score-zh.webp" alt="乐谱页面与绘画工具" width="720">

### 5. 导出乐谱

Ossia 支持多种导出格式，不同导出格式具备不同的参数选项，请根据需要选择。

<img src="docs/readme-assets/export-zh.webp" alt="乐谱导出面板" width="720">

## 小贴士

### 本地数据

Ossia 将乐谱库、导入原件和版本历史保存在本地，可以从管理窗口打开数据目录。默认位置为：

- macOS：`~/.ossia`（即 `/Users/<用户名>/.ossia`）
- Windows：`%USERPROFILE%\.ossia`（通常为 `C:\Users\<用户名>\.ossia`）

### 如何更新

在「设置 → 关于」中可以设置自动下载更新，应根据网络环境选择：

- Ossia 更新源：为中国大陆优化。
- GitHub 更新源：供全球访问。

### 其他提示

- 在 PC 乐谱页上使用快捷键能提升操作效率。
- 在乐谱上进行标注或绘画，会实时同步至打开此乐谱的其他设备。
- 乐谱导出也可在 Agent 对话中完成。
- 关闭主窗口后，Ossia 通常会继续在系统托盘中运行。在托盘菜单中选择「退出 Ossia」可完全退出 Ossia，退出后将无法在 Agent 中使用 Ossia。

## Q&A

### Q: Ossia 支持导入 PDF、图片乐谱吗？

支持。你也可以在「设置 → 常规 → 乐谱识别服务（OMR）」中配置光学乐谱识别（OMR）服务，OMR 可以加速将 PDF 或图片转换成乐谱。

OMR 是可选的，即便没有它，你的 Agent（配合强大的模型）仍可能识别出乐谱。

> 如果你想自己部署服务，请参阅 [Ossia OMR 自部署指南](packages/ossia-omr/deploy/README.md)。

### Q: Ossia 支持音频扒谱吗？

Ossia 没有提供内置的音频转谱功能，但也许你的 Agent 可以做到，不妨试试看：

> /ossia 把 audio.mp3 导入为乐谱。

### Q: 为什么 Agent 完成任务的结果不及预期？

Ossia 针对常见需求进行过测试和 Agent 路径优化，难免遇到未验证过的需求场景导致 Agent 绕远路或无法实现。

此时可换用能力更强的模型，或者提交 Issue 告知你的需求场景，以帮助我们进行优化。

## 开发指引

在根目录运行：

```sh
# 安装依赖并下载 LilyPond 引擎和文档。
pnpm setup

# 启动桌面程序
pnpm dev:desktop
```

- 开发版的数据使用项目中的 `.ossia`，与正式版隔离。
- 可以运行 `pnpm check`、`cargo test` 和 `pnpm test:ui` 执行相应检查。

## 致谢

Ossia 的诞生离不开开源软件和社区，感谢以下开源项目及其贡献者：

| 项目                                                         | 用途               | 许可证                                                                                                                  |
| ------------------------------------------------------------ | ------------------ | ----------------------------------------------------------------------------------------------------------------------- |
| [LilyPond](https://lilypond.org/)                            | 乐谱排版与格式转换 | 主程序为 [GPL-3.0-or-later](https://github.com/lilypond/lilypond/blob/master/LICENSE)；文档、字体及部分文件采用其他条款 |
| [Rust](https://www.rust-lang.org/)                           | 核心程序与后端开发 | 主要为 [MIT 和 Apache-2.0](https://github.com/rust-lang/rust#license)；部分组件采用其他许可证                           |
| [Tauri](https://tauri.app/)                                  | 桌面应用框架       | [MIT 或 Apache-2.0](https://github.com/tauri-apps/tauri/blob/dev/ARCHITECTURE.md#license)                               |
| [React](https://react.dev/)                                  | 用户界面           | [MIT](https://github.com/facebook/react/blob/main/LICENSE)                                                              |
| [Vite](https://vite.dev/)                                    | 前端构建工具       | [MIT](https://github.com/vitejs/vite/blob/main/LICENSE)                                                                 |
| [SQLite](https://sqlite.org/)                                | 本地乐谱库         | [Public domain](https://sqlite.org/copyright.html)                                                                      |
| [resvg](https://github.com/linebender/resvg)                 | SVG 渲染           | MIT 或 Apache-2.0                                                                                                       |
| [libimagequant](https://github.com/ImageOptim/libimagequant) | PNG 图像量化       | GPL-3.0-or-later                                                                                                        |

- 这里列出的是主要项目，并非完整清单。
- 发行安装包中的 `THIRD-PARTY-NOTICES.md` 和 `licenses/` 包含更多依赖及许可信息。
- 音频样本的来源和授权见[样本清单](packages/ui/public/audio/manifest.json)。

## 许可证

Ossia 采用 [GNU 通用公共许可证第 3 版或更高版本](LICENSE)。
