//! 编辑器视图：菜单栏 + 工具栏 + 画布 + 图层/属性栏 + 状态栏。
//!
//! 视图是一棵 `SceneTree`：**画布是树里的一个 `Node2D`**（`Visual::Image`），
//! 相机变换就挂在它的 `Transform2D` 上，由 `draw_scene` 负责画；UI 仍由
//! `draw_components` 搭、`draw_ui` 排。宿主（[`crate::app::application`]）
//! 拥有窗口、wgpu 后端和纹理上传。
//!
//! ```text
//! Input -> EditorView::event -> EditorView::update -> layout -> paint
//! paint:  scene.paint(世界/画布)  ->  draw_ui::paint(UI 覆盖在上层)
//! ```
//!
//! 每块面板一个模块（[`menu`] / [`toolbar`] / [`canvas`] / [`layer_panel`] /
//! [`properties_panel`] / [`status_bar`]），这个文件负责把它们拼成整页、
//! 持有共享状态，并把 `CanvasCamera` 同步到文档节点的变换。

mod canvas;
mod icon_panel;
mod layer_panel;
mod menu;
mod properties_panel;
mod status_bar;
mod toolbar;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use draw_components::{set_text, Component, Flex, ListState, NodeRef};
use draw_core::{
    Edges, EventResult, InputEvent, Key, NodeId, PointerButton, Rect, Size, Vec2, ViewportSize,
};
use draw_render::PaintContext;
use draw_scene::{SceneChild, SceneTree, Visual};
use draw_theme::{space, SurfaceLevel, Theme};
use draw_ui::{MouseFilter, SizeBasis, TextMeasurer};

use crate::app::state::{ActiveTool, AppState, HistoryAction};
use crate::canvas::{document_to_pixel, screen_to_document, CanvasCamera, DOCUMENT_TEXTURE};
use crate::document::{LayerId, PixelBuffer};
use crate::icons::{self, IconSet};
use crate::renderer::{CpuRenderer, RenderTarget, Renderer};
use crate::tools::{BrushMode, BrushTool, PointerEvent, Tool, ToolContext};

/// 工具栏宽度（逻辑像素）。
const TOOLBAR_WIDTH: f32 = 52.0;
/// 右侧栏宽度（逻辑像素）。
const SIDEBAR_WIDTH: f32 = 280.0;
/// 适配时画布四周留的空白。
const FIT_PADDING: f32 = 24.0;
/// 一个滚轮刻度 / `+`/`-` 的缩放倍率。
const ZOOM_STEP: f32 = 1.25;

/// 需要在构建后回写的节点槽位。
#[derive(Default)]
struct Refs {
    tool: NodeRef,
    zoom: NodeRef,
    message: NodeRef,
    canvas: NodeRef,
    props_name: NodeRef,
    props_detail: NodeRef,
    icons: NodeRef,
}

/// 正在进行的图层重命名。
struct Rename {
    id: LayerId,
    buffer: String,
}

/// 状态栏同步快照：(工具, 缩放, 宽, 高, 指针下的文档像素)。
type StatusSnapshot = (ActiveTool, f32, u32, u32, Option<(u32, u32)>);

/// 编辑器视图。
pub struct EditorView {
    tree: SceneTree,
    theme: Theme,
    /// 应用状态。回调只往这里写，[`EditorView::update`] 负责同步到控件。
    state: Rc<RefCell<AppState>>,
    /// 菜单 / 占位按钮留下的提示（回调拿不到 `&mut self`，用共享格转交）。
    message: Rc<RefCell<Option<String>>>,
    tool_label: NodeId,
    zoom_label: NodeId,
    message_label: NodeId,
    /// 工具按钮节点。测试与自检靠它真的点一下。
    tool_nodes: Vec<(ActiveTool, NodeId)>,
    /// 撤销 / 重做按钮节点（Phase 6）。
    history_nodes: Vec<(HistoryAction, NodeId)>,
    /// 图标包里索引到的图标数量（自检 / 报告用）。
    icon_count: usize,
    /// 上一次同步到状态栏的内容，避免每帧都 `set_text`。
    shown: Option<StatusSnapshot>,
    /// 指针最近一次落在画布区域里的位置（用于状态栏的像素坐标）。
    pointer: Option<Vec2>,
    /// 世界层里的文档节点（`Node2D` + `Visual::Image`）；相机变换挂在它上面。
    document_node: NodeId,
    /// 透明画布区域控件，用来取 fit 区域和做滚轮 / 平移的命中测试。
    canvas_area: NodeId,
    renderer: CpuRenderer,
    render_target: RenderTarget,
    /// 文档变了、还没上传的合成结果。
    texture_dirty: bool,
    /// 是否已做过首次适配。
    fitted: bool,
    viewport: ViewportSize,
    /// 中键平移时上一次的指针位置。
    pan_last: Option<Vec2>,
    /// 画笔 / 橡皮引擎（同一个，靠 `mode` 区分）。
    brush: BrushTool,
    /// 图层列表的共享行数 / 选中行 / 状态。
    layer_count: Rc<Cell<usize>>,
    layer_selected: Rc<Cell<Option<usize>>>,
    layer_state: ListState,
    /// 最近一次见到的文档 `revision`；变了就重合成 + 刷新图层列表。
    last_revision: u64,
    /// “重命名”按钮点过之后置位，由 `update` 取走。
    rename_request: Rc<Cell<bool>>,
    renaming: Option<Rename>,
    props_name: NodeId,
    props_detail: NodeId,
    last_active: Option<LayerId>,
}

impl EditorView {
    /// 构建整棵视图树，并做一次初始同步。
    pub fn new(theme: Theme, state: AppState) -> Self {
        let refs = Refs::default();
        let document_size = (state.document.width, state.document.height);
        let state = Rc::new(RefCell::new(state));
        let message: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));

        let mut tool_refs: Vec<(ActiveTool, NodeRef)> = Vec::new();
        let mut history_refs: Vec<(HistoryAction, NodeRef)> = Vec::new();
        let toolbar = toolbar::tool_bar(
            theme,
            state.clone(),
            message.clone(),
            &mut tool_refs,
            &mut history_refs,
        );

        let layer_count = Rc::new(Cell::new(0usize));
        let layer_selected: Rc<Cell<Option<usize>>> = Rc::new(Cell::new(None));
        let rename_request = Rc::new(Cell::new(false));
        let layer_list = layer_panel::layer_list(
            theme,
            state.clone(),
            layer_count.clone(),
            layer_selected.clone(),
        );
        let layer_state = layer_list.state();

        // 布局根（SceneTree 根）的子节点是按 anchors 摆的，flex 从下一层才开始 ——
        // 所以页面 column 必须是根的唯一子节点（demo_app / file_browser 同款形状）。
        let page = Flex::column()
            .gap(0.0)
            .padding(Edges::ZERO)
            .mouse_filter(MouseFilter::Ignore)
            .child(menu::menu_bar(theme, message.clone()))
            .child(
                Flex::row()
                    .gap(0.0)
                    .padding(Edges::ZERO)
                    .grow(1.0)
                    .mouse_filter(MouseFilter::Ignore)
                    .child(toolbar)
                    .child(canvas::canvas_area().ref_(&refs.canvas))
                    .child(
                        Flex::column()
                            .basis(SizeBasis::Px(SIDEBAR_WIDTH))
                            .shrink(0.0)
                            .gap(space::SM)
                            .padding(Edges::all(space::SM))
                            .background(theme.surface(SurfaceLevel::Surface))
                            .mouse_filter(MouseFilter::Ignore)
                            .child(layer_panel::layer_panel(
                                theme,
                                state.clone(),
                                layer_list,
                                rename_request.clone(),
                            ))
                            .child(icon_panel::icon_panel(theme, &refs.icons))
                            .child(properties_panel::properties_panel(
                                theme,
                                &refs.props_name,
                                &refs.props_detail,
                            )),
                    ),
            )
            .child(status_bar::status_bar(
                theme,
                &refs.tool,
                &refs.message,
                &refs.zoom,
            ));

        let mut tree = Flex::new()
            .gap(0.0)
            .padding(Edges::ZERO)
            .mouse_filter(MouseFilter::Ignore)
            .child(page)
            .into_tree();

        // 画布是场景里的一个 `Node2D`：相机的 transform 就是这个节点的世界
        // 变换，`SceneTree::paint` 会把合成结果贴上去；纹理由宿主注册进后端。
        let document_node = tree.add_node2d(tree.root(), "Document");
        tree.set_visual(
            document_node,
            Visual::Image {
                texture: DOCUMENT_TEXTURE,
                size: Size::new(document_size.0 as f32, document_size.1 as f32),
            },
        );

        let tool_nodes: Vec<(ActiveTool, NodeId)> = tool_refs
            .into_iter()
            .map(|(tool, slot)| (tool, slot.get().expect("tool button mounted")))
            .collect();
        let history_nodes: Vec<(HistoryAction, NodeId)> = history_refs
            .into_iter()
            .map(|(action, slot)| (action, slot.get().expect("history button mounted")))
            .collect();

        // 图标包 -> 工具栏按钮 + 侧边栏网格。解析结果由 decorator 持有，每帧只重描边。
        let icon_set = IconSet::load();
        let icon_color = theme.palette.foreground;
        for (tool, node) in &tool_nodes {
            icon_set.attach_icon(&mut tree, *node, tool.icon(), icon_color);
        }
        for (action, node) in &history_nodes {
            icon_set.attach_icon(&mut tree, *node, action.icon(), icon_color);
        }
        if let Some(surface) = refs.icons.get() {
            icon_set.attach_grid(&mut tree, surface, &icons::ICON_NAMES, icon_color);
        }
        let icon_count = icon_set.len();

        let mut view = Self {
            tree,
            theme,
            state,
            message,
            tool_label: refs.tool.get().expect("tool label mounted"),
            zoom_label: refs.zoom.get().expect("zoom label mounted"),
            message_label: refs.message.get().expect("message label mounted"),
            tool_nodes,
            history_nodes,
            icon_count,
            shown: None,
            pointer: None,
            document_node,
            canvas_area: refs.canvas.get().expect("canvas area mounted"),
            renderer: CpuRenderer,
            render_target: RenderTarget::new(document_size.0, document_size.1),
            texture_dirty: true,
            fitted: false,
            viewport: ViewportSize::new(Size::new(1280.0, 800.0)),
            pan_last: None,
            brush: BrushTool::paint(),
            layer_count,
            layer_selected,
            layer_state,
            last_revision: u64::MAX,
            rename_request,
            renaming: None,
            props_name: refs.props_name.get().expect("props name mounted"),
            props_detail: refs.props_detail.get().expect("props detail mounted"),
            last_active: None,
        };
        view.update();
        view
    }

    // -- 生命周期 --------------------------------------------------------

    /// 用后端真实字体的度量，让排版量到的宽度跟画出来的宽度一致。
    pub fn set_text_measurer(&mut self, measurer: Rc<dyn TextMeasurer>) {
        draw_ui::set_text_measurer(&mut self.tree, measurer);
    }

    /// 宿主每帧调用：同步状态栏、在文档改动后刷新图层列表并标记重合成。
    pub fn update(&mut self) {
        self.sync_status();

        let (revision, active) = {
            let state = self.state.borrow();
            (state.document.revision(), state.document.active_layer)
        };
        if revision != self.last_revision {
            self.last_revision = revision;
            self.texture_dirty = true;
            self.refresh_layers();
            self.sync_properties();
        }
        if active != self.last_active {
            self.last_active = active;
            self.sync_properties();
        }

        if self.rename_request.replace(false) {
            self.begin_rename();
        }
        if self.renaming.is_some() {
            self.sync_rename_status();
        } else if let Some(message) = self.message.borrow_mut().take() {
            set_text(&mut self.tree, self.message_label, message);
        }
    }

    /// 工具 / 缩放 / 指针像素 -> 状态栏。
    fn sync_status(&mut self) {
        let (tool, zoom, width, height, camera) = {
            let state = self.state.borrow();
            (
                state.active_tool,
                state.canvas.zoom,
                state.document.width,
                state.document.height,
                state.canvas,
            )
        };
        // 指针在画布区域里时，状态栏顺便报一下文档像素坐标（§12 的坐标转换）。
        let pixel = self
            .pointer
            .and_then(|point| document_to_pixel(screen_to_document(camera, point), width, height));
        let shown = (tool, zoom, width, height, pixel);
        if self.shown != Some(shown) {
            set_text(&mut self.tree, self.tool_label, tool.label());
            let coords = match pixel {
                Some((x, y)) => format!("  ·  ({x}, {y})"),
                None => String::new(),
            };
            set_text(
                &mut self.tree,
                self.zoom_label,
                format!("{width} × {height}  ·  {:.0}%{coords}", zoom * 100.0),
            );
            self.shown = Some(shown);
        }
    }

    /// 文档改动后：更新列表行数与选中行，并让已挂载的行重新读数据。
    fn refresh_layers(&mut self) {
        let count = self.state.borrow().document.layers.len();
        self.layer_count.set(count);
        self.layer_selected.set(self.active_list_index());
        self.layer_state.invalidate();
    }

    /// 当前图层在“最上面为 0”的列表里的下标。
    fn active_list_index(&self) -> Option<usize> {
        let state = self.state.borrow();
        let document = &state.document;
        document
            .active_layer
            .and_then(|id| document.layer_index(id))
            .map(|index| document.layers.len() - 1 - index)
    }

    /// 当前图层 -> 属性面板。
    fn sync_properties(&mut self) {
        let (name, detail) = {
            let state = self.state.borrow();
            match state.document.active_layer() {
                Some(layer) => (
                    layer.name.clone(),
                    format!("不透明度 {:.0}%  ·  Normal", layer.opacity * 100.0),
                ),
                None => ("无图层".to_string(), "点「+ 图层」新建一层".to_string()),
            }
        };
        set_text(&mut self.tree, self.props_name, name);
        set_text(&mut self.tree, self.props_detail, detail);
    }

    fn begin_rename(&mut self) {
        let (id, name) = {
            let state = self.state.borrow();
            match state.document.active_layer() {
                Some(layer) => (layer.id, layer.name.clone()),
                None => return,
            }
        };
        self.renaming = Some(Rename { id, buffer: name });
    }

    fn cancel_rename(&mut self) {
        self.renaming = None;
        set_text(&mut self.tree, self.message_label, "");
    }

    fn commit_rename(&mut self) {
        let Some(rename) = self.renaming.take() else {
            return;
        };
        let name = rename.buffer.trim().to_string();
        if !name.is_empty() {
            self.state
                .borrow_mut()
                .document
                .rename_layer(rename.id, name);
        }
        set_text(&mut self.tree, self.message_label, "");
    }

    fn sync_rename_status(&mut self) {
        if let Some(rename) = &self.renaming {
            let text = format!("重命名图层：{}▌  （Enter 确认 · Esc 取消）", rename.buffer);
            set_text(&mut self.tree, self.message_label, text);
        }
    }

    /// 重命名编辑中的按键 / 文本输入；返回是否消费了事件。
    fn handle_rename_input(&mut self, event: &InputEvent) -> bool {
        match event {
            InputEvent::KeyDown { key: Key::Escape } => {
                self.cancel_rename();
                true
            }
            InputEvent::KeyDown { key: Key::Enter } => {
                self.commit_rename();
                true
            }
            InputEvent::KeyDown {
                key: Key::Backspace,
            } => {
                if let Some(rename) = &mut self.renaming {
                    rename.buffer.pop();
                }
                true
            }
            InputEvent::TextInput { text } => {
                if let Some(rename) = &mut self.renaming {
                    rename
                        .buffer
                        .extend(text.chars().filter(|ch| !ch.is_control()));
                }
                true
            }
            // 其余按键也吞掉：重命名时按字母不该触发工具 / 画布快捷键。
            InputEvent::KeyDown { .. } => true,
            _ => false,
        }
    }

    /// 排布整棵树，并把相机同步到文档节点。
    pub fn layout(&mut self, viewport: ViewportSize) {
        self.viewport = viewport;
        self.tree.set_viewport_size(viewport.logical_size());
        draw_ui::layout(&mut self.tree, viewport);
        // 第一次布局拿到画布区域的真实矩形，才能做适配。
        if !self.fitted {
            self.fit_to_canvas();
            self.fitted = true;
        }
        // 图层列表的行池按容器高度算，所以要在 layout 之后 sync；池变了再排一次。
        if self.layer_state.sync(&mut self.tree) {
            draw_ui::layout(&mut self.tree, viewport);
        }
        self.sync_document_node();
        self.tree.update();
    }

    /// 发出这一帧的绘制命令：先世界（文档图像），UI 覆盖在上层。
    pub fn paint(&self, ctx: &mut PaintContext) {
        self.tree.paint(ctx);
        draw_ui::paint(&self.tree, ctx);
    }

    /// 路由一个后端无关的输入事件。
    ///
    /// 工具快捷键、画布缩放 / 平移由视图先处理；其余（点击、指针）交给
    /// `draw_ui::handle_input`，由它去找带回调的控件。
    pub fn event(&mut self, event: &InputEvent) -> EventResult {
        // 重命名进行中：键盘只编辑名字，不触发工具 / 画布快捷键。
        if self.renaming.is_some() && self.handle_rename_input(event) {
            return EventResult::Handled;
        }
        match event {
            InputEvent::KeyDown { key } => {
                if let Some(tool) = shortcut_tool(*key) {
                    self.state.borrow_mut().active_tool = tool;
                    tracing::info!(target: "image_editor", tool = tool.label(), "tool_changed");
                    return EventResult::Handled;
                }
                if let Some(action) = canvas_shortcut(*key) {
                    self.apply_canvas_action(action);
                    return EventResult::Handled;
                }
                if let Some(delta) = brush_size_shortcut(*key) {
                    self.adjust_brush_size(delta);
                    return EventResult::Handled;
                }
                if let Some(delta) = brush_opacity_shortcut(*key) {
                    self.adjust_brush_opacity(delta);
                    return EventResult::Handled;
                }
            }
            InputEvent::Wheel { position, delta } => {
                if self.canvas_area_contains(*position) {
                    let factor = if delta.y < 0.0 {
                        ZOOM_STEP
                    } else {
                        1.0 / ZOOM_STEP
                    };
                    self.state.borrow_mut().canvas.zoom_at(*position, factor);
                    return EventResult::Handled;
                }
            }
            InputEvent::PointerDown {
                position,
                button: PointerButton::Left,
            } => {
                if self.canvas_area_contains(*position) {
                    self.begin_tool(*position);
                    return EventResult::Handled;
                }
            }
            InputEvent::PointerDown {
                position,
                button: PointerButton::Middle,
            } => {
                if self.canvas_area_contains(*position) {
                    self.pan_last = Some(*position);
                    return EventResult::Handled;
                }
            }
            InputEvent::PointerMove { position } => {
                self.pointer = self.canvas_area_contains(*position).then_some(*position);
                if let Some(last) = self.pan_last {
                    let delta = *position - last;
                    self.pan_last = Some(*position);
                    self.state.borrow_mut().canvas.pan_by(delta);
                    return EventResult::Handled;
                }
                if self.brush.is_drawing() {
                    self.continue_tool(*position);
                    return EventResult::Handled;
                }
            }
            InputEvent::PointerLeave => {
                self.pointer = None;
            }
            InputEvent::PointerUp {
                position,
                button: PointerButton::Left,
            } => {
                if self.brush.is_drawing() {
                    self.end_tool(*position);
                    return EventResult::Handled;
                }
            }
            InputEvent::PointerUp {
                button: PointerButton::Middle,
                ..
            } if self.pan_last.take().is_some() => {
                return EventResult::Handled;
            }
            _ => {}
        }
        draw_ui::handle_input(&mut self.tree, event)
    }

    // -- 画布 ------------------------------------------------------------

    /// 相机的变换 -> 文档节点；视觉尺寸跟随文档尺寸。
    fn sync_document_node(&mut self) {
        let (camera, size) = {
            let state = self.state.borrow();
            (
                state.canvas,
                Size::new(state.document.width as f32, state.document.height as f32),
            )
        };
        self.tree
            .set_transform(self.document_node, camera.transform());
        self.tree.set_visual(
            self.document_node,
            Visual::Image {
                texture: DOCUMENT_TEXTURE,
                size,
            },
        );
    }

    fn apply_canvas_action(&mut self, action: CanvasAction) {
        match action {
            CanvasAction::ZoomIn => self.zoom_by(ZOOM_STEP),
            CanvasAction::ZoomOut => self.zoom_by(1.0 / ZOOM_STEP),
            CanvasAction::Reset => self.reset_view(),
            CanvasAction::Fit => self.fit_to_canvas(),
        }
    }

    fn zoom_by(&mut self, factor: f32) {
        let anchor = self
            .canvas_area_rect()
            .map(|rect| rect.center())
            .unwrap_or_else(|| {
                Rect::from_min_size(Vec2::ZERO, self.viewport.logical_size()).center()
            });
        self.state.borrow_mut().canvas.zoom_at(anchor, factor);
    }

    /// 让文档适配画布区域并居中。
    pub fn fit_to_canvas(&mut self) {
        let Some(area) = self.canvas_area_rect() else {
            return;
        };
        let document = self.document_size();
        let camera = CanvasCamera::fit(
            area,
            Size::new(document.0 as f32, document.1 as f32),
            FIT_PADDING,
        );
        self.state.borrow_mut().canvas = camera;
    }

    /// 100% 且居中。
    fn reset_view(&mut self) {
        let Some(area) = self.canvas_area_rect() else {
            return;
        };
        let document = self.document_size();
        self.state.borrow_mut().canvas =
            CanvasCamera::centered(area, Size::new(document.0 as f32, document.1 as f32), 1.0);
    }

    fn canvas_area_rect(&self) -> Option<Rect> {
        draw_ui::control(&self.tree, self.canvas_area).map(|control| control.rect)
    }

    fn canvas_area_contains(&self, position: Vec2) -> bool {
        self.canvas_area_rect()
            .is_some_and(|rect| rect.contains(position))
    }

    // -- 工具 ------------------------------------------------------------

    /// 屏幕逻辑坐标 -> 文档坐标。
    fn document_position(&self, screen: Vec2) -> Vec2 {
        let camera = self.state.borrow().canvas;
        screen_to_document(camera, screen)
    }

    /// 左键落下：把屏幕点换算成文档坐标，交给当前工具。
    fn begin_tool(&mut self, screen: Vec2) {
        let tool = self.state.borrow().active_tool;
        match tool {
            ActiveTool::Brush | ActiveTool::Eraser => {
                self.brush.mode = if tool == ActiveTool::Eraser {
                    BrushMode::Erase
                } else {
                    BrushMode::Paint
                };
                let foreground = self.state.borrow().foreground;
                self.brush.color = foreground;
                tracing::debug!(target: "image_editor", tool = self.brush.name(), "stroke_started");
                let position = self.document_position(screen);
                let mut state = self.state.borrow_mut();
                let AppState {
                    document, history, ..
                } = &mut *state;
                let mut ctx = ToolContext { document, history };
                self.brush.on_pointer_down(
                    &mut ctx,
                    PointerEvent {
                        position,
                        button: PointerButton::Left,
                    },
                );
                self.texture_dirty = true;
            }
            other => {
                *self.message.borrow_mut() =
                    Some(format!("「{}」工具将在后续 Phase 实现", other.label()));
            }
        }
    }

    fn continue_tool(&mut self, screen: Vec2) {
        if !self.brush.is_drawing() {
            return;
        }
        let position = self.document_position(screen);
        let mut state = self.state.borrow_mut();
        let AppState {
            document, history, ..
        } = &mut *state;
        let mut ctx = ToolContext { document, history };
        self.brush.on_pointer_move(
            &mut ctx,
            PointerEvent {
                position,
                button: PointerButton::Left,
            },
        );
        self.texture_dirty = true;
    }

    fn end_tool(&mut self, screen: Vec2) {
        if !self.brush.is_drawing() {
            return;
        }
        let position = self.document_position(screen);
        let mut state = self.state.borrow_mut();
        let AppState {
            document, history, ..
        } = &mut *state;
        let mut ctx = ToolContext { document, history };
        self.brush.on_pointer_up(
            &mut ctx,
            PointerEvent {
                position,
                button: PointerButton::Left,
            },
        );
        self.texture_dirty = true;
    }

    fn adjust_brush_size(&mut self, delta: f32) {
        self.brush.size = (self.brush.size + delta).clamp(BrushTool::MIN_SIZE, BrushTool::MAX_SIZE);
        *self.message.borrow_mut() = Some(format!("笔刷大小 {:.0}px", self.brush.size));
    }

    fn adjust_brush_opacity(&mut self, delta: f32) {
        self.brush.opacity = (self.brush.opacity + delta).clamp(0.05, 1.0);
        *self.message.borrow_mut() =
            Some(format!("笔刷不透明度 {:.0}%", self.brush.opacity * 100.0));
    }

    // -- 访问器（宿主 / 测试 / 自检） -----------------------------------

    /// 当前激活的工具。
    pub fn active_tool(&self) -> ActiveTool {
        self.state.borrow().active_tool
    }

    /// 当前文档名字（宿主用它更新窗口标题）。
    pub fn document_name(&self) -> String {
        self.state.borrow().document.name.clone()
    }

    /// 当前文档像素尺寸。
    pub fn document_size(&self) -> (u32, u32) {
        let state = self.state.borrow();
        (state.document.width, state.document.height)
    }

    /// 当前画布相机。
    pub fn canvas_camera(&self) -> CanvasCamera {
        self.state.borrow().canvas
    }

    /// 取走待上传的文档合成结果；宿主把它注册成纹理。没有变化时返回 `None`。
    pub fn take_texture_upload(&mut self) -> Option<PixelBuffer> {
        if !self.texture_dirty {
            return None;
        }
        self.texture_dirty = false;
        let state = self.state.borrow();
        self.renderer
            .render(&state.document, &mut self.render_target);
        Some(self.render_target.pixels.clone())
    }

    /// 文档像素变了，需要重新合成 / 上传（Phase 4/5 的编辑会调用）。
    pub fn mark_texture_dirty(&mut self) {
        self.texture_dirty = true;
    }

    // -- 撤销 / 重做（Phase 6） -------------------------------------------

    /// 撤销一步，返回是否真的撤销了。
    pub fn undo(&mut self) -> bool {
        self.run_history(HistoryAction::Undo)
    }

    /// 重做一步，返回是否真的重做了。
    pub fn redo(&mut self) -> bool {
        self.run_history(HistoryAction::Redo)
    }

    fn run_history(&mut self, action: HistoryAction) -> bool {
        let result = {
            let mut state = self.state.borrow_mut();
            match action {
                HistoryAction::Undo => state.undo(),
                HistoryAction::Redo => state.redo(),
            }
        };
        let message = match result {
            Some(label) => {
                self.texture_dirty = true;
                format!("{}：{label}", action.label())
            }
            None => format!("没有可{}的操作", action.label()),
        };
        *self.message.borrow_mut() = Some(message);
        result.is_some()
    }

    pub fn can_undo(&self) -> bool {
        self.state.borrow().can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.state.borrow().can_redo()
    }

    /// 撤销 / 重做栈的长度，测试与自检用。
    pub fn history_len(&self) -> (usize, usize) {
        let state = self.state.borrow();
        (state.history.undo_len(), state.history.redo_len())
    }

    /// 图标包里索引到的图标数量（vendor 子集为 20，完整包为 2000+）。
    pub fn icon_count(&self) -> usize {
        self.icon_count
    }

    /// 主题（宿主用它取清屏色）。
    pub fn theme(&self) -> Theme {
        self.theme
    }

    /// 工具按钮的中心点（逻辑坐标）。测试与自检用它模拟一次真实点击。
    pub fn tool_center(&self, tool: ActiveTool) -> Option<Vec2> {
        let id = self.tool_node(tool)?;
        draw_ui::control(&self.tree, id).map(|control| control.rect.center())
    }

    /// 工具按钮的节点 id。
    pub fn tool_node(&self, tool: ActiveTool) -> Option<NodeId> {
        self.tool_nodes
            .iter()
            .find(|(candidate, _)| *candidate == tool)
            .map(|(_, id)| *id)
    }

    /// 撤销 / 重做按钮的中心点（逻辑坐标）；测试与自检模拟点击用。
    pub fn history_center(&self, action: HistoryAction) -> Option<Vec2> {
        let id = self.history_node(action)?;
        draw_ui::control(&self.tree, id).map(|control| control.rect.center())
    }

    /// 撤销 / 重做按钮的节点 id。
    pub fn history_node(&self, action: HistoryAction) -> Option<NodeId> {
        self.history_nodes
            .iter()
            .find(|(candidate, _)| *candidate == action)
            .map(|(_, id)| *id)
    }

    /// 控件数（自检的预算检查用）。
    pub fn control_count(&self) -> usize {
        draw_ui::control_count(&self.tree)
    }
}

/// 工具快捷键：`V` 移动、`B` 画笔、`E` 橡皮、`M` 框选、`I` 吸管。
fn shortcut_tool(key: Key) -> Option<ActiveTool> {
    match key {
        Key::Character('v') | Key::Character('V') => Some(ActiveTool::Move),
        Key::Character('b') | Key::Character('B') => Some(ActiveTool::Brush),
        Key::Character('e') | Key::Character('E') => Some(ActiveTool::Eraser),
        Key::Character('m') | Key::Character('M') => Some(ActiveTool::RectangleSelect),
        Key::Character('i') | Key::Character('I') => Some(ActiveTool::Eyedropper),
        _ => None,
    }
}

/// 画布动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CanvasAction {
    ZoomIn,
    ZoomOut,
    Reset,
    Fit,
}

/// 画布快捷键：`+`/`=` 放大、`-` 缩小、`0` 100%、`F` 适配。
fn canvas_shortcut(key: Key) -> Option<CanvasAction> {
    match key {
        Key::Character('+') | Key::Character('=') => Some(CanvasAction::ZoomIn),
        Key::Character('-') => Some(CanvasAction::ZoomOut),
        Key::Character('0') => Some(CanvasAction::Reset),
        Key::Character('f') | Key::Character('F') => Some(CanvasAction::Fit),
        _ => None,
    }
}

/// `[`/`]` 调整笔刷直径。
fn brush_size_shortcut(key: Key) -> Option<f32> {
    match key {
        Key::Character('[') => Some(-2.0),
        Key::Character(']') => Some(2.0),
        _ => None,
    }
}

/// `,`/`.` 调整笔刷不透明度。
fn brush_opacity_shortcut(key: Key) -> Option<f32> {
    match key {
        Key::Character(',') => Some(-0.05),
        Key::Character('.') => Some(0.05),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Color;
    use draw_core::{PointerButton, Size};
    use draw_render::DrawCommand;
    use draw_ui::Widget;

    fn viewport() -> ViewportSize {
        ViewportSize::new(Size::new(1280.0, 800.0))
    }

    fn click(view: &mut EditorView, position: Vec2) {
        draw_ui::handle_input(
            &mut view.tree,
            &InputEvent::PointerDown {
                position,
                button: PointerButton::Left,
            },
        );
        draw_ui::handle_input(
            &mut view.tree,
            &InputEvent::PointerUp {
                position,
                button: PointerButton::Left,
            },
        );
    }

    fn label_text(view: &EditorView, id: NodeId) -> String {
        match draw_ui::widget(&view.tree, id) {
            Some(Widget::Label { text, .. }) => text.clone(),
            _ => panic!("expected a label"),
        }
    }

    #[test]
    fn keyboard_shortcut_selects_a_tool() {
        let mut view = EditorView::new(Theme::dark(), AppState::default());
        view.layout(viewport());
        assert!(view
            .event(&InputEvent::KeyDown {
                key: Key::Character('b'),
            })
            .is_handled());
        view.update();
        assert_eq!(view.active_tool(), ActiveTool::Brush);
        assert_eq!(label_text(&view, view.tool_label), "画笔工具");
    }

    #[test]
    fn clicking_a_tool_button_selects_it() {
        let mut view = EditorView::new(Theme::dark(), AppState::default());
        view.layout(viewport());
        let center = view.tool_center(ActiveTool::Eraser).expect("eraser button");
        click(&mut view, center);
        view.update();
        assert_eq!(view.active_tool(), ActiveTool::Eraser);
        assert_eq!(label_text(&view, view.tool_label), "橡皮擦");
    }

    #[test]
    fn clicking_a_menu_reports_a_placeholder_message() {
        let mut view = EditorView::new(Theme::dark(), AppState::default());
        view.layout(viewport());
        // 菜单栏第一项是“文件”，点击它的中心。
        let text = "文件";
        let position = view
            .tree
            .iter()
            .find_map(|id| match draw_ui::widget(&view.tree, id) {
                Some(Widget::Label { text: label, .. }) if label == text => {
                    draw_ui::control(&view.tree, id).map(|c| c.rect.center())
                }
                _ => None,
            })
            .expect("file menu label");
        click(&mut view, position);
        view.update();
        assert!(
            label_text(&view, view.message_label).contains("占位"),
            "菜单占位提示应出现在状态栏"
        );
    }

    #[test]
    fn the_status_bar_survives_a_relayout_without_extra_text_churn() {
        let mut view = EditorView::new(Theme::dark(), AppState::default());
        // 首次布局会做适配，状态栏文字随之变化；先让它稳定下来。
        view.layout(viewport());
        view.update();
        view.layout(viewport());
        let before = draw_ui::layout_count(&view.tree);
        view.update();
        view.layout(viewport());
        assert_eq!(
            draw_ui::layout_count(&view.tree),
            before,
            "稳定后的一帧不该触发重排"
        );
    }

    fn paint_commands(view: &EditorView) -> Vec<DrawCommand> {
        let mut ctx = PaintContext::new();
        view.paint(&mut ctx);
        ctx.into_draw_list().into_commands()
    }

    #[test]
    fn the_scene_paints_the_document_image() {
        let mut view = EditorView::new(Theme::dark(), AppState::default());
        view.layout(viewport());
        assert!(
            paint_commands(&view).iter().any(|command| matches!(
                command,
                DrawCommand::DrawImage { texture, .. } if *texture == DOCUMENT_TEXTURE
            )),
            "画布应发出文档图像的 DrawImage"
        );
    }

    #[test]
    fn the_first_layout_fits_and_centers_the_document() {
        let mut view = EditorView::new(Theme::dark(), AppState::default());
        view.layout(viewport());
        let area = view.canvas_area_rect().expect("canvas area");
        let camera = view.canvas_camera();
        assert!(camera.zoom > 0.0);
        let center = camera.document_to_screen(Vec2::new(400.0, 300.0));
        assert!(
            (center.x - area.center().x).abs() < 0.5,
            "center = {center:?}"
        );
        assert!(
            (center.y - area.center().y).abs() < 0.5,
            "center = {center:?}"
        );
    }

    #[test]
    fn the_wheel_zooms_around_the_pointer() {
        let mut view = EditorView::new(Theme::dark(), AppState::default());
        view.layout(viewport());
        let point = view.canvas_area_rect().expect("canvas area").center();
        let before = view.canvas_camera();
        let anchor = before.screen_to_document(point);

        assert!(view
            .event(&InputEvent::Wheel {
                position: point,
                delta: Vec2::new(0.0, -40.0),
            })
            .is_handled());

        let after = view.canvas_camera();
        assert!(after.zoom > before.zoom, "向上滚应放大");
        assert!(
            (after.screen_to_document(point) - anchor).length_squared() < 0.25,
            "缩放应锚定在指针下的文档点"
        );
    }

    #[test]
    fn middle_drag_pans_the_canvas() {
        let mut view = EditorView::new(Theme::dark(), AppState::default());
        view.layout(viewport());
        let start = view.canvas_area_rect().expect("canvas area").center();
        let before = view.canvas_camera().offset;

        assert!(view
            .event(&InputEvent::PointerDown {
                position: start,
                button: PointerButton::Middle,
            })
            .is_handled());
        view.event(&InputEvent::PointerMove {
            position: start + Vec2::new(20.0, -10.0),
        });
        view.event(&InputEvent::PointerUp {
            position: start + Vec2::new(20.0, -10.0),
            button: PointerButton::Middle,
        });

        assert_eq!(view.canvas_camera().offset, before + Vec2::new(20.0, -10.0));
    }

    #[test]
    fn the_zoom_shortcuts_change_the_camera() {
        let mut view = EditorView::new(Theme::dark(), AppState::default());
        view.layout(viewport());
        let before = view.canvas_camera().zoom;
        assert!(view
            .event(&InputEvent::KeyDown {
                key: Key::Character('+'),
            })
            .is_handled());
        assert!(view.canvas_camera().zoom > before);
        assert!(view
            .event(&InputEvent::KeyDown {
                key: Key::Character('0'),
            })
            .is_handled());
        assert_eq!(view.canvas_camera().zoom, 1.0);
    }

    fn frame_texts(view: &EditorView) -> Vec<String> {
        paint_commands(view)
            .iter()
            .filter_map(|command| match command {
                DrawCommand::DrawText { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn adding_a_layer_refreshes_the_list_and_recomposites() {
        let mut view = EditorView::new(Theme::dark(), AppState::default());
        view.layout(viewport());
        view.state.borrow_mut().document.add_layer("新图层");
        view.update();
        view.layout(viewport());

        assert_eq!(view.layer_count.get(), 2);
        assert!(
            frame_texts(&view)
                .iter()
                .any(|text| text.contains("新图层")),
            "图层列表应出现新图层"
        );
        assert!(view.take_texture_upload().is_some(), "改动后要重传纹理");
    }

    #[test]
    fn the_properties_panel_follows_the_active_layer() {
        let mut view = EditorView::new(Theme::dark(), AppState::default());
        view.layout(viewport());
        // 新图层成为当前图层，属性面板应显示它的名字。
        view.state.borrow_mut().document.add_layer("上层");
        view.update();
        view.layout(viewport());
        assert!(frame_texts(&view).iter().any(|text| text.contains("上层")));
    }

    #[test]
    fn renaming_the_active_layer_uses_text_input() {
        let mut view = EditorView::new(Theme::dark(), AppState::default());
        view.layout(viewport());
        view.rename_request.set(true);
        view.update();
        assert!(view.renaming.is_some(), "应进入重命名");

        // 清掉“背景”再输入新名字。
        for _ in 0..8 {
            view.event(&InputEvent::KeyDown {
                key: Key::Backspace,
            });
        }
        view.event(&InputEvent::TextInput {
            text: "Hero".into(),
        });
        view.event(&InputEvent::KeyDown { key: Key::Enter });

        assert!(view.renaming.is_none(), "Enter 结束重命名");
        let name = view
            .state
            .borrow()
            .document
            .active_layer()
            .map(|layer| layer.name.clone());
        assert_eq!(name.as_deref(), Some("Hero"));
    }

    #[test]
    fn escape_cancels_a_rename() {
        let mut view = EditorView::new(Theme::dark(), AppState::default());
        view.layout(viewport());
        view.rename_request.set(true);
        view.update();
        view.event(&InputEvent::TextInput { text: "x".into() });
        view.event(&InputEvent::KeyDown { key: Key::Escape });
        assert!(view.renaming.is_none());
        assert_eq!(
            view.state.borrow().document.active_layer().unwrap().name,
            "背景",
            "取消后名字不变"
        );
    }

    #[test]
    fn a_left_drag_with_the_brush_paints_the_active_layer() {
        let mut view = EditorView::new(Theme::dark(), AppState::default());
        view.layout(viewport());
        view.event(&InputEvent::KeyDown {
            key: Key::Character('b'),
        });
        view.update();

        let point = view
            .canvas_camera()
            .document_to_screen(Vec2::new(400.0, 300.0));
        view.event(&InputEvent::PointerDown {
            position: point,
            button: PointerButton::Left,
        });
        view.event(&InputEvent::PointerMove {
            position: point + Vec2::new(40.0, 0.0),
        });
        view.event(&InputEvent::PointerUp {
            position: point + Vec2::new(40.0, 0.0),
            button: PointerButton::Left,
        });
        view.update();

        let painted = view
            .state
            .borrow()
            .document
            .active_layer()
            .unwrap()
            .pixels
            .get_pixel(400, 300);
        assert_eq!(painted, Color::BLACK);
        assert!(view.take_texture_upload().is_some(), "画笔改动要重传纹理");
    }

    #[test]
    fn the_eraser_clears_instead_of_painting() {
        let mut view = EditorView::new(Theme::dark(), AppState::default());
        view.layout(viewport());
        view.event(&InputEvent::KeyDown {
            key: Key::Character('e'),
        });
        view.update();

        let point = view
            .canvas_camera()
            .document_to_screen(Vec2::new(400.0, 300.0));
        view.event(&InputEvent::PointerDown {
            position: point,
            button: PointerButton::Left,
        });
        view.event(&InputEvent::PointerUp {
            position: point,
            button: PointerButton::Left,
        });

        let alpha = view
            .state
            .borrow()
            .document
            .active_layer()
            .unwrap()
            .pixels
            .get_pixel(400, 300)
            .a;
        assert_eq!(alpha, 0, "橡皮把背景擦透明");
    }

    /// 用画笔在画布中央画一笔（屏幕坐标 -> 文档坐标由视图换算）。
    fn paint_a_stroke(view: &mut EditorView) {
        view.event(&InputEvent::KeyDown {
            key: Key::Character('b'),
        });
        view.update();
        let point = view
            .canvas_camera()
            .document_to_screen(Vec2::new(400.0, 300.0));
        view.event(&InputEvent::PointerDown {
            position: point,
            button: PointerButton::Left,
        });
        view.event(&InputEvent::PointerMove {
            position: point + Vec2::new(40.0, 0.0),
        });
        view.event(&InputEvent::PointerUp {
            position: point + Vec2::new(40.0, 0.0),
            button: PointerButton::Left,
        });
        view.update();
    }

    #[test]
    fn a_brush_stroke_is_exactly_one_undo_step() {
        let mut view = EditorView::new(Theme::dark(), AppState::default());
        view.layout(viewport());
        paint_a_stroke(&mut view);

        assert!(view.can_undo());
        assert_eq!(view.history_len(), (1, 0));
        assert!(view.undo());
        assert_eq!(view.history_len(), (0, 1));
        assert!(view.can_redo());
        assert!(view.redo());
        assert_eq!(view.history_len(), (1, 0));
    }

    #[test]
    fn undo_restores_the_stroke_and_redo_reapplies_it_through_the_view() {
        let mut view = EditorView::new(Theme::dark(), AppState::default());
        view.layout(viewport());
        paint_a_stroke(&mut view);
        view.mark_texture_dirty();
        let painted = view.take_texture_upload().expect("脏文档应产出合成结果");
        assert_eq!(painted.get_pixel(400, 300), Color::BLACK);

        view.undo();
        let undone = view.take_texture_upload().expect("撤销后应重合成");
        assert_eq!(undone.get_pixel(400, 300), Color::WHITE);

        view.redo();
        let redone = view.take_texture_upload().expect("重做后应重合成");
        assert_eq!(redone.get_pixel(400, 300), Color::BLACK);
    }

    #[test]
    fn undo_and_redo_with_empty_history_are_no_ops() {
        let mut view = EditorView::new(Theme::dark(), AppState::default());
        view.layout(viewport());
        assert!(!view.can_undo());
        assert!(!view.undo());
        view.update();
        assert!(label_text(&view, view.message_label).contains("没有可撤销"));
        assert!(!view.redo());
        view.update();
        assert!(label_text(&view, view.message_label).contains("没有可重做"));
    }

    #[test]
    fn the_history_toolbar_buttons_undo_and_redo_a_stroke() {
        let mut view = EditorView::new(Theme::dark(), AppState::default());
        view.layout(viewport());
        paint_a_stroke(&mut view);
        assert_eq!(view.history_len(), (1, 0));

        let undo = view.history_center(HistoryAction::Undo).expect("撤销按钮");
        click(&mut view, undo);
        view.update();
        assert_eq!(view.history_len(), (0, 1));
        assert!(label_text(&view, view.message_label).contains("撤销"));

        let redo = view.history_center(HistoryAction::Redo).expect("重做按钮");
        click(&mut view, redo);
        view.update();
        assert_eq!(view.history_len(), (1, 0));
    }

    #[test]
    fn the_icon_gallery_loads_the_pack() {
        let view = EditorView::new(Theme::dark(), AppState::default());
        assert!(
            view.icon_count() >= icons::ICON_NAMES.len(),
            "图标包应至少包含展示的这组图标"
        );
    }

    #[test]
    fn icons_draw_into_the_painted_frame() {
        let mut view = EditorView::new(Theme::dark(), AppState::default());
        view.layout(viewport());
        // 图标用圆头 / 圆角，会发出 FillCircle；普通 UI 不画圆。
        assert!(
            paint_commands(&view)
                .iter()
                .any(|command| matches!(command, DrawCommand::FillCircle { .. })),
            "图标应发出 FillCircle"
        );
    }

    #[test]
    fn the_toolbar_packs_its_buttons_tightly() {
        let mut view = EditorView::new(Theme::dark(), AppState::default());
        view.layout(viewport());
        let first = view.tool_center(ActiveTool::ALL[0]).expect("first tool");
        let last = view
            .tool_center(*ActiveTool::ALL.last().unwrap())
            .expect("last tool");
        let pitch = (last.y - first.y) / (ActiveTool::ALL.len() - 1) as f32;
        // `Flex` 默认带 16px 内边距；忘记清零会让每一项高 32px、间距翻倍。
        assert!(pitch < 60.0, "工具栏每一项的间距应紧凑，实际 {pitch:.1}px");
    }
}
