# image_editor — TODO

`examples/image_editor` 自己的项目级待办（这是一个独立 cargo workspace）。
**从上往下做**；每条写清「改哪里 / 怎么验收」。与 quill 主仓的路线图
（`docs/plan.md`、`docs/godot-migration.md`）分开，别混。

约定（见 `/AGENTS.md`）：每阶段 **API → 测试 → 实现 → 集成**，结束出报告等确认；
验证只允许程序化（DrawList 命令 / 布局矩形 / 回调 / 后端像素缓冲），**不做截图**。

最近落地：透明棋盘格背景；默认 128×128 像素画布 + 1px 硬边笔 + 最近邻采样；
移动图层后画笔对准光标、画布外像素不裁。

---

## 1. 主线：像素模式

现状：`src/tools/brush.rs` 的 `BrushTool::dab` 只有 `size <= 1.0` 走硬边 / 吸附
分支，`> 1` 一律 `coverage = (radius + 0.5 - distance).clamp(0, 1)`（抗锯齿）。
目标：任意尺寸都能画**硬边像素**，可选**方形笔**，并接到选项栏。

### P1 — `BrushTool` 硬边 + 形状（核心 API）

文件：`src/tools/brush.rs`

- API：
  - `pub enum BrushShape { Round, Square }`
  - `BrushTool { pub hard: bool, pub shape: BrushShape }`；默认 `hard = true`、
    `shape = Square`（demo 已是像素编辑器）。
- 行为：
  - `hard` 时圆心吸附到像素网格（`floor + 0.5`），对所有尺寸生效（现在只对 1px）。
  - `coverage` 二值：`Round → distance <= radius`；`Square → 整数边长
    n = size.round().max(1)`，奇数 `n×n` 居中、偶数把光标像素放在四个中心之一
    （避免偶数尺寸多一格 / 十字）。
  - `opacity` 照乘；`< 100%` 叠加会加深（P4 才彻底解决）。
- 测试（`brush.rs`）：
  - size 3 + `hard` 只产生 `a ∈ {0, 255}`；`hard = false` 负向对照会出中间 alpha。
  - `Square` size 3 = 9 px、size 2 = 4 px。
  - size 1 现有行为不变（现有测试继续绿）。

### P2 — 选项栏开关（集成）

文件：`src/ui/options_bar.rs`、`src/ui/mod.rs`

- 画笔配置里加「像素」开关：`Rc<Cell<bool>>` + `dynamic_background`（照
  `src/ui/toolbar.rs` 的 active 高亮范式），点击翻转 cell；`EditorView::update`
  把 cell 同步进 `self.brush.hard`（cell 管显示、`brush` 管绘制，每帧对齐）。
- 切到橡皮同样生效（共用 `self.brush`）。
- 测试（`ui/mod.rs`）：点开关 → `view.brush_hard()` 翻转；再点回；切工具后保持；
  像素模式下 size 3 的笔画没有半透明边。
- `--selfcheck`：真实点一次开关 + 画一笔，断言区域内无部分 alpha。

### P3 — 像素网格叠加（可选）

- 像素模式开启 **且** `zoom >= 4` 时，在画布上画文档像素边界的 1px 线
  （`DrawCommand::Line`），屏幕空间、不随缩放变粗；只画可见文档范围。
- 位置照 `EditorView::paint_selection`（世界之上、UI 之下）。
- 测试：`RecordedFrame` 断言线的数量 / 位置随缩放变化；关闭或低缩放不画。

### P4 — 像素完美连线（可选）

- 1px 硬边笔改用 Bresenham 连接采样点，去掉重复压点导致的 `<100%` 加深、以及
  斜线的 L 型加粗。
- 测试：45° 斜线只覆盖 Bresenham 上的像素。

### P5 — 文档 / 自检

- 本 README 的 Phase 5 / 新 Phase 9.x、`AGENTS.md` 补像素模式。
- 全量 `fmt / check / test / selfcheck`。

### 待拍板（开工前确认）

1. 默认 `hard = true` + `Square`？
2. 一个「像素」开关，还是「硬边」+「方形」两个独立开关？
3. P3 像素网格：现在做还是缓？
4. 状态归属：`Rc<Cell>` + `self.brush` 每帧同步（推荐），还是把笔刷设置搬进
   `AppState`（改动更大）？

---

## 2. 紧接着

- **CPU 合成脏矩形**（性能，优先）：`src/renderer/cpu.rs` 每次全量合成；图层缓冲
  现在可能比文档大，成本随缓冲尺寸走。加 `DirtyRegion`，或至少只合成文档范围。
- **棋盘格屏幕空间恒定大小**：现在 8 文档像素/格，随缩放变大；可改成屏幕空间恒定
  格子或加开关。文件：`src/canvas/checkerboard.rs`、`src/ui/mod.rs`。
- **最近邻只在 wgpu**：给 `draw_backend_canvas` 加 `imageSmoothingEnabled` 等价
  开关。文件：`crates/draw_backend_canvas`（quill 主仓）。
- **图层缓冲只增不减**：加上限 / 回收，或提供「裁到文档」的命令。
- **`--selfcheck` 覆盖**：棋盘格 / 最近邻 / 移动映射目前只有单元测试。
- **UI 暴露**：属性面板显示图层 `position` / 缓冲尺寸；棋盘格 / 最近邻开关。

## 3. 计划阶段（Phase 10+）

复用 `draw_components::Overlays` 与 `examples/file_browser` 的工作线程 /
`EventLoopProxy` 模式：

- [ ] **Phase 10 — 文件浏览器**：把「文件」面板的路径文本输入换成选择器覆盖层：
  `List` 列目录项，扫描走工作线程，双击 / Enter 进入目录、过滤 `*.png`，选中即
  导入并设定导出路径。
- [ ] **Phase 11 — 图层右键菜单**：图层行右键在指针处弹出上下文菜单（重命名 /
  复制 / 删除 / 显示隐藏 / 上移下移 / 向下合并 / 不透明度）。两个前置（增量）：
  `Overlays` 支持**原始矩形 / 指针位置**锚点；`draw_ui` 增加右键回调
  （`PointerButton::Right` 已存在，但 `handle_input` 目前只处理左键）。

## Done（近期）

- 透明棋盘格背景（显示用合成，导出 PNG 仍保留 alpha）。
- 默认 128×128 像素画布 + 1px 硬边笔 + `TextureFilter::Nearest`。
- 移动图层后画笔对准光标；图层缓冲按「当前范围 ∪ 文档范围」扩展，
  **画布外像素不裁掉**，历史命令区域同步平移。
