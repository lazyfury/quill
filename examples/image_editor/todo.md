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

目标：任意尺寸都能画**硬边像素**，可选**方形笔**，并接到选项栏。
已定：默认 `hard = true`、`shape = Square`；两个独立开关（「像素」+「方形」）；
状态用 `Rc<Cell>` 管显示、`self.brush` 管绘制、每帧同步。

- [x] **P1 — `BrushTool` 硬边 + 形状**（`src/tools/brush.rs`）：
  `BrushShape { Round, Square }` + `BrushTool { hard, shape }`；`hard` 时圆心吸附
  像素网格（所有尺寸），`coverage` 二值（Round 比距离、Square 比整数边长 n，
  奇数居中 / 偶数把光标像素放左上）。测试：硬边只有纯色、软边有灰边、
  方形 3×3 / 2×2、size 1 不变。
- [x] **P2 — 选项栏开关**（`src/ui/options_bar.rs` + `src/ui/mod.rs`）：
  画笔配置加「像素」「方形」两个开关（`Rc<Cell>` + `dynamic_background`），
  `update` 同步进 `self.brush`；`EditorView` 暴露
  `pixel_mode`/`square_mode`/`brush_hard`/`brush_shape`/`brush_toggle_center`。
  `--selfcheck` 用一个新视图验证 size 3 硬边无灰边、软边（圆头）有灰边。
- [x] **P5 — 文档**：README / `AGENTS.md` 已补像素模式。
- [ ] **P3 — 像素网格叠加（可选）**：像素模式开启且 `zoom >= 4` 时，在画布上
  画文档像素边界的 1px 线（`DrawCommand::Line`，屏幕空间、不随缩放变粗），位置照
  `EditorView::paint_selection`；测试用 `RecordedFrame` 断言线的数量 / 位置随缩放
  变化，关闭或低缩放不画。
- [ ] **P4 — 像素完美连线（可选）**：1px 硬边笔改用 Bresenham 连接采样点，去掉
  重复压点导致的 `<100%` 加深与斜线 L 型加粗；测试 45° 斜线只覆盖 Bresenham
  上的像素。

已知取舍：软方形在整数尺寸上 `coverage` 会饱和成实心（没有灰边）；需要柔和方形
时再调整过渡带公式。

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

- **像素模式 P1/P2**：`BrushShape { Round, Square }` + `BrushTool { hard, shape }`
  （默认硬边方形，任意尺寸圆心吸附像素网格、`coverage` 二值）；选项栏加「像素」
  「方形」两个开关（`Rc<Cell>` + `dynamic_background`，`update` 同步进画笔）；
  `--selfcheck` 验证硬边无灰边 / 软边有灰边。
- 透明棋盘格背景（显示用合成，导出 PNG 仍保留 alpha）。
- 默认 128×128 像素画布 + 1px 硬边笔 + `TextureFilter::Nearest`。
- 移动图层后画笔对准光标；图层缓冲按「当前范围 ∪ 文档范围」扩展，
  **画布外像素不裁掉**，历史命令区域同步平移。
