# image_editor — 用 quill 自己画的图像编辑器

一个精简版 Photoshop 风格的 2D 图像编辑器示例。**界面用本仓库自己的 UI 栈**
（`draw_theme` / `draw_components` / `draw_ui` + `draw_backend_wgpu`）实现，
不用 `egui`。

这是一个独立包（自己的 workspace），不加入主 workspace，避免 `winit` / `wgpu`
影响 `cargo check --workspace`。图标用核心的 `draw_svg`（零外部依赖）；PNG
编解码用 `png` crate（依赖只留在**示例层**，核心 crate 仍然无依赖）。

## 运行

```bash
cargo run --manifest-path examples/image_editor/Cargo.toml
cargo run --manifest-path examples/image_editor/Cargo.toml -- --selfcheck
cargo run --manifest-path examples/image_editor/Cargo.toml -- --dump
cargo run --manifest-path examples/image_editor/Cargo.toml -- --frames 120
cargo run --manifest-path examples/image_editor/Cargo.toml -- --light
cargo run --manifest-path examples/image_editor/Cargo.toml -- --pixel-font
```

## 阶段

按 `AGENTS.md` 的 Phase 计划逐步实现，**一次一个 Phase**。

- [x] **Phase 1 — Skeleton**：窗口、菜单栏、工具栏、画布占位、图层/属性栏、
  状态栏。工具选择是真的（点击 / 快捷键），其余按钮点击在状态栏给出
  “该功能属于哪个 Phase”的提示。
- [x] **Phase 2 — Document / Layer / PixelBuffer / Color**：后端无关的文档数据
  模型（`src/document/`）+ `AppState` 持有真实的 800×600 `Document`；画布占位
  和状态栏显示文档尺寸。合成 / 渲染仍未接。
- [x] **Phase 3 — Canvas（缩放 / 平移 / 坐标转换 / 显示图像）**：
  `src/renderer/` 的 CPU 合成器把文档图层合成为一张 `PixelBuffer`；宿主用
  `register_texture` 上传，画布是场景里的一个 `Node2D`（`Visual::Image`），
  相机变换挂在它的 `Transform2D` 上，由 `draw_scene` 绘制、UI 覆盖其上。
  `src/canvas/` 提供相机与坐标转换；滚轮缩放（锚定指针）、中键平移、
  `+`/`-`/`0`/`F` 快捷键，状态栏显示指针下的像素坐标。画布默认带一层透明
  棋盘格背景（`src/canvas/checkerboard.rs`）：只在显示用的合成结果上生成，
  所以隐藏 / 擦除掉不透明的「背景」图层就会露出来，而导出的 PNG 仍保留 alpha；
  视图菜单的「显示棋盘格」可以开关它。
  默认文档是 128×128 的像素画画布，宿主用
  `WgpuBackend::set_texture_filter(.., TextureFilter::Nearest)` 放大时做最近邻
  采样，像素不会插值模糊。
- [x] **Phase 4 — 图层管理**：图层面板接真实 `Document`：虚拟化图层列表
  （眼睛 / 名字 / 不透明度、点选当前图层）+ 增删 / 显示隐藏 / 不透明度 ±10% /
  上移下移 / 重命名（键盘内联编辑，Enter 确认・Esc 取消）；任何改动后重新合成
  并重传纹理。属性面板实时显示当前图层：名字 / 不透明度 / 混合模式，以及图层
  的 `position` 偏移与缓冲区尺寸（移动画布外内容后缓冲会变大，这里能看到）。
- [x] **Phase 5 — Brush / Eraser**：`src/tools/` 的 `Tool` 接口 +
  `BrushTool`（`BrushMode::{Paint, Erase}` 共用一个引擎，§11）；在画布上
  左键拖动即在当前图层绘制 / 擦除，实时重合成。默认是**像素模式**
  （`hard = true` + `BrushShape::Square`）：任意尺寸圆心都吸附到像素网格、
  `coverage` 二值（边缘不抗锯齿），适合画像素图；工具选项栏可切换「像素」
  与「方形」两个开关。关掉像素模式后，圆形笔走 1px 抗锯齿、方形笔走切比雪夫
  距离的软边。1px 硬边笔走 Bresenham 连线（每个像素只压一次，斜线不会加粗）；
  像素模式 + `zoom >= 6` 时在画布上叠加屏幕空间的像素网格。`[`/`]` 调大小，
  `,`/`.` 调不透明度。
  “一笔 = 一次 Undo”：抬笔时提交成一条 [`PaintCommand`]，由 Phase 6 的历史栈接管。
- [x] **Phase 6 — History**：`Command` 模式 + 两条栈
  （`src/document/history.rs`），一笔画笔 / 橡皮 = 一步 undo。命令只记录**差异
  区域的前后像素**（`PixelRegion`，靠落笔快照与抬笔缓冲求 diff），所以撤销内存
  跟笔画大小成正比，不是整层快照；新命令清空 redo 栈，栈有上限（默认 32 步）。
  工具栏加了 `↶ 撤销` / `↷ 重做` 按钮，快捷键 `Ctrl/Cmd+Z` 撤销、
  `Shift+Ctrl/Cmd+Z`（或 `Ctrl+Y`）重做，结果写状态栏。
  除了画笔，图层的编辑也入历史：移动工具（`SetLayerPositionCommand`）、新建 / 删除
  （`AddLayerCommand` / `RemoveLayerCommand`）、重命名 / 可见性 / 不透明度 / 排序
  （`LayerMetaCommand`，只存元数据 + 顺序，不克隆像素）、裁到文档
  （`CropLayerCommand`）。
- [x] **Phase 7 — 导入导出**：`src/io/` 用 `png` crate 做编码与解码，拆成
  两个关注点：`codec`（`PixelBuffer <-> PNG 字节`）与 `file`
  （`路径 <-> PNG 字节` + 默认命名）。右侧「文件」面板可改路径（内联编辑，
  Enter 确认・Esc 取消）、「导出 PNG」把当前文档**合成后**写盘、「导入 PNG」
  把图片作为**新图层**放到最上面并选中。winit 没有原生文件对话框，所以用路径
  文本输入而不是系统弹窗；导入不进撤销栈（与「+ 图层」一致）。
- [x] **Phase 8 — Move / 框选 / 吸管**：补齐三个工具。
  `tools/move_tool.rs` 在画布上拖动改变**当前图层**的 `position`（图层像素
  缓冲区的原点，可以变负；不进撤销栈，与图层的增删 / 排序一致）。画笔 / 橡皮
  在**文档坐标**落笔，写入时减去 `position` 换算到缓冲区，所以笔迹始终对着
  光标；落笔前 `Document::ensure_layer_covers_document` 把缓冲区扩到
  “当前范围 ∪ 文档范围”，于是拖动画布之后：图层移空、重新露在文档里的区域是
  透明像素、可以继续画（“画布外／canvas 内也能画”），而**移出画布的像素不裁掉**，
  留在缓冲区里还能再移回来（补空间是左侧 / 上方，历史命令的区域会跟着平移）。
  要回收被移动撑大的缓冲区，用图层菜单的「裁到文档」（按 `position` 摆好后裁回
  文档尺寸、丢弃画布外像素）。
  框选拖出选区（`canvas::pixel_selection` 把两个角点
  裁剪成整数 `PixelRegion`，存在 `AppState.selection`），画笔落笔按选区裁剪，
  选中时画一圈描边，Esc 清空；吸管用 `renderer::sample_pixel` 取**合成后**的
  颜色写进前景色。三个工具的纯逻辑分别放在 `canvas` / `renderer` / `tools`，
  视图只做坐标换算与状态同步。
- [x] **Phase 9 — 菜单栏（真实下拉）**：标题点击改成一个共享请求格，
  `EditorView::update` 用它调 `draw_components::Overlays::menu` 弹出下拉；
  下拉内容是一个 `Menu`（`MenuItem` 行 = 左标签 + 右快捷键，动作不可用时
  `disabled`）。撤销 / 重做、导入 / 导出、缩放 / 适配、取消选区、关于已接真实
  动作，其余是带标注的占位项。菜单打开时点**另一个**标题会一次点击切换、点
  当前标题则关闭（标题的点击在覆盖层消费外部点击之前被 `EditorView` 拦下，
  再交给主树）。`Menu` / `MenuItem` / `Overlays::menu` 都在核心包
  `draw_components`，Phase 11 的右键菜单直接复用。
- [x] **Phase 9.x — 工具选项栏 + 可拖动右栏**：菜单栏下面多一行工具选项栏
  （`ui/options_bar.rs`）：画笔 / 橡皮显示笔刷大小、不透明度与 `−` / `+`
  按钮，其余工具显示一句操作提示；`−` / `+` 只写请求格，`update` 统一调整。
  右侧栏宽度改用 `ResizeHandle::vertical(..).invert()`（目标在把手右边）拖动，
  并在 `layout` 里 `clamp_sidebar_width`，保证画布不被挤没。右栏内部的
  「文件 / 图层 / 属性」三块之间也用 `ResizeHandle::horizontal(..)` 分隔，可以拖动
  调整各自高度（`文件` 驱动上面的面板、`属性` 用 `invert()` 驱动下面的面板，中间
  图层面板 `grow(1)` 吸收剩余）；`clamp_panel_heights` 预留图层最小高度。
  右栏还多了「历史」面板（`ui/history_panel.rs`）：一条虚拟化 `List` 显示
  撤销 / 重做栈（旧→新 → 「● 当前」→ 重做下一个），点某一步就撤 / 重做到那里。
  左侧工具栏右边是**独立的调色盘面板**（`ui/palette.rs`，宽度可拖）：上面一个
  **HSV 取色器**（饱和/明度方块 + 色相条，拖动即改前景色），下面是当前前景 /
  背景与 16 个预设色块。取色器靠 `Component::on_pointer`（press + move 给绝对
  位置）把指针映射到自己的矩形上；没有渐变图元，方块用一小片实心色块拼出。
  面板外观抽成了项目内的 `ui/card.rs`（`Card`：`surface` 底 + 内边距 + 行间距，
  无边框 / 圆角），右栏的文件 / 图层 / 属性 / 历史和它用同一套，侧栏看起来是
  一组卡片。

### 计划（Phase 10+）

本示例的待办已经整理到同目录的 [`todo.md`](todo.md)（含像素模式主线、
Phase 10/11 和性能收尾），README 只留功能说明。要点：

- **Phase 10 — 文件浏览器**：路径输入换成 `List` 覆盖层，扫描走工作线程，
  过滤 `*.png`，选中即导入 / 设导出路径。
- **Phase 11 — 图层右键菜单**：两个前置（增量）：`Overlays` 支持原始矩形 / 指针
  位置锚点；`draw_ui` 增加右键回调。

## 主题（紧凑）

编辑器用自己的主题 `theme::editor_theme(light)`：设计系统的调色板 +
`Density::COMPACT`（间距 ×0.75、控件更矮、默认 mini 按钮）。这是一次 token
替换，不是第二条代码路径 —— 组件库的间距 / 控件高度都从 `Theme` 读
（`theme.spacing(..)` / `theme.control_height(..)` / `theme.row_height()`），
所以改 `editor_theme` 一处就能整体调紧/调松。想回到常规尺寸用
`Density::COMFORTABLE`。

工具栏的图标尺寸是**显式固定**的（`icons::TOOLBAR_ICON`，20px，画在按钮中央），
不从按钮矩形推算 —— 所以紧凑主题把按钮变矮时图标不会跟着缩水；图标按钮用
`.min_size(32, 28)` 钉住高度，密度默认值不会覆盖它。

## 快捷键

| 键 | 作用 |
|---|---|
| `V` | 移动当前图层（画布上拖动） |
| `B` | 画笔 |
| `E` | 橡皮 |
| `M` | 矩形选择 |
| `I` | 吸管 |
| `Esc` | 清空选区 |
| `Ctrl/Cmd+Z` | 撤销 |
| `Shift+Ctrl/Cmd+Z` / `Ctrl+Y` | 重做 |
| `[` / `]` | 笔刷直径 ∓2px |
| `,` / `.` | 笔刷不透明度 ∓5% |
| `+` / `=` / `-` | 画布缩放 |
| `0` | 100% 并居中 |
| `F` | 适配画布区域 |

> 修饰键快捷键在**宿主层**处理：`InputEvent::KeyDown` 没有修饰键字段
> （`draw_core` 刻意保持最小），所以 `app/application.rs` 把 `Ctrl/Cmd+Z`
> 直接映射到 `EditorView::undo/redo`，不当作普通按键。

## 结构

```text
assets/icons/            # vendor 的 7 个 Lucide 图标（工具栏用）+ ISC LICENSE
src/
├── main.rs              # 参数解析
├── app/
│   ├── state.rs         # ActiveTool / CanvasCamera / AppState（纯数据）
│   └── application.rs   # winit + wgpu 宿主（滚轮 -> Wheel、纹理上传）
├── icons.rs             # Lucide 图标包加载 + `Icon` 组件（draw_svg）
├── document/            # Phase 2：后端无关的数据模型
│   ├── mod.rs
│   ├── color.rs         # 8-bit RGBA
│   ├── id.rs            # DocumentId / LayerId
│   ├── point.rs         # 文档像素坐标
│   ├── region.rs        # Phase 6：整数像素矩形 + diff + contains（选裁剪）
│   ├── pixel_buffer.rs  # 行优先 RGBA
│   ├── layer.rs         # Layer + BlendMode
│   ├── history.rs       # Phase 6：Command / History / PaintCommand
│   └── document.rs      # Document + 图层栈操作
├── canvas/              # Phase 3：画布相机与坐标转换
│   ├── mod.rs           # DOCUMENT_TEXTURE 约定
│   ├── camera.rs        # CanvasCamera（zoom / offset / fit）
│   └── coordinate.rs    # screen -> document -> pixel + pixel_selection（框选）
├── renderer/            # Phase 3：合成器
│   ├── mod.rs           # Renderer trait
│   ├── target.rs        # RenderTarget
│   └── cpu.rs           # CpuRenderer（Normal + opacity + position）+ sample_pixel
├── io/                  # Phase 7：导入导出
│   ├── mod.rs           # IoError
│   ├── codec.rs         # PixelBuffer <-> PNG 字节（png crate）
│   └── file.rs          # 路径 <-> PNG 字节 + 默认命名 / 图层名
├── tools/               # Phase 5/8：工具系统
│   ├── mod.rs
│   ├── tool.rs          # Tool trait + PointerEvent + ToolContext（含 History）
│   ├── brush.rs         # BrushTool + BrushMode（画笔 / 橡皮，抬笔提交一笔）
│   └── move_tool.rs     # Phase 8：MoveTool（拖动当前图层）
├── ui/
│   ├── mod.rs           # EditorView：页面 + 文档 Node2D + 相机同步 + undo/redo
│   ├── card.rs          # 项目内简单卡片（侧栏统一外观：surface + 内边距）
│   ├── menu.rs          # 菜单栏
│   ├── toolbar.rs       # 工具栏（工具 + 撤销/重做）
│   ├── palette.rs       # 左侧调色盘面板（HSV 取色器 + 前景/背景 + 预设色块）
│   ├── options_bar.rs   # 工具选项栏（笔刷大小 / 不透明度 / 提示）
│   ├── canvas.rs        # 透明画布区域（命中 / 定位用）
│   ├── file_panel.rs    # Phase 7：文件面板（路径 + 导入 / 导出按钮）
│   ├── layer_panel.rs   # 图层面板（List + 操作按钮）
│   ├── history_panel.rs # 历史面板（List：undo/redo 栈，点一步跳过去）
│   ├── properties_panel.rs  # 当前图层属性（只读）
│   └── status_bar.rs
└── selfcheck.rs         # 无头自检（录制 DrawList + draw_profile 体检）
```

## 画布操作

| 操作 | 效果 |
|---|---|
| 滚轮 | 缩放（锚定指针下的文档点） |
| 中键拖拽 | 平移 |
| `+` / `=` | 放大 |
| `-` | 缩小 |
| `0` | 100% 并居中 |
| `F` | 适配画布区域 |
| `[` / `]` | 笔刷直径 ∓2px |
| `,` / `.` | 笔刷不透明度 ∓5% |
| 左键拖动 | 画笔 / 橡皮画笔触（先选 `B`/`E`） |

## 图层操作

在右侧图层面板里先点选一个图层，再用按钮操作它：`+ 图层` / `− 删除`、
`显示/隐藏`、不透明度 `−` / `+`、`↑` / `↓`、`重命名`（输入后 Enter 确认，
Esc 取消）。列表第 0 行是最上面的图层。

## 导入导出（Phase 7）

右侧「文件」面板：

1. 点「改路径」内联输入一个 `.png` 路径（Enter 确认，Esc 取消）；
   默认是当前目录 + 文档名。
2. 「导出 PNG」把当前文档**合成后**写盘（含透明背景；隐藏的图层不写）。
3. 「导入 PNG」把该文件解码后作为**新图层**放在最上面并选中。图片尺寸可以与
   文档不同：合成器按 `Layer.position` 裁剪，超出画布的部分不显示。

没有原生文件对话框（winit 不带），所以用路径文本输入。路径打不开 / 不是 PNG
只在状态栏报错，不会崩。

## 工具（Phase 8）

| 工具 | 键 | 操作 |
|---|---|---|
| 移动 | `V` | 在画布上拖动 → 当前图层按指针位移改 `position`（不进撤销栈） |
| 框选 | `M` | 拖动出矩形选区；画笔只在选区内落笔；选中有描边；`Esc` 清空 |
| 吸管 | `I` | 点画布 → 取**合成后**的颜色作为前景色（画笔颜色） |

选区是编辑器状态（`AppState.selection`），不是文档数据；两个角点由
`canvas::pixel_selection` 归一化并裁剪到画布内。移动只改 `Layer.position`，
合成器会把偏移算进摆放；和图层增删 / 排序一样**不**占用撤销步数。

## 验证（无截图）

遵守仓库规则：不截图。`--selfcheck` 把同一棵视图画进
`draw_backend_recording::RecordingBackend`，用 `draw_profile::inspect` 做结构
体检，并断言菜单 / 工具 / 画布 / 图层 / 图标 / 状态栏的文字都在命令流里。
Phase 6 起还做一次**真实的撤销 / 重做往返**：画笔一笔 → 点工具栏「撤销」→ 断言
合成结果回到白底且有 redo → 点「重做」→ 断言又变黑；最后断言历史栈是
`undo 1 · redo 0`。Phase 7 再加一段**真实的 PNG 往返**：把当前文档导出到临时
文件、解码回来核对白底与画笔像素；另写一张 4×3 红色 PNG 点「导入 PNG」，断言
图层数 +1 且合成后左上角变红。Phase 8 再验三个工具：吸管点在黑色笔画上 → 前景色
变黑；拖出框选 → 选区存在；移动拖动 → 当前图层位置改变。附加的图标包也会断言：
索引覆盖工具栏的工具与撤销 / 重做，且帧里确实有图标描出来的 `FillCircle`。同一套
断言在 `cargo test --manifest-path examples/image_editor/Cargo.toml` 里跑。

## 图标（Lucide，附加）

工具栏的 5 个工具和撤销 / 重做各有一枚 Lucide 图标。图标是一个真正的
`Icon` **组件**（`src/icons.rs`）：`Icon::new(icons, name, color, size)` 占
`size × size`，在自己的矩形里居中描边；构建按钮时直接
`.child(Icon::new(..))`（见 `ui/toolbar.rs`），不用等树建好再回头挂装饰器。
按钮本身是空的点击 / 高亮区，`Icon` 的 `mouse_filter` 是 `Ignore`，点击落到按钮上。
当前工具用 `theme.palette.selection` 高亮（`Button` 的显式 `dynamic_background`
会覆盖 variant 的默认背景）。

描边走核心的 `draw_svg`（backend-neutral：把 SVG 描边成 IR 的 `Line` /
`FillCircle`）——**不栅格化成纹理**，也不加额外依赖；小字标签在按钮下面。
默认加载仓库自带的 7 个图标（`assets/icons/`，含 Lucide 的 ISC `LICENSE`），
测试与 `--selfcheck` 因而可复现。

想看完整图标包（2112 个）：

```bash
curl -L -o lucide.tgz https://registry.npmjs.org/lucide-static/-/lucide-static-1.47.0.tgz
tar -xzf lucide.tgz package/icons
IMAGE_EDITOR_ICON_DIR=$PWD/package/icons \
  cargo run --manifest-path examples/image_editor/Cargo.toml
```

工具栏仍只用得到那 7 个名字，但 `IconPack` 会索引整包（`--selfcheck` 报告的图标数
会从 7 变成 2000+）。渲染细节见 `docs/svg.md`。
