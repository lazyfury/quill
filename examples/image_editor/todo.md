# image_editor — TODO

`examples/image_editor` 自己的项目级待办（这是一个独立 cargo workspace）。
**从上往下做**；每条写清「改哪里 / 怎么验收」。与 quill 主仓的路线图
（`docs/plan.md`、`docs/godot-migration.md`）分开，别混。

约定（见 `/AGENTS.md`）：每阶段 **API → 测试 → 实现 → 集成**，结束出报告等确认；
验证只允许程序化（DrawList 命令 / 布局矩形 / 回调 / 后端像素缓冲），**不做截图**。

最近落地：透明棋盘格背景；默认 128×128 像素画布 + 1px 硬边笔 + 最近邻采样；
移动图层后画笔对准光标、画布外像素不裁。

---

## 1. 主线：像素模式（已完成）

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
- [x] **P3 — 像素网格叠加**：像素模式开启且 `zoom >= GRID_MIN_ZOOM`(6) 时，在画布上
  画文档像素边界的 1px 线（`DrawCommand::Line`，屏幕空间、不随缩放变粗），只画
  「文档矩形 ∩ 画布区域」；测试断言放大+像素模式会多出网格线、缩小后不画。
- [x] **P4 — 像素完美连线**：1px 硬边笔改用 Bresenham 连接采样点（每个像素只压
  一次），去掉重复压点加深与斜线 L 型加粗；测试 45° 斜线正好 8 个像素。

已知取舍：软方形在整数尺寸上 `coverage` 会饱和成实心（没有灰边）；需要柔和方形
时再调整过渡带公式。

---

## 2. 紧接着

- [x] **CPU 合成成本收口**（`src/renderer/cpu.rs`）：图层缓冲现在可能比文档大，
  之前的慢路会遍历整块缓冲。改成只遍历「落在渲染目标里的那部分」，合成成本
  只跟文档尺寸有关，不再随缓冲增长（`DirtyRegion` 留给文档变大时再说）。
- [~] **棋盘格**：视图菜单加「显示棋盘格」开关（`MenuAction::ToggleCheckerboard`）；
  **屏幕空间恒定大小**仍未做（现在烘在显示纹理里，改成独立叠加改动偏大，暂缓）。
  文件：`src/canvas/checkerboard.rs`、`src/ui/mod.rs`。
- **最近邻只在 wgpu**（**暂不做，canvas 先不管**）：给 `draw_backend_canvas` 加
  `imageSmoothingEnabled` 等价开关。文件：`crates/draw_backend_canvas`（quill 主仓）。
- [x] **图层缓冲回收**：图层菜单加「裁到文档」（`Document::crop_layer_to_document`）：
  按 `position` 摆好后裁回文档尺寸、`position` 归零，丢掉画布外像素。
- **`--selfcheck` 覆盖**：棋盘格 / 移动映射目前只有单元测试。
- [x] **UI 暴露**：属性面板显示图层 `position` / 缓冲尺寸；视图菜单可开关棋盘格。

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

- **调色盘 + 历史面板**：**左侧独立调色盘面板**（`ui/palette.rs`，宽度可拖；
  前景 / 背景 + 16 个预设色块，点击设前景色）；右侧栏「历史」面板（虚拟化 `List`，
  `History::undo_labels`/`redo_labels`，点一步撤 / 重做到那里，`ui/history_panel.rs`）；
  右栏高度钳制改为三个可拖面板。
- **撤销补全 + 右侧栏可调**：移动工具（`SetLayerPositionCommand`）、新建 / 删除图层
  （`AddLayerCommand`/`RemoveLayerCommand`）、重命名 / 可见性 / 不透明度 / 排序
  （`LayerMetaCommand`，不克隆像素）、裁到文档（`CropLayerCommand`）全部入历史；
  「文件 / 图层 / 属性」三块之间加 `ResizeHandle::horizontal` 分隔条（可拖高度，
  `clamp_panel_heights` 保证图层不被挤没）。
- **棋盘格开关**：视图菜单「显示棋盘格」（可关，导出不受影响）。
- **图层菜单「裁到文档」**：`Document::crop_layer_to_document` 回收被移动撑大的缓冲
  （`cropping_a_layer_to_the_document_drops_off_canvas_pixels`）。
- **属性面板显示图层几何**：`偏移 (x, y) · 缓冲 W×H`（`the_properties_panel_shows_layer_geometry`）。
- **CPU 合成只遍历可见范围**（`src/renderer/cpu.rs`）：合成成本不再随图层缓冲尺寸
  增长（`a_layer_larger_than_the_document_composites_only_the_visible_part`）。
- **像素模式 P1–P4**：`BrushShape { Round, Square }` + `BrushTool { hard, shape }`
  （默认硬边方形，任意尺寸圆心吸附像素网格、`coverage` 二值）；选项栏加「像素」
  「方形」两个开关（`Rc<Cell>` + `dynamic_background`，`update` 同步进画笔）；
  像素模式 + `zoom >= 6` 画屏幕空间像素网格；1px 硬边笔走 Bresenham（无重复压点 /
  斜线加粗）。`--selfcheck` 验证硬边无灰边 / 软边有灰边。
- 透明棋盘格背景（显示用合成，导出 PNG 仍保留 alpha）。
- 默认 128×128 像素画布 + 1px 硬边笔 + `TextureFilter::Nearest`。
- 移动图层后画笔对准光标；图层缓冲按「当前范围 ∪ 文档范围」扩展，
  **画布外像素不裁掉**，历史命令区域同步平移。
