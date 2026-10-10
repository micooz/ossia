
# Ossia 核心产品功能链路时序图

- 状态：Derived（由规范推导，不新增决策）
- 上游：[2026-10-08_tech-stack.md](./2026-10-08_tech-stack.md)（ADR-0001）、
  [2026-10-08_ly-parsing-indexing.md](./2026-10-08_ly-parsing-indexing.md)（ADR-0002）
- 日期：2026-10-08

本文把两份 ADR 的服务端五服务（A 文档 / B 渲染 / C 定位 / D Agent / E 导出）串成用户视角的四条产品链路。
图中每条分支与耗时标注都对应 ADR 条目，无新增决策；成本口径为 spike 实测值。

## 1. 参与者

| 标识 | 含义 |
|---|---|
| U | 演奏者（用户） |
| C | 客户端：Tauri 2，四端同一代码库；只见 SVG/PDF/摘要与 locator |
| A | 文档服务：版本链（DB + 内容寻址存储）、导入基线、CAS |
| B | 渲染服务：LilyPond 引擎子进程（`/opt/lilypond/<tag>/bin/`） |
| Cx | 定位服务：L0 词法 → L1 语义索引 → L2 版面列 → L3 锚点增强 |
| D | Agent 服务：投影 → LLM → 清洗 → 五道门 → 补丁 |
| E | 导出服务：基线 + 已接受补丁集 → MusicXML/MIDI |
| L | LLM |

贯穿所有图的两条不变量：**`.ly` 源码与基线 MusicXML 永不下发客户端**（G6、D9）；
**未经门禁校验的 LLM 文本不落盘**（G2、§6.4）。

## 2. 链路 1 · 导入：MusicXML → 可编辑可定位的乐谱

```mermaid
sequenceDiagram
    autonumber
    participant U as 演奏者
    participant C as 客户端 Tauri
    participant A as 文档服务
    participant B as 渲染服务 LilyPond
    participant Cx as 定位服务

    U->>C: 上传 MusicXML（或 MIDI）
    C->>A: 导入请求（文件）
    A->>B: 子进程 musicxml2ly --no-stem-directions（同版本 tag）
    Note over A,B: MIDI 输入走 midi2ly；一次性转换，基线随文档留存（D3）
    B-->>A: .ly 源码
    A->>A: 语义校验 AST 对齐：小节数 / 音符事件数 / 指法数 / tie 数
    alt 校验干净
        A->>A: 入库 versions(seq, content_hash, parent_hash, origin=import)
    else 转换器告警（如 Negative skip）
        A-->>C: 三态之一：需人工确认 / 声部预处理后重转 / 拒绝并报告
        Note over A,C: 不得静默入库（§6.1）
    end
    A->>B: 首次渲染 \pointAndClickOn → SVG + PDF + \midi 回放 MIDI
    B-->>Cx: 带锚点 SVG
    Cx->>Cx: L0 词法 tokenize（不变量 I1 字节级往返）
    Cx->>Cx: L1 语义索引 measure→源码行区间（权威号来自 % N 注释）
    Cx->>Cx: L2 版面列 staff space 众数 → 谱表 → 系统双峰分组 → 小节线覆盖率 → 小节列
    Cx->>Cx: L3 锚点增强 textedit:///…ly:L:C1:C2（仅符号级补充）
    Cx-->>A: locator 表 + 渲染产物（gzip 后 25–28 KB/页）
    A-->>C: 版本元数据 + SVG + locator sidecar（不含源码与基线）
    C-->>U: 谱面浏览
```

成本：转换 ≤ 2 s；L1 整曲 2.21 ms、L2 单页 4.4–6.3 ms；SVG 出图 ≤ 12 s（32 小节整曲实测 9.9 s）。

## 3. 链路 2 · 选择：点击 → locator

```mermaid
sequenceDiagram
    autonumber
    participant U as 演奏者
    participant C as 客户端
    participant Cx as 定位服务

    U->>C: 点选或框选（Apple Pencil / 鼠标）坐标 (x,y)
    C->>Cx: 提交版面坐标（或使用本地缓存的 locator 表求解）
    Cx->>Cx: L2 定位 staff + measure（小节列，稳定命中）
    Cx->>Cx: L3 在列内用锚点跨度推断 note / chord / clef / dynamic
    Cx->>Cx: L1 校验归一（无锚符号按规则映射：谱表首=谱号/调号/拍号，段末=结束线）
    Cx-->>C: locator {rev, measure, staff, voice?, element?} 或 measure/beat 区间
    Note over C,Cx: 视觉模型不作主机制；仅 L2 失败时作辅助提示，结论仍须 L1 校验（§6.2）
    C-->>U: 高亮所选（小节级必对，符号级允许二次确认修正）
```

## 4. 链路 3 · 自然语言修改（核心链路）

```mermaid
sequenceDiagram
    autonumber
    participant U as 演奏者
    participant C as 客户端
    participant D as Agent 服务
    participant L as LLM
    participant B as 渲染服务
    participant A as 文档服务

    U->>C: 批注「把第4小节末尾 bes 移低一个八度」+ 选择集
    C->>D: 任务（locator 集合 + 自然语言 + base content_hash）
    D->>A: 取当前版本 .ly；CAS 校验基版本哈希
    A-->>D: .ly（服务端内部，绝不下发）
    D->>D: L1：locator → 授权 lineStart/lineEnd；单文档任务串行
    D->>D: 投影 = 全量 .ly + 4 字符行号 gutter + TARGET 标签 [Voice m4]
    Note over D: ≈3.5k tokens，全量投放优于窗口切分（§5.1，结案 ADR-OPEN-5）
    D->>L: 投影 + 批注（max_tokens 8000，reasoning_effort=low）
    L-->>D: 提案 JSON（可能前置推理、出现多次自我修正）
    D->>D: 解析：取最后一个含合法 edits 数组的平衡对象；剥离 json 围栏
    D->>D: 清洗：剥离回写的 gutter 前缀与 TARGET 标签（§5.4）
    D->>D: G-a 行界：edits 全部落在授权区间内
    D->>D: G-b 外区不变：小节区外字符片段必须保留
    D->>B: G-c 语法/排版：编译 -dno-pdf-output（1.3 s）
    B-->>D: 有无 error
    D->>B: G-d 爆炸半径：隔离渲染所选小节（0.6 s）得音符上限 + 整曲 MIDI diff（1.3 s）
    Note over D,B: \relative 单音改动可致后续 341 音整体移八度，而编译全绿（§6.1）
    D->>D: G-e 语义判据：整曲 MIDI diff × token 前后缀裁剪 diff 双通道
    alt 任一门失败
        D->>L: 回带失败原因重试（固定 1 次；length 与格式错误分开计数）
        L-->>D: 新提案，重新过五道门
        alt 重试仍失败
            D-->>C: 自然语言原因 + 受影响范围（不暴露内部错误码与源码）
            C-->>U: 提示未达成，原谱不变
        end
    else 全部通过
        D->>D: AST diff 落结构化补丁，事务写入
        D->>B: 重新渲染 SVG 与 PDF
        B-->>D: 变更后谱面
        D-->>C: 变更前/后 SVG + 变化小节高亮 + 逐条变更说明 why
        C-->>U: 结果对比视图（不呈现补丁 / AST / 源码）
        alt 用户接受
            U->>C: 接受
            C->>A: 推进版本 origin=agent，parent_hash=旧哈希
        else 用户撤销
            U->>C: 撤销
            Note over C,A: 不写版本链，服务端丢弃补丁
        end
    end
```

成本：单任务 ≈ 1.5 模型秒 + 3.2 s 编译/门禁，另加 SVG 出图 ≤ 12 s。

唯一未闭合类别是「绝对音高计算」：提示规则能改变模型写法选择（`\absolute` /
`\resetRelativeOctave`）但补不上算错的八度，解法定为服务端由 L1 索引 + 音高状态机确定性提供目标音高事实
（ADR-0002 §6.3，ADR-OPEN-9，优先级最高）。

## 5. 链路 4 · 历史 / 回退 / 导出

```mermaid
sequenceDiagram
    autonumber
    participant U as 演奏者
    participant C as 客户端
    participant A as 文档服务
    participant B as 渲染服务
    participant E as 导出服务

    U->>C: 浏览历史版本
    C->>A: 请求版本列表
    A-->>C: versions(seq, hash, author, origin, created_at) + 各版本 SVG 缓存
    U->>C: 选择回退到 seq=k
    C->>A: restore 请求（携带基哈希）
    A->>A: 追加 restore 节点指向旧 content_hash（不删历史，链保持线性）
    A->>B: 按该版本记录的 tag 重新渲染（要求 /opt/lilypond/<tag>/ 仍在）
    B-->>A: SVG / PDF
    A-->>C: 回退后谱面

    U->>C: 导出出版包
    C->>E: 导出请求（当前版本 + 格式）
    E->>A: 取基线 MusicXML + 该版本的全部已接受补丁集
    E->>E: 补丁回放至基线（D7：.ly → MusicXML 无可复用文本转换器）
    E->>E: musicxml-io 序列化
    E-->>C: MusicXML 4.0 .mxl / MIDI / PDF（PDF 直接取渲染服务成品）
    C-->>U: 交付（对接 mymusic5 / Dorico / MuseScore / Flat）
```

## 6. 数据权威边界

| 数据 | 权威位置 | 是否下发客户端 |
|---|---|---|
| `.ly` 真源、投影文本、提案 JSON | 服务端（内容寻址存储） | 否 |
| 基线 MusicXML、补丁集、版本链 | 文档服务 A | 否 |
| SVG / PDF / 回放 MIDI | 渲染服务 B | 是（本地缓存） |
| locator `{rev, measure, staff, voice?, element?}` | 定位服务 Cx | 是（sidecar） |
| 变更说明 why、失败原因 | Agent 服务 D | 是 |
| 导出 .mxl / MIDI | 导出服务 E | 是 |

一句话链路：**导入定真源 → 三层定位收选择 → 批注驱动 Agent 在授权行区间改 `.ly` → 五道门把渲染危险挡在服务端 → 对比视图让演奏者决定 → 版本链留痕 → 补丁回放产出发布格式。**

G1 一致性由「`.ly` 同时是渲染源、编辑源与 Agent 读写面」这一结构保证，不依赖多表示间的同步纪律。
