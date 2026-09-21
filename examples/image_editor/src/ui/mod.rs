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
mod card;
mod file_panel;
mod history_panel;
mod home;
mod layer_panel;
pub mod menu;
mod new_document;
mod options_bar;
mod palette;
mod properties_panel;
mod status_bar;
mod tabs;
mod toolbar;

pub use file_panel::IoAction;
pub use home::HomeView;
pub use new_document::{NewDocumentResult, NewDocumentSpec, NewDocumentView};
pub use options_bar::BrushAdjust;
pub use options_bar::BrushToggle;
pub use tabs::SidebarTab;

use std::cell::{Cell, RefCell};
use std::path::Path;
use std::rc::Rc;

use draw_components::{
    set_text, update_control, Component, Divider, Flex, ListState, NodeRef, Overlays, ResizeHandle,
};
use draw_core::{
    Edges, EventResult, InputEvent, Key, NodeId, PointerButton, Rect, Size, Vec2, ViewportSize,
};
use draw_render::{Paint, PaintContext};
use draw_scene::{SceneChild, SceneTree, Visual};
use draw_theme::{space, Theme};
use draw_ui::{MouseFilter, SizeBasis, TextMeasurer};

use crate::app::state::{ActiveTool, AppState, HistoryAction};
use crate::canvas::{
    document_to_pixel, paint_backdrop, pixel_selection, screen_to_document, CanvasCamera,
    DOCUMENT_TEXTURE,
};
use crate::document::{
    AddLayerCommand, CropLayerCommand, Layer, LayerId, LayerMetaCommand, PixelBuffer,
};
use crate::icons::IconSet;
use crate::renderer::{sample_pixel, CpuRenderer, RenderTarget, Renderer};
use crate::tools::{BrushMode, BrushShape, BrushTool, MoveTool, PointerEvent, Tool, ToolContext};
use crate::ui::options_bar::{options_bar, options_hint, tool_has_brush, OptionsRefs};
use crate::ui::tabs::TabsView;

/// 工具栏宽度（逻辑像素）。
const TOOLBAR_WIDTH: f32 = 52.0;
/// 左侧调色盘面板的宽度（可拖）。
const PALETTE_WIDTH: f32 = 172.0;
const PALETTE_MIN: f32 = 156.0;
const PALETTE_MAX: f32 = 250.0;
/// 右侧栏默认 / 最小宽度（逻辑像素）。
const SIDEBAR_WIDTH: f32 = 280.0;
const SIDEBAR_MIN: f32 = 200.0;
/// 画布区域的最小宽度：右栏拖动到再宽也不能把画布挤没。
const CANVAS_MIN: f32 = 160.0;
/// 分隔条的把手宽度（`ResizeHandle` 的默认值）。
const RESIZE_GUTTER: f32 = 6.0;
/// 适配时画布四周留的空白。
const FIT_PADDING: f32 = 24.0;
/// 一个滚轮刻度 / `+`/`-` 的缩放倍率。
const ZOOM_STEP: f32 = 1.25;
/// 像素模式下，出现像素网格的最小缩放。
const GRID_MIN_ZOOM: f32 = 6.0;
/// 右侧栏「文件 / 历史 / 属性」标签页面板的默认高度 / 拖拽钳制（逻辑像素）。
const TABS_PANEL_HEIGHT: f32 = 220.0;
const TABS_PANEL_MIN: f32 = 120.0;
const TABS_PANEL_MAX: f32 = 480.0;
/// 「图层」面板保留的最小高度（上面的分隔条不能把它挤没）。
const LAYER_PANEL_MIN: f32 = 120.0;

/// 需要在构建后回写的节点槽位。
#[derive(Default)]
struct Refs {
    tool: NodeRef,
    zoom: NodeRef,
    message: NodeRef,
    canvas: NodeRef,
    props_name: NodeRef,
    props_detail: NodeRef,
    props_geometry: NodeRef,
    tabs_panel: NodeRef,
    tabs_handle: NodeRef,
    palette_panel: NodeRef,
    palette_picker: NodeRef,
    palette_handle: NodeRef,
    path: NodeRef,
    sidebar: NodeRef,
    sidebar_handle: NodeRef,
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
    palette_nodes: Vec<NodeId>,
    /// 文件面板的按钮节点（Phase 7）。
    file_nodes: Vec<(IoAction, NodeId)>,
    /// 导入 / 导出的路径，文件面板与内联编辑共享。
    path: Rc<RefCell<String>>,
    /// 文件面板按钮留下的动作请求；`update` 取走执行。
    io_request: Rc<Cell<Option<IoAction>>>,
    /// 菜单标题留下的“打开第几个菜单”请求；`update` 取走打开下拉。
    menu_request: Rc<Cell<Option<usize>>>,
    /// 菜单项留下的动作请求；`update` 取走执行（并关闭菜单）。
    menu_action: Rc<Cell<Option<menu::MenuAction>>>,
    /// 菜单标题的节点（下拉的锚点），顺序与 [`menu::MENUS`] 一致。
    menu_nodes: Vec<NodeId>,
    /// 当前打开菜单的下标（用于点同一标题时切换成关闭）。
    open_menu: Cell<Option<usize>>,
    /// 覆盖层：承载菜单下拉，画在 UI 之上并优先接收输入。
    overlays: Overlays,
    /// 工具选项栏的文本 / 容器节点。
    options_tool: NodeId,
    options_brush: NodeId,
    options_size: NodeId,
    options_opacity: NodeId,
    options_hint: NodeId,
    /// 选项栏 `−` / `+` 留下的请求；`update` 取走执行。
    brush_request: Rc<Cell<Option<BrushAdjust>>>,
    /// `−` / `+` 按钮节点（测试 / 自检点击用）。
    brush_buttons: Vec<(BrushAdjust, NodeId)>,
    /// 像素模式 / 方形笔的共享开关状态（选项栏按钮直接翻转，`update` 同步进
    /// `self.brush`）。
    pixel_mode: Rc<Cell<bool>>,
    square_mode: Rc<Cell<bool>>,
    /// 开关按钮节点（测试 / 自检点击用）。
    brush_toggles: Vec<(BrushToggle, NodeId)>,
    /// 右侧栏共享宽度（分隔条写、`clamp_sidebar_width` 也写）。
    sidebar_width: Rc<Cell<f32>>,
    /// 右侧栏节点（分隔条的目标）。
    sidebar_node: NodeId,
    /// 右侧栏分隔条的节点。
    sidebar_handle_node: NodeId,
    /// 左侧调色盘面板的宽度 / 节点 / 分隔条。
    palette_width: Rc<Cell<f32>>,
    palette_panel_node: NodeId,
    palette_picker_node: NodeId,
    palette_handle_node: NodeId,
    /// 侧栏标签页：当前页（按钮写）、面板高度 / 节点 / 分隔条。
    active_tab: Rc<Cell<SidebarTab>>,
    /// 上一次同步到树的标签页；变了才重设可见性 + 标脏重排。
    shown_tab: Option<SidebarTab>,
    tabs_height: Rc<Cell<f32>>,
    tabs_panel_node: NodeId,
    tabs_handle_node: NodeId,
    /// 标签按钮 / 内容容器节点，测试与自检靠它们真的点一下。
    tab_buttons: Vec<(SidebarTab, NodeId)>,
    tab_contents: Vec<(SidebarTab, NodeId)>,
    /// 路径标签节点，以及上一次写入的内容（避免每帧刷文本）。
    path_label: NodeId,
    shown_path: Option<String>,
    /// 路径内联编辑的缓冲区（`Some` = 正在编辑）。
    path_edit: Option<String>,
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
    /// 显示用的透明棋盘格背景是否开启。
    show_checkerboard: bool,
    /// 是否已做过首次适配。
    fitted: bool,
    viewport: ViewportSize,
    /// 中键平移时上一次的指针位置。
    pan_last: Option<Vec2>,
    /// 画笔 / 橡皮引擎（同一个，靠 `mode` 区分）。
    brush: BrushTool,
    /// 移动工具（Phase 8）。
    move_tool: MoveTool,
    /// 框选拖拽起点（文档坐标）；`Some` = 正在框选。
    select_anchor: Option<Vec2>,
    /// 图层列表的共享行数 / 选中行 / 状态。
    layer_count: Rc<Cell<usize>>,
    layer_selected: Rc<Cell<Option<usize>>>,
    layer_state: ListState,
    /// 历史列表的共享行数 / 状态；历史长度变了就刷新。
    history_count: Rc<Cell<usize>>,
    history_state: ListState,
    /// 最近一次见到的 (undo, redo) 长度；变了就刷新历史面板。
    last_history: Option<(usize, usize)>,
    /// 最近一次见到的文档 `revision`；变了就重合成 + 刷新图层列表。
    last_revision: u64,
    /// “重命名”按钮点过之后置位，由 `update` 取走。
    rename_request: Rc<Cell<bool>>,
    renaming: Option<Rename>,
    props_name: NodeId,
    props_detail: NodeId,
    props_geometry: NodeId,
    last_active: Option<LayerId>,
}

impl EditorView {
    /// 构建整棵视图树，并做一次初始同步。
    pub fn new(theme: Theme, state: AppState) -> Self {
        let refs = Refs::default();
        let document_size = (state.document.width, state.document.height);
        let document_name = state.document.name.clone();
        let state = Rc::new(RefCell::new(state));
        let message: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
        // 默认导出到当前目录 + 文档名；用户可在文件面板里改。
        let path: Rc<RefCell<String>> = Rc::new(RefCell::new(
            crate::io::default_export_path(&document_name)
                .to_string_lossy()
                .into_owned(),
        ));
        let io_request: Rc<Cell<Option<IoAction>>> = Rc::new(Cell::new(None));
        let menu_request: Rc<Cell<Option<usize>>> = Rc::new(Cell::new(None));
        let menu_action: Rc<Cell<Option<menu::MenuAction>>> = Rc::new(Cell::new(None));
        let mut menu_refs: Vec<NodeRef> = Vec::new();
        let mut file_refs: Vec<(IoAction, NodeRef)> = Vec::new();
        let file_panel = file_panel::file_panel(
            theme,
            path.clone(),
            io_request.clone(),
            &refs.path,
            &mut file_refs,
        );

        let mut tool_refs: Vec<(ActiveTool, NodeRef)> = Vec::new();
        let mut history_refs: Vec<(HistoryAction, NodeRef)> = Vec::new();
        let mut palette_refs: Vec<NodeRef> = Vec::new();
        // 图标包只加载一次；工具栏的按钮在构建时就把 `Icon` 子组件搭进去。
        let icons = Rc::new(IconSet::load());
        let toolbar = toolbar::tool_bar(
            theme,
            state.clone(),
            message.clone(),
            icons.clone(),
            &mut tool_refs,
            &mut history_refs,
        );
        let palette_panel = palette::palette_panel(
            theme,
            state.clone(),
            &mut palette_refs,
            &refs.palette_picker,
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

        let history_count = Rc::new(Cell::new(0usize));
        let history_list = history_panel::history_list(theme, state.clone(), history_count.clone());
        let history_state = history_list.state();

        // 工具选项栏（菜单栏下面一行）+ 可拖动的右栏。
        let options = OptionsRefs::default();
        let brush_request: Rc<Cell<Option<BrushAdjust>>> = Rc::new(Cell::new(None));
        let mut brush_refs: Vec<(BrushAdjust, NodeRef)> = Vec::new();
        let pixel_mode = Rc::new(Cell::new(true));
        let square_mode = Rc::new(Cell::new(true));
        let mut toggle_refs: Vec<(BrushToggle, NodeRef)> = Vec::new();
        let sidebar_width = Rc::new(Cell::new(SIDEBAR_WIDTH));
        let palette_width = Rc::new(Cell::new(PALETTE_WIDTH));
        let active_tab: Rc<Cell<SidebarTab>> = Rc::new(Cell::new(SidebarTab::default()));
        let tabs_height = Rc::new(Cell::new(TABS_PANEL_HEIGHT));

        // 侧栏标签页：文件 / 历史 / 属性共用一个卡片。按钮与内容容器各留一个
        // `NodeRef`，建完后收进 `tab_buttons` / `tab_contents` 供交互与可见性同步。
        let properties = properties_panel::properties_panel(
            theme,
            &refs.props_name,
            &refs.props_detail,
            &refs.props_geometry,
        );
        let mut tab_refs: Vec<(SidebarTab, NodeRef, NodeRef)> = Vec::new();
        let mut tabs_view = TabsView::new(theme, active_tab.clone());
        let file_button = NodeRef::new();
        let file_content = NodeRef::new();
        tabs_view = tabs_view.tab(SidebarTab::File, &file_button, &file_content, file_panel);
        tab_refs.push((SidebarTab::File, file_button, file_content));
        let history_button = NodeRef::new();
        let history_content = NodeRef::new();
        tabs_view = tabs_view.tab(
            SidebarTab::History,
            &history_button,
            &history_content,
            history_list,
        );
        tab_refs.push((SidebarTab::History, history_button, history_content));
        let props_button = NodeRef::new();
        let props_content = NodeRef::new();
        tabs_view = tabs_view.tab(
            SidebarTab::Properties,
            &props_button,
            &props_content,
            properties,
        );
        tab_refs.push((SidebarTab::Properties, props_button, props_content));

        // 布局根（SceneTree 根）的子节点是按 anchors 摆的，flex 从下一层才开始 ——
        // 所以页面 column 必须是根的唯一子节点（demo_app / file_browser 同款形状）。
        let page = Flex::column()
            .gap(0.0)
            .padding(Edges::ZERO)
            .mouse_filter(MouseFilter::Ignore)
            .child(menu::menu_bar(theme, menu_request.clone(), &mut menu_refs))
            .child(options_bar(
                theme,
                brush_request.clone(),
                pixel_mode.clone(),
                square_mode.clone(),
                &options,
                &mut brush_refs,
                &mut toggle_refs,
            ))
            .child(
                Flex::row()
                    .gap(0.0)
                    .padding(Edges::ZERO)
                    .grow(1.0)
                    .mouse_filter(MouseFilter::Ignore)
                    .child(toolbar)
                    .child(Divider::vertical(theme))
                    .child(
                        Flex::row()
                            .background(theme.background())
                            .padding(Edges::ZERO)
                            .gap(0.0)
                            .child(
                                // 调色盘外面留一圈边距，让它像右栏那些卡片一样“浮”在底色上，
                                // 而不是一条贴着工具栏的通栏面板。
                                Flex::column()
                                    .basis(SizeBasis::Px(palette_width.get()))
                                    .shrink(0.0)
                                    .padding(Edges::ZERO)
                                    .mouse_filter(MouseFilter::Ignore)
                                    .child(palette_panel)
                                    .ref_(&refs.palette_panel),
                            )
                            .child(
                                ResizeHandle::vertical(theme)
                                    .target(refs.palette_panel.clone())
                                    .width(palette_width.clone())
                                    .min(PALETTE_MIN)
                                    .max(PALETTE_MAX)
                                    .ref_(&refs.palette_handle),
                            ),
                    )
                    .child(canvas::canvas_area().ref_(&refs.canvas))
                    .child(
                        Flex::row()
                            .background(theme.background())
                            .padding(Edges::ZERO)
                            .gap(0.0)
                            .child(
                                ResizeHandle::vertical(theme)
                                    .target(refs.sidebar.clone())
                                    .width(sidebar_width.clone())
                                    .min(SIDEBAR_MIN)
                                    .invert()
                                    .ref_(&refs.sidebar_handle),
                            )
                            .child(
                                Flex::column()
                                    .basis(SizeBasis::Px(sidebar_width.get()))
                                    .shrink(0.0)
                                    .gap(space::XXXS)
                                    .padding(Edges::ZERO)
                                    .mouse_filter(MouseFilter::Ignore)
                                    .child(
                                        tabs_view
                                            .basis(SizeBasis::Px(tabs_height.get()))
                                            .shrink(0.0)
                                            .ref_(&refs.tabs_panel),
                                    )
                                    .child(
                                        ResizeHandle::horizontal(theme)
                                            .target(refs.tabs_panel.clone())
                                            .width(tabs_height.clone())
                                            .min(TABS_PANEL_MIN)
                                            .max(TABS_PANEL_MAX)
                                            .ref_(&refs.tabs_handle),
                                    )
                                    .child(layer_panel::layer_panel(
                                        theme,
                                        state.clone(),
                                        layer_list,
                                        rename_request.clone(),
                                    ))
                                    .ref_(&refs.sidebar),
                            ),
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
        let palette_nodes: Vec<NodeId> = palette_refs
            .iter()
            .map(|slot| slot.get().expect("palette swatch mounted"))
            .collect();
        let file_nodes: Vec<(IoAction, NodeId)> = file_refs
            .into_iter()
            .map(|(action, slot)| (action, slot.get().expect("file button mounted")))
            .collect();
        let menu_nodes: Vec<NodeId> = menu_refs
            .iter()
            .map(|slot| slot.get().expect("menu button mounted"))
            .collect();
        let brush_buttons: Vec<(BrushAdjust, NodeId)> = brush_refs
            .into_iter()
            .map(|(adjust, slot)| (adjust, slot.get().expect("brush button mounted")))
            .collect();
        let brush_toggles: Vec<(BrushToggle, NodeId)> = toggle_refs
            .into_iter()
            .map(|(toggle, slot)| (toggle, slot.get().expect("brush toggle mounted")))
            .collect();
        let tab_buttons: Vec<(SidebarTab, NodeId)> = tab_refs
            .iter()
            .map(|(tab, button, _)| (*tab, button.get().expect("tab button mounted")))
            .collect();
        let tab_contents: Vec<(SidebarTab, NodeId)> = tab_refs
            .iter()
            .map(|(tab, _, content)| (*tab, content.get().expect("tab content mounted")))
            .collect();

        // 图标包在构建按钮时已被各 `Icon` 组件解析并持有（decorator 里），这里
        // 只留个数给自检 / 报告用。
        let icon_count = icons.len();

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
            palette_nodes,
            file_nodes,
            path,
            io_request,
            menu_request,
            menu_action,
            menu_nodes,
            open_menu: Cell::new(None),
            overlays: Overlays::new(theme),
            options_tool: options.tool.get().expect("options tool mounted"),
            options_brush: options.brush.get().expect("options brush mounted"),
            options_size: options.size.get().expect("options size mounted"),
            options_opacity: options.opacity.get().expect("options opacity mounted"),
            options_hint: options.hint.get().expect("options hint mounted"),
            brush_request,
            brush_buttons,
            pixel_mode,
            square_mode,
            brush_toggles,
            sidebar_width,
            sidebar_node: refs.sidebar.get().expect("sidebar mounted"),
            sidebar_handle_node: refs.sidebar_handle.get().expect("sidebar handle mounted"),
            palette_width,
            palette_panel_node: refs.palette_panel.get().expect("palette panel mounted"),
            palette_picker_node: refs.palette_picker.get().expect("palette picker mounted"),
            palette_handle_node: refs.palette_handle.get().expect("palette handle mounted"),
            active_tab,
            shown_tab: None,
            tabs_height,
            tabs_panel_node: refs.tabs_panel.get().expect("tabs panel mounted"),
            tabs_handle_node: refs.tabs_handle.get().expect("tabs handle mounted"),
            tab_buttons,
            tab_contents,
            path_label: refs.path.get().expect("path label mounted"),
            shown_path: None,
            path_edit: None,
            icon_count,
            shown: None,
            pointer: None,
            document_node,
            canvas_area: refs.canvas.get().expect("canvas area mounted"),
            renderer: CpuRenderer,
            render_target: RenderTarget::new(document_size.0, document_size.1),
            texture_dirty: true,
            show_checkerboard: true,
            fitted: false,
            viewport: ViewportSize::new(Size::new(1280.0, 800.0)),
            pan_last: None,
            brush: BrushTool::paint(),
            move_tool: MoveTool::new(),
            select_anchor: None,
            layer_count,
            layer_selected,
            layer_state,
            history_count,
            history_state,
            last_history: None,
            last_revision: u64::MAX,
            rename_request,
            renaming: None,
            props_name: refs.props_name.get().expect("props name mounted"),
            props_detail: refs.props_detail.get().expect("props detail mounted"),
            props_geometry: refs.props_geometry.get().expect("props geometry mounted"),
            last_active: None,
        };
        view.update();
        view
    }

    // -- 生命周期 --------------------------------------------------------

    /// 用后端真实字体的度量，让排版量到的宽度跟画出来的宽度一致。
    pub fn set_text_measurer(&mut self, measurer: Rc<dyn TextMeasurer>) {
        draw_ui::set_text_measurer(&mut self.tree, measurer.clone());
        self.overlays.set_text_measurer(measurer);
    }

    /// 宿主每帧调用：同步状态栏、在文档改动后刷新图层列表并标记重合成。
    pub fn update(&mut self) {
        // 文件面板的请求先执行：导入会改文档，之后的 revision 检查会刷新
        // 图层列表与合成。
        if let Some(action) = self.io_request.take() {
            self.handle_io(action);
        }
        self.sync_status();
        self.sync_path();
        if let Some(adjust) = self.brush_request.take() {
            self.apply_brush_adjust(adjust);
        }
        // 选项栏开关是显示源，`brush` 是绘制源，这里每帧对齐。
        self.brush.hard = self.pixel_mode.get();
        self.brush.shape = if self.square_mode.get() {
            BrushShape::Square
        } else {
            BrushShape::Round
        };
        self.sync_options();

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
        let history = {
            let state = self.state.borrow();
            (state.history.undo_len(), state.history.redo_len())
        };
        if self.last_history != Some(history) {
            self.last_history = Some(history);
            self.refresh_history();
        }

        if self.rename_request.replace(false) {
            self.begin_rename();
        }
        if let Some(action) = self.menu_action.take() {
            self.overlays.close_all();
            self.apply_menu_action(action);
        }
        if let Some(index) = self.menu_request.take() {
            self.open_menu(index);
        }
        // Esc / 点外部已经关掉了菜单时，清掉记录的下标。
        if self.overlays.is_empty() {
            self.open_menu.set(None);
        }
        if self.renaming.is_some() {
            self.sync_rename_status();
        } else if let Some(message) = self.message.borrow_mut().take() {
            set_text(&mut self.tree, self.message_label, message);
        }
        self.sync_tabs();
    }

    /// 标签页切换：只让当前内容参与布局与绘制（隐藏内容不可见，也不命中）。
    /// 激活页没变时是空操作，避免每帧重排。
    fn sync_tabs(&mut self) {
        let active = self.active_tab.get();
        if self.shown_tab == Some(active) {
            return;
        }
        self.shown_tab = Some(active);
        tabs::show_active(
            &mut self.tree,
            self.tabs_panel_node,
            active,
            &self.tab_contents,
        );
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

    /// 历史长度变化后刷新历史面板的行数。
    fn refresh_history(&mut self) {
        let (undo, redo) = {
            let state = self.state.borrow();
            (state.history.undo_len(), state.history.redo_len())
        };
        self.history_count.set(undo + 1 + redo);
        self.history_state.invalidate();
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
        let (name, detail, geometry) = {
            let state = self.state.borrow();
            match state.document.active_layer() {
                Some(layer) => (
                    layer.name.clone(),
                    format!("不透明度 {:.0}%  ·  Normal", layer.opacity * 100.0),
                    format!(
                        "偏移 ({}, {})  ·  缓冲 {}×{}",
                        layer.position.x, layer.position.y, layer.pixels.width, layer.pixels.height
                    ),
                ),
                None => (
                    "无图层".to_string(),
                    "点「+ 图层」新建一层".to_string(),
                    String::new(),
                ),
            }
        };
        set_text(&mut self.tree, self.props_name, name);
        set_text(&mut self.tree, self.props_detail, detail);
        set_text(&mut self.tree, self.props_geometry, geometry);
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
            let mut state = self.state.borrow_mut();
            let before = LayerMetaCommand::capture(&state.document);
            state.document.rename_layer(rename.id, name);
            let after = LayerMetaCommand::capture(&state.document);
            state.execute(Box::new(LayerMetaCommand::new(before, after, "重命名图层")));
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

    // -- 文件 / 路径（Phase 7） ------------------------------------------

    /// 路径标签每帧同步：编辑中显示缓冲区 + 光标，否则显示当前路径。
    fn sync_path(&mut self) {
        let display = match &self.path_edit {
            Some(buffer) => format!("{buffer}▌"),
            None => self.path.borrow().clone(),
        };
        if self.shown_path.as_deref() != Some(display.as_str()) {
            set_text(&mut self.tree, self.path_label, display.clone());
            self.shown_path = Some(display);
        }
    }

    /// 执行一个文件动作（由文件面板按钮请求触发）。
    fn handle_io(&mut self, action: IoAction) {
        match action {
            IoAction::EditPath => {
                self.renaming = None;
                self.path_edit = Some(self.path.borrow().clone());
                *self.message.borrow_mut() = Some("编辑路径：Enter 确认 · Esc 取消".to_string());
            }
            IoAction::Export => {
                let path = self.io_path();
                if path.trim().is_empty() {
                    *self.message.borrow_mut() = Some("先点「改路径」设置导出路径".to_string());
                    return;
                }
                let message = match self.export_png_to(&path) {
                    Ok(()) => {
                        tracing::info!(target: "image_editor", path = path.as_str(), "png_exported");
                        format!("已导出 PNG：{path}")
                    }
                    Err(error) => format!("导出失败：{error}"),
                };
                *self.message.borrow_mut() = Some(message);
            }
            IoAction::Import => {
                let path = self.io_path();
                if path.trim().is_empty() {
                    *self.message.borrow_mut() = Some("先点「改路径」设置导入路径".to_string());
                    return;
                }
                let message = match self.import_png_from(&path) {
                    Ok((width, height)) => {
                        tracing::info!(
                            target: "image_editor",
                            path = path.as_str(),
                            width,
                            height,
                            "png_imported"
                        );
                        format!("已导入 PNG：{path}（{width} × {height}）")
                    }
                    Err(error) => format!("导入失败：{error}"),
                };
                *self.message.borrow_mut() = Some(message);
            }
        }
    }

    /// 命中位置落在哪个菜单标题上（含其子节点）；用于一次点击切换菜单。
    fn menu_index_at(&self, position: Vec2) -> Option<usize> {
        let hit = draw_ui::hit_test(&self.tree, position)?;
        let mut current = Some(hit);
        while let Some(node) = current {
            if let Some(index) = self.menu_nodes.iter().position(|id| *id == node) {
                return Some(index);
            }
            current = self.tree.parent(node);
        }
        None
    }

    /// 打开第 `index` 个菜单的下拉，锚在对应标题按钮下。
    fn open_menu(&mut self, index: usize) {
        let Some(&anchor) = self.menu_nodes.get(index) else {
            return;
        };
        self.overlays.close_all();
        let theme = self.theme;
        let action = self.menu_action.clone();
        let (can_undo, can_redo, has_selection) = {
            let state = self.state.borrow();
            (
                state.can_undo(),
                state.can_redo(),
                state.selection.is_some(),
            )
        };
        self.overlays.menu(anchor, move |tree, root| {
            menu::menu_content(
                tree,
                root,
                theme,
                index,
                action.clone(),
                can_undo,
                can_redo,
                has_selection,
            );
        });
        self.open_menu.set(Some(index));
    }

    /// 执行一个菜单动作（[`EditorView::update`] 在关闭菜单后调用）。
    fn apply_menu_action(&mut self, action: menu::MenuAction) {
        match action {
            menu::MenuAction::Undo => {
                self.undo();
            }
            menu::MenuAction::Redo => {
                self.redo();
            }
            menu::MenuAction::Import => self.handle_io(IoAction::Import),
            menu::MenuAction::Export => self.handle_io(IoAction::Export),
            menu::MenuAction::ZoomIn => self.apply_canvas_action(CanvasAction::ZoomIn),
            menu::MenuAction::ZoomOut => self.apply_canvas_action(CanvasAction::ZoomOut),
            menu::MenuAction::ZoomReset => self.apply_canvas_action(CanvasAction::Reset),
            menu::MenuAction::ZoomFit => self.apply_canvas_action(CanvasAction::Fit),
            menu::MenuAction::ClearSelection => {
                self.clear_selection();
            }
            menu::MenuAction::CropLayerToDocument => self.crop_layer_to_document(),
            menu::MenuAction::ToggleCheckerboard => {
                self.show_checkerboard = !self.show_checkerboard;
                self.texture_dirty = true;
                *self.message.borrow_mut() = Some(if self.show_checkerboard {
                    "已显示棋盘格".to_string()
                } else {
                    "已隐藏棋盘格".to_string()
                });
            }
            menu::MenuAction::About => {
                *self.message.borrow_mut() = Some(menu::about_text());
            }
            menu::MenuAction::Placeholder(note) => {
                *self.message.borrow_mut() = Some(note.to_string());
            }
        }
    }

    /// 把当前图层裁到文档大小（丢弃画布外像素，回收缓冲区）。
    fn crop_layer_to_document(&mut self) {
        let mut state = self.state.borrow_mut();
        let Some(layer) = state.document.active_layer() else {
            *self.message.borrow_mut() = Some("没有可裁剪的图层".to_string());
            return;
        };
        if layer.position == crate::document::Point::ZERO
            && layer.pixels.width == state.document.width
            && layer.pixels.height == state.document.height
        {
            *self.message.borrow_mut() = Some("图层已经在文档范围内".to_string());
            return;
        }
        let id = layer.id;
        let before = layer.pixels.clone();
        let position = layer.position;
        state.execute(Box::new(CropLayerCommand::new(id, before, position)));
        self.texture_dirty = true;
        *self.message.borrow_mut() = Some("已裁到文档大小".to_string());
    }

    /// 路径编辑中的按键 / 文本输入；返回是否消费了事件。
    fn handle_path_input(&mut self, event: &InputEvent) -> bool {
        match event {
            InputEvent::KeyDown { key: Key::Escape } => {
                self.path_edit = None;
                true
            }
            InputEvent::KeyDown { key: Key::Enter } => {
                self.commit_path_edit();
                true
            }
            InputEvent::KeyDown {
                key: Key::Backspace,
            } => {
                if let Some(buffer) = &mut self.path_edit {
                    buffer.pop();
                }
                true
            }
            InputEvent::TextInput { text } => {
                if let Some(buffer) = &mut self.path_edit {
                    buffer.extend(text.chars().filter(|ch| !ch.is_control()));
                }
                true
            }
            // 其余按键也吞掉：编辑路径时按字母不该触发工具 / 画布快捷键。
            InputEvent::KeyDown { .. } => true,
            _ => false,
        }
    }

    fn commit_path_edit(&mut self) {
        if let Some(buffer) = self.path_edit.take() {
            let path = buffer.trim();
            if !path.is_empty() {
                *self.path.borrow_mut() = path.to_string();
            }
        }
    }

    /// 把当前文档合成后写成 PNG。
    pub fn export_png_to(&self, path: &str) -> Result<(), crate::io::IoError> {
        let state = self.state.borrow();
        let mut target = RenderTarget::new(0, 0);
        self.renderer.render(&state.document, &mut target);
        crate::io::write_png(Path::new(path), &target.pixels)
    }

    /// 从 PNG 文件导入一个新图层（放在最上面并选中）。
    pub fn import_png_from(&mut self, path: &str) -> Result<(u32, u32), crate::io::IoError> {
        let pixels = crate::io::read_png(Path::new(path))?;
        let size = (pixels.width, pixels.height);
        let name = crate::io::file_label(Path::new(path));
        let mut state = self.state.borrow_mut();
        let index = state.document.layers.len();
        let before_active = state.document.active_layer;
        let layer = Layer::new(name, pixels);
        state.execute(Box::new(AddLayerCommand::new(layer, index, before_active)));
        drop(state);
        self.texture_dirty = true;
        Ok(size)
    }

    /// 排布整棵树，并把相机同步到文档节点。
    pub fn layout(&mut self, viewport: ViewportSize) {
        self.viewport = viewport;
        self.tree.set_viewport_size(viewport.logical_size());
        self.clamp_palette_width();
        self.clamp_sidebar_width();
        draw_ui::layout(&mut self.tree, viewport);
        // 面板高度按刚拿到的侧栏矩形钳制一次；变了就再排一遍。
        if self.clamp_panel_heights() {
            draw_ui::layout(&mut self.tree, viewport);
        }
        // 第一次布局拿到画布区域的真实矩形，才能做适配。
        if !self.fitted {
            self.fit_to_canvas();
            self.fitted = true;
        }
        // 图层列表的行池按容器高度算，所以要在 layout 之后 sync；池变了再排一次。
        if self.layer_state.sync(&mut self.tree) {
            draw_ui::layout(&mut self.tree, viewport);
        }
        if self.history_state.sync(&mut self.tree) {
            draw_ui::layout(&mut self.tree, viewport);
        }
        // 覆盖层用自己的树，必须在主树排完之后定位。
        self.overlays.layout(&self.tree, viewport);
        self.sync_document_node();
        self.tree.update();
    }

    /// 发出这一帧的绘制命令：先世界（文档图像 + 像素网格），再选区描边，
    /// UI 覆盖在上层。
    pub fn paint(&self, ctx: &mut PaintContext) {
        self.tree.paint(ctx);
        self.paint_pixel_grid(ctx);
        self.paint_selection(ctx);
        draw_ui::paint(&self.tree, ctx);
        self.overlays.paint(ctx);
    }

    /// 像素模式 + 放大到 [`GRID_MIN_ZOOM`] 以上时，在文档上画像素网格。
    ///
    /// 网格画在屏幕空间（相机换算后的坐标），线宽 1 逻辑像素、不随缩放变粗；
    /// 只画「文档矩形 ∩ 画布区域」里的那部分，所以命令数有界。
    fn paint_pixel_grid(&self, ctx: &mut PaintContext) {
        if !self.pixel_mode.get() {
            return;
        }
        let state = self.state.borrow();
        let camera = state.canvas;
        if camera.zoom < GRID_MIN_ZOOM {
            return;
        }
        let Some(area) = self.canvas_area_rect() else {
            return;
        };
        let origin = camera.document_to_screen(Vec2::ZERO);
        let corner = camera.document_to_screen(Vec2::new(
            state.document.width as f32,
            state.document.height as f32,
        ));
        let Some(visible) = Rect::from_min_max(origin, corner).intersection(area) else {
            return;
        };
        let (min, max) = (visible.min(), visible.max());
        let color = self.theme.palette.border.with_alpha(0.3);
        let zoom = camera.zoom;

        let x_first = ((min.x - origin.x) / zoom).ceil() as i64;
        let x_last = ((max.x - origin.x) / zoom).floor() as i64;
        for x in x_first..=x_last {
            let sx = origin.x + x as f32 * zoom;
            ctx.draw_line(Vec2::new(sx, min.y), Vec2::new(sx, max.y), 1.0, color);
        }
        let y_first = ((min.y - origin.y) / zoom).ceil() as i64;
        let y_last = ((max.y - origin.y) / zoom).floor() as i64;
        for y in y_first..=y_last {
            let sy = origin.y + y as f32 * zoom;
            ctx.draw_line(Vec2::new(min.x, sy), Vec2::new(max.x, sy), 1.0, color);
        }
    }

    /// 框选选区的描边：把文档像素矩形换算成屏幕坐标，画一圈 1px 线。
    ///
    /// 画在场景（文档图像）之上、UI 之下，所以它不会盖住面板，也不受 `tree`
    /// 里的相机变换影响（这里用的是相机换算后的屏幕坐标）。
    fn paint_selection(&self, ctx: &mut PaintContext) {
        let state = self.state.borrow();
        let Some(selection) = state.selection else {
            return;
        };
        let camera = state.canvas;
        let min = camera.document_to_screen(Vec2::new(selection.x as f32, selection.y as f32));
        let max = camera.document_to_screen(Vec2::new(
            selection.right() as f32,
            selection.bottom() as f32,
        ));
        let rect = Rect::from_min_max(
            Vec2::new(min.x.min(max.x), min.y.min(max.y)),
            Vec2::new(min.x.max(max.x), min.y.max(max.y)),
        );
        let color = self.theme.palette.selection;
        ctx.stroke_rect(rect, 1.0, Paint::new(color));
    }

    /// 路由一个后端无关的输入事件。
    ///
    /// 工具快捷键、画布缩放 / 平移由视图先处理；其余（点击、指针）交给
    /// `draw_ui::handle_input`，由它去找带回调的控件。
    pub fn event(&mut self, event: &InputEvent) -> EventResult {
        // 覆盖层优先：菜单打开时，Esc / 点击外部由它先处理。
        // 例外：点的是菜单标题时，先关掉当前菜单、把点击留给主树 —— 这样点
        // 另一个标题一次就能切换，点同一个标题则是关闭。
        if let InputEvent::PointerDown { position, .. } = event {
            if self.menu_open() {
                if let Some(index) = self.menu_index_at(*position) {
                    let same = self.open_menu.get() == Some(index);
                    self.overlays.close_all();
                    self.open_menu.set(None);
                    if same {
                        return EventResult::Handled;
                    }
                } else if self.overlays.handle_input(event).is_handled() {
                    return EventResult::Handled;
                }
            } else if self.overlays.handle_input(event).is_handled() {
                return EventResult::Handled;
            }
        } else if self.overlays.handle_input(event).is_handled() {
            return EventResult::Handled;
        }
        // 重命名 / 路径编辑进行中：键盘只编辑文本，不触发工具 / 画布快捷键。
        if self.renaming.is_some() && self.handle_rename_input(event) {
            return EventResult::Handled;
        }
        if self.path_edit.is_some() && self.handle_path_input(event) {
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
                if matches!(key, Key::Escape) && self.clear_selection() {
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
                if self.is_dragging() {
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
                if self.is_dragging() {
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

    /// 分隔条只管 `[min, max]`，管不了"窗口变窄了"。那一半在这里：右栏最多
    /// 占到给画布留 [`CANVAS_MIN`] 为止（`examples/file_browser` 同款）。
    fn clamp_sidebar_width(&mut self) {
        let full = self.viewport.logical_size().width;
        let max =
            (full - TOOLBAR_WIDTH - self.palette_width.get() - 2.0 * RESIZE_GUTTER - CANVAS_MIN)
                .max(SIDEBAR_MIN);
        let current = self.sidebar_width.get();
        let next = current.clamp(SIDEBAR_MIN, max);
        if (next - current).abs() > f32::EPSILON {
            self.sidebar_width.set(next);
            update_control(&mut self.tree, self.sidebar_node, |data| {
                data.layout.basis = SizeBasis::Px(next);
            });
        }
    }

    /// 左侧调色盘面板的宽度不能把画布 / 右栏挤没。
    fn clamp_palette_width(&mut self) {
        let full = self.viewport.logical_size().width;
        let max = (full - TOOLBAR_WIDTH - 2.0 * RESIZE_GUTTER - SIDEBAR_MIN - CANVAS_MIN)
            .min(PALETTE_MAX)
            .max(PALETTE_MIN);
        let current = self.palette_width.get();
        let next = current.clamp(PALETTE_MIN, max);
        if (next - current).abs() > f32::EPSILON {
            self.palette_width.set(next);
            update_control(&mut self.tree, self.palette_panel_node, |data| {
                data.layout.basis = SizeBasis::Px(next);
            });
        }
    }

    /// 保证可拖的标签页面板不会把下面的「图层」面板挤没。
    /// 返回是否调整过（调用方需要再排一次）。
    fn clamp_panel_heights(&mut self) -> bool {
        let Some(sidebar) = draw_ui::control(&self.tree, self.sidebar_node) else {
            return false;
        };
        // 预留：图层最小高度 + 分隔条 + 两个子节点之间的列间距。
        let overhead = LAYER_PANEL_MIN + RESIZE_GUTTER + space::XXXS;
        let max = (sidebar.rect.size.height - overhead).clamp(TABS_PANEL_MIN, TABS_PANEL_MAX);
        let current = self.tabs_height.get();
        let next = current.clamp(TABS_PANEL_MIN, max);
        if (next - current).abs() <= f32::EPSILON {
            return false;
        }
        self.tabs_height.set(next);
        update_control(&mut self.tree, self.tabs_panel_node, |data| {
            data.layout.basis = SizeBasis::Px(next);
        });
        true
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

    /// 是否有工具正处在一次拖拽 / 笔画中。
    fn is_dragging(&self) -> bool {
        self.brush.is_drawing() || self.move_tool.is_moving() || self.select_anchor.is_some()
    }

    /// 左键落下：把屏幕点换算成文档坐标，交给当前工具。
    fn begin_tool(&mut self, screen: Vec2) {
        let tool = self.state.borrow().active_tool;
        let position = self.document_position(screen);
        match tool {
            ActiveTool::Brush | ActiveTool::Eraser => {
                self.brush.mode = if tool == ActiveTool::Eraser {
                    BrushMode::Erase
                } else {
                    BrushMode::Paint
                };
                let (foreground, selection) = {
                    let state = self.state.borrow();
                    (state.foreground, state.selection)
                };
                self.brush.color = foreground;
                self.brush.clip = selection;
                tracing::debug!(target: "image_editor", tool = self.brush.name(), "stroke_started");
                let mut state = self.state.borrow_mut();
                let AppState {
                    document, history, ..
                } = &mut *state;
                let mut ctx = ToolContext { document, history };
                self.brush.on_pointer_down(&mut ctx, left_pointer(position));
                self.texture_dirty = true;
            }
            ActiveTool::Move => {
                let mut state = self.state.borrow_mut();
                let AppState {
                    document, history, ..
                } = &mut *state;
                let mut ctx = ToolContext { document, history };
                self.move_tool
                    .on_pointer_down(&mut ctx, left_pointer(position));
                self.texture_dirty = true;
            }
            ActiveTool::RectangleSelect => {
                self.select_anchor = Some(position);
                self.update_selection(position);
            }
            ActiveTool::Eyedropper => self.pick_color(position),
        }
    }

    fn continue_tool(&mut self, screen: Vec2) {
        let tool = self.state.borrow().active_tool;
        let position = self.document_position(screen);
        match tool {
            ActiveTool::Brush | ActiveTool::Eraser if self.brush.is_drawing() => {
                let mut state = self.state.borrow_mut();
                let AppState {
                    document, history, ..
                } = &mut *state;
                let mut ctx = ToolContext { document, history };
                self.brush.on_pointer_move(&mut ctx, left_pointer(position));
                self.texture_dirty = true;
            }
            ActiveTool::Move if self.move_tool.is_moving() => {
                let mut state = self.state.borrow_mut();
                let AppState {
                    document, history, ..
                } = &mut *state;
                let mut ctx = ToolContext { document, history };
                self.move_tool
                    .on_pointer_move(&mut ctx, left_pointer(position));
                self.texture_dirty = true;
            }
            ActiveTool::RectangleSelect => self.update_selection(position),
            _ => {}
        }
    }

    fn end_tool(&mut self, screen: Vec2) {
        let tool = self.state.borrow().active_tool;
        let position = self.document_position(screen);
        match tool {
            ActiveTool::Brush | ActiveTool::Eraser if self.brush.is_drawing() => {
                let mut state = self.state.borrow_mut();
                let AppState {
                    document, history, ..
                } = &mut *state;
                let mut ctx = ToolContext { document, history };
                self.brush.on_pointer_up(&mut ctx, left_pointer(position));
                self.texture_dirty = true;
            }
            ActiveTool::Move if self.move_tool.is_moving() => {
                let mut state = self.state.borrow_mut();
                let AppState {
                    document, history, ..
                } = &mut *state;
                let mut ctx = ToolContext { document, history };
                self.move_tool
                    .on_pointer_up(&mut ctx, left_pointer(position));
            }
            ActiveTool::RectangleSelect if self.select_anchor.take().is_some() => {
                self.update_selection(position);
                let message = match self.state.borrow().selection {
                    Some(region) => format!(
                        "选区 {} × {} @ ({}, {})",
                        region.width, region.height, region.x, region.y
                    ),
                    None => "选区已清空".to_string(),
                };
                *self.message.borrow_mut() = Some(message);
            }
            _ => {}
        }
    }

    /// 拖拽框选：起点 + 当前点 -> 裁剪到画布的整数选区。
    fn update_selection(&mut self, position: Vec2) {
        let Some(anchor) = self.select_anchor else {
            return;
        };
        let (width, height) = self.document_size();
        let selection = pixel_selection(anchor, position, width, height);
        self.state.borrow_mut().selection = selection;
    }

    /// 吸管：取合成后 `position` 处的颜色作为前景色。
    fn pick_color(&mut self, position: Vec2) {
        let (width, height) = self.document_size();
        let Some((x, y)) = document_to_pixel(position, width, height) else {
            *self.message.borrow_mut() = Some("吸管：点在画布外".to_string());
            return;
        };
        let color = {
            let state = self.state.borrow();
            sample_pixel(&state.document, x, y)
        };
        self.state.borrow_mut().foreground = color;
        tracing::info!(
            target: "image_editor",
            r = color.r,
            g = color.g,
            b = color.b,
            "color_picked"
        );
        *self.message.borrow_mut() = Some(format!(
            "吸管：({x}, {y}) → #{:02X}{:02X}{:02X}",
            color.r, color.g, color.b
        ));
    }

    /// Escape 清空选区；返回是否真的清了。
    fn clear_selection(&mut self) -> bool {
        if self.state.borrow().selection.is_none() {
            return false;
        }
        self.state.borrow_mut().selection = None;
        *self.message.borrow_mut() = Some("已清空选区".to_string());
        true
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

    /// 选项栏的 `−` / `+` 请求 -> 真正的笔刷调整。
    fn apply_brush_adjust(&mut self, adjust: BrushAdjust) {
        match adjust {
            BrushAdjust::SizeDown => self.adjust_brush_size(-1.0),
            BrushAdjust::SizeUp => self.adjust_brush_size(1.0),
            BrushAdjust::OpacityDown => self.adjust_brush_opacity(-0.05),
            BrushAdjust::OpacityUp => self.adjust_brush_opacity(0.05),
        }
    }

    /// 工具选项栏：工具名 / 提示 / 笔刷配置，按当前工具显示。
    fn sync_options(&mut self) {
        let tool = self.state.borrow().active_tool;
        set_text(&mut self.tree, self.options_tool, tool.label());
        set_text(
            &mut self.tree,
            self.options_size,
            format!("{:.0}px", self.brush.size),
        );
        set_text(
            &mut self.tree,
            self.options_opacity,
            format!("{:.0}%", self.brush.opacity * 100.0),
        );
        set_text(&mut self.tree, self.options_hint, options_hint(tool));

        let show_brush = tool_has_brush(tool);
        if self.tree.is_visible(self.options_brush) != Some(show_brush) {
            self.tree.set_visible(self.options_brush, show_brush);
            draw_ui::mark_dirty(&mut self.tree, self.options_brush);
        }
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
        // 显示用：透明区域补上棋盘格（可关）。导出走 `export_png_to`，仍是纯合成。
        if self.show_checkerboard {
            paint_backdrop(&mut self.render_target.pixels);
        }
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

    /// 调色盘色块数量。
    pub fn palette_swatch_count(&self) -> usize {
        self.palette_nodes.len()
    }

    /// 调色盘第 `index` 个色块的中心点；测试与自检模拟点击用。
    pub fn palette_swatch_center(&self, index: usize) -> Option<Vec2> {
        let id = *self.palette_nodes.get(index)?;
        draw_ui::control(&self.tree, id).map(|control| control.rect.center())
    }

    /// 取色器饱和 / 明度方块的中心点；测试与自检模拟拖动用。
    pub fn palette_picker_center(&self) -> Option<Vec2> {
        draw_ui::control(&self.tree, self.palette_picker_node).map(|control| control.rect.center())
    }

    /// 文件面板按钮的节点 id。
    pub fn file_node(&self, action: IoAction) -> Option<NodeId> {
        self.file_nodes
            .iter()
            .find(|(candidate, _)| *candidate == action)
            .map(|(_, id)| *id)
    }

    /// 文件面板按钮的中心点（逻辑坐标）；测试与自检模拟点击用。
    pub fn file_center(&self, action: IoAction) -> Option<Vec2> {
        draw_ui::control(&self.tree, self.file_node(action)?).map(|control| control.rect.center())
    }

    /// 选项栏 `−` / `+` 按钮的节点。
    pub fn brush_adjust_node(&self, adjust: BrushAdjust) -> Option<NodeId> {
        self.brush_buttons
            .iter()
            .find(|(candidate, _)| *candidate == adjust)
            .map(|(_, id)| *id)
    }

    /// 选项栏 `−` / `+` 按钮的中心点（逻辑坐标）；测试与自检模拟点击用。
    pub fn brush_adjust_center(&self, adjust: BrushAdjust) -> Option<Vec2> {
        let id = self.brush_adjust_node(adjust)?;
        draw_ui::control(&self.tree, id).map(|control| control.rect.center())
    }

    /// 当前笔刷大小（逻辑像素）。
    pub fn brush_size(&self) -> f32 {
        self.brush.size
    }

    /// 画笔当前的硬边状态（像素模式同步后的结果）。
    pub fn brush_hard(&self) -> bool {
        self.brush.hard
    }

    /// 画笔当前的形状。
    pub fn brush_shape(&self) -> BrushShape {
        self.brush.shape
    }

    /// 像素模式开关是否打开（硬边）。
    pub fn pixel_mode(&self) -> bool {
        self.pixel_mode.get()
    }

    /// 方形笔开关是否打开。
    pub fn square_mode(&self) -> bool {
        self.square_mode.get()
    }

    /// 选项栏开关按钮的节点。
    pub fn brush_toggle_node(&self, toggle: BrushToggle) -> Option<NodeId> {
        self.brush_toggles
            .iter()
            .find(|(candidate, _)| *candidate == toggle)
            .map(|(_, id)| *id)
    }

    /// 选项栏开关按钮的中心点（逻辑坐标）；测试与自检模拟点击用。
    pub fn brush_toggle_center(&self, toggle: BrushToggle) -> Option<Vec2> {
        let id = self.brush_toggle_node(toggle)?;
        draw_ui::control(&self.tree, id).map(|control| control.rect.center())
    }

    /// 右栏当前宽度（逻辑像素）。
    pub fn sidebar_width(&self) -> f32 {
        self.sidebar_width.get()
    }

    /// 右栏分隔条的中心点（逻辑坐标）；测试与自检模拟拖动用。
    pub fn sidebar_handle_center(&self) -> Option<Vec2> {
        draw_ui::control(&self.tree, self.sidebar_handle_node).map(|control| control.rect.center())
    }

    /// 左侧调色盘面板的宽度 / 分隔条中心。
    pub fn palette_width(&self) -> f32 {
        self.palette_width.get()
    }

    pub fn palette_handle_center(&self) -> Option<Vec2> {
        draw_ui::control(&self.tree, self.palette_handle_node).map(|control| control.rect.center())
    }

    /// 右侧栏标签页面板的当前高度（逻辑像素）。
    pub fn tabs_height(&self) -> f32 {
        self.tabs_height.get()
    }

    /// 标签页面板与「图层」面板之间分隔条的中心点（逻辑坐标）。
    pub fn tabs_handle_center(&self) -> Option<Vec2> {
        draw_ui::control(&self.tree, self.tabs_handle_node).map(|control| control.rect.center())
    }

    /// 当前标签页。
    pub fn active_tab(&self) -> SidebarTab {
        self.active_tab.get()
    }

    /// 某个标签按钮的中心点（逻辑坐标）；测试与自检模拟点击用。
    pub fn tab_center(&self, tab: SidebarTab) -> Option<Vec2> {
        let id = self
            .tab_buttons
            .iter()
            .find(|(candidate, _)| *candidate == tab)
            .map(|(_, id)| *id)?;
        draw_ui::control(&self.tree, id).map(|control| control.rect.center())
    }

    /// 某个标签页内容当前是否可见。
    pub fn tab_content_visible(&self, tab: SidebarTab) -> bool {
        self.tab_contents
            .iter()
            .find(|(candidate, _)| *candidate == tab)
            .and_then(|(_, id)| self.tree.is_visible(*id))
            .unwrap_or(false)
    }

    /// 历史面板的行数（撤销栈 + 「当前」+ 重做栈）。
    pub fn history_rows(&self) -> usize {
        self.history_count.get()
    }

    /// 菜单标题按钮的中心点（逻辑坐标）；测试与自检模拟点击用。
    pub fn menu_center(&self, index: usize) -> Option<Vec2> {
        let id = *self.menu_nodes.get(index)?;
        draw_ui::control(&self.tree, id).map(|control| control.rect.center())
    }

    /// 当前是否有下拉菜单打开。
    pub fn menu_open(&self) -> bool {
        !self.overlays.is_empty()
    }

    /// 当前打开菜单的下标（没有打开则为 `None`）。
    pub fn open_menu_index(&self) -> Option<usize> {
        self.open_menu.get()
    }

    /// 当前文档的图层数（自检核对导入结果用）。
    pub fn layer_count(&self) -> usize {
        self.state.borrow().document.layers.len()
    }

    /// 当前前景色（吸管取色后测试 / 自检用）。
    pub fn foreground(&self) -> crate::document::Color {
        self.state.borrow().foreground
    }

    /// 当前选区（框选后测试 / 自检用）。
    pub fn selection(&self) -> Option<crate::document::PixelRegion> {
        self.state.borrow().selection
    }

    /// 当前图层的像素偏移（移动工具的效果，测试 / 自检用）。
    pub fn active_layer_position(&self) -> Option<crate::document::Point> {
        self.state
            .borrow()
            .document
            .active_layer()
            .map(|layer| layer.position)
    }

    /// 当前的导入 / 导出路径。
    pub fn io_path(&self) -> String {
        self.path.borrow().clone()
    }

    /// 设置导入 / 导出路径（测试与自检把它指到临时文件）。
    pub fn set_io_path(&mut self, path: impl Into<String>) {
        *self.path.borrow_mut() = path.into();
    }

    /// 控件数（自检的预算检查用）。
    pub fn control_count(&self) -> usize {
        draw_ui::control_count(&self.tree)
    }
}

/// 左键指针事件（这些工具只处理左键）。
fn left_pointer(position: Vec2) -> PointerEvent {
    PointerEvent {
        position,
        button: PointerButton::Left,
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
    use crate::document::{Color, PixelBuffer, Point};
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

    fn menu_index(name: &str) -> usize {
        crate::ui::menu::MENUS
            .iter()
            .position(|candidate| *candidate == name)
            .expect("menu exists")
    }

    /// 走 `EditorView::event` 的一次点击（覆盖层参与路由，与真实鼠标一致）。
    fn click_event(view: &mut EditorView, position: Vec2) {
        view.event(&InputEvent::PointerDown {
            position,
            button: PointerButton::Left,
        });
        view.event(&InputEvent::PointerUp {
            position,
            button: PointerButton::Left,
        });
    }

    #[test]
    fn clicking_a_menu_title_opens_its_dropdown() {
        let mut view = EditorView::new(Theme::dark(), AppState::default());
        view.layout(viewport());
        let help = menu_index("帮助");
        let center = view.menu_center(help).expect("help menu button");
        click_event(&mut view, center);
        view.update();
        assert!(view.menu_open(), "点菜单标题应打开下拉");
    }

    #[test]
    fn clicking_another_menu_title_switches_in_one_click() {
        let mut view = EditorView::new(Theme::dark(), AppState::default());
        view.layout(viewport());
        let file = menu_index("文件");
        let edit = menu_index("编辑");
        let center = view.menu_center(file).expect("file menu button");
        click_event(&mut view, center);
        view.update();
        assert_eq!(view.open_menu_index(), Some(file));
        // 一次点击就切到「编辑」，而不是先关再点。
        let center = view.menu_center(edit).expect("edit menu button");
        click_event(&mut view, center);
        view.update();
        assert_eq!(view.open_menu_index(), Some(edit));
        assert!(view.menu_open());
    }

    #[test]
    fn clicking_the_open_menu_title_closes_it() {
        let mut view = EditorView::new(Theme::dark(), AppState::default());
        view.layout(viewport());
        let help = menu_index("帮助");
        let center = view.menu_center(help).expect("help menu button");
        click_event(&mut view, center);
        view.update();
        assert!(view.menu_open());
        click_event(&mut view, center);
        view.update();
        assert!(!view.menu_open(), "点同一标题应关闭菜单");
    }

    #[test]
    fn escape_closes_an_open_menu() {
        let mut view = EditorView::new(Theme::dark(), AppState::default());
        view.layout(viewport());
        let help = menu_index("帮助");
        let center = view.menu_center(help).expect("help menu button");
        click_event(&mut view, center);
        view.update();
        assert!(view.menu_open());
        view.event(&InputEvent::KeyDown { key: Key::Escape });
        assert!(!view.menu_open(), "Esc 应关闭菜单");
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
    fn palette_swatches_wrap_within_the_panel() {
        let mut view = EditorView::new(Theme::dark(), AppState::default());
        view.layout(viewport());
        let c0 = view.palette_swatch_center(0).expect("swatch 0");
        let c1 = view.palette_swatch_center(1).expect("swatch 1");
        let last = view.palette_swatch_center(15).expect("swatch 15");
        assert!((c0.y - c1.y).abs() < 0.5, "同一行的前两个 y 应相同");
        assert!(last.y > c0.y + 1.0, "最后一个色块应换到下面的行");
        // 每行不超过面板宽度。
        assert!(c1.x > c0.x, "同一行从左往右排");
    }

    #[test]
    fn clicking_a_palette_swatch_sets_the_foreground() {
        let mut view = EditorView::new(Theme::dark(), AppState::default());
        view.layout(viewport());
        view.event(&InputEvent::KeyDown {
            key: Key::Character('b'),
        });
        view.update();
        view.layout(viewport());
        assert_eq!(view.foreground(), Color::BLACK, "默认前景是黑");
        assert!(view.palette_swatch_count() >= 16);

        // SWATCHES[4] = (255, 0, 0)。
        let center = view.palette_swatch_center(4).expect("swatch mounted");
        view.event(&InputEvent::PointerDown {
            position: center,
            button: PointerButton::Left,
        });
        view.event(&InputEvent::PointerUp {
            position: center,
            button: PointerButton::Left,
        });
        view.update();
        assert_eq!(view.foreground(), Color::RED);
    }

    #[test]
    fn dragging_the_picker_sets_the_foreground() {
        let mut view = EditorView::new(Theme::dark(), AppState::default());
        view.layout(viewport());
        view.event(&InputEvent::KeyDown {
            key: Key::Character('b'),
        });
        view.update();
        view.layout(viewport());
        assert_eq!(view.foreground(), Color::BLACK, "默认前景是黑");

        let center = view.palette_picker_center().expect("picker mounted");
        view.event(&InputEvent::PointerDown {
            position: center,
            button: PointerButton::Left,
        });
        view.event(&InputEvent::PointerUp {
            position: center,
            button: PointerButton::Left,
        });
        view.update();
        let fg = view.foreground();
        assert_ne!(fg, Color::BLACK, "取色器应改前景色");
        assert!(
            fg.r > fg.g && fg.g == fg.b,
            "中心应是色相 0 / s=v=0.5 的暗红，得到 {fg:?}"
        );
    }

    #[test]
    fn the_history_panel_tracks_the_command_count() {
        let mut view = EditorView::new(Theme::dark(), AppState::default());
        view.layout(viewport());
        view.update();
        assert_eq!(view.history_rows(), 1, "空历史只有「当前」一行");
        paint_a_stroke(&mut view);
        view.update();
        assert_eq!(view.history_rows(), 2, "一笔之后多一行");
    }

    #[test]
    fn the_options_bar_adjusts_the_brush_size() {
        let mut view = EditorView::new(Theme::dark(), AppState::default());
        view.layout(viewport());
        // Select the brush so its config is shown in the options bar.
        view.event(&InputEvent::KeyDown {
            key: Key::Character('b'),
        });
        view.update();
        view.layout(viewport());
        let before = view.brush_size();
        let center = view
            .brush_adjust_center(BrushAdjust::SizeUp)
            .expect("size + button");
        view.event(&InputEvent::PointerDown {
            position: center,
            button: PointerButton::Left,
        });
        view.event(&InputEvent::PointerUp {
            position: center,
            button: PointerButton::Left,
        });
        view.update();
        assert_eq!(view.brush_size(), before + 1.0);
    }

    #[test]
    fn the_pixel_mode_toggles_flip_the_brush() {
        let mut view = EditorView::new(Theme::dark(), AppState::default());
        view.layout(viewport());
        view.event(&InputEvent::KeyDown {
            key: Key::Character('b'),
        });
        view.update();
        view.layout(viewport());

        // 默认：像素模式开、方形开。
        assert!(view.pixel_mode());
        assert!(view.square_mode());
        assert!(view.brush_hard());
        assert_eq!(view.brush_shape(), BrushShape::Square);

        for toggle in [BrushToggle::Hard, BrushToggle::Square] {
            click_toggle(&mut view, toggle);
        }
        assert!(!view.pixel_mode());
        assert!(!view.square_mode());
        // 开关同步进了画笔。
        assert!(!view.brush_hard());
        assert_eq!(view.brush_shape(), BrushShape::Round);

        for toggle in [BrushToggle::Hard, BrushToggle::Square] {
            click_toggle(&mut view, toggle);
        }
        assert!(view.pixel_mode() && view.square_mode());
    }

    fn click_toggle(view: &mut EditorView, toggle: BrushToggle) {
        let center = view.brush_toggle_center(toggle).expect("toggle button");
        view.event(&InputEvent::PointerDown {
            position: center,
            button: PointerButton::Left,
        });
        view.event(&InputEvent::PointerUp {
            position: center,
            button: PointerButton::Left,
        });
        view.update();
    }

    #[test]
    fn dragging_the_sidebar_handle_resizes_the_sidebar() {
        let mut view = EditorView::new(Theme::dark(), AppState::default());
        view.layout(viewport());
        let before = view.sidebar_width();
        let start = view.sidebar_handle_center().expect("sidebar handle");
        // The sidebar is on the right, so dragging the handle left grows it.
        let end = start - Vec2::new(40.0, 0.0);
        view.event(&InputEvent::PointerDown {
            position: start,
            button: PointerButton::Left,
        });
        view.event(&InputEvent::PointerMove { position: end });
        view.event(&InputEvent::PointerUp {
            position: end,
            button: PointerButton::Left,
        });
        view.layout(viewport());
        assert!(
            view.sidebar_width() > before,
            "{} should have grown from {before}",
            view.sidebar_width()
        );
    }

    #[test]
    fn dragging_the_palette_handle_resizes_it() {
        let mut view = EditorView::new(Theme::dark(), AppState::default());
        view.layout(viewport());

        let before = view.palette_width();
        let start = view.palette_handle_center().expect("palette handle");
        let end = start + Vec2::new(24.0, 0.0);
        view.event(&InputEvent::PointerDown {
            position: start,
            button: PointerButton::Left,
        });
        view.event(&InputEvent::PointerMove { position: end });
        view.event(&InputEvent::PointerUp {
            position: end,
            button: PointerButton::Left,
        });
        view.layout(viewport());
        assert!(view.palette_width() > before, "调色盘面板应变宽");
    }

    #[test]
    fn a_layer_panel_edit_is_undoable() {
        let mut view = EditorView::new(Theme::dark(), AppState::default());
        view.layout(viewport());
        // 走和面板按钮一样的路径：AppState::execute + LayerMetaCommand。
        {
            let mut state = view.state.borrow_mut();
            let id = state.document.active_layer().unwrap().id;
            let before = crate::document::LayerMetaCommand::capture(&state.document);
            state.document.set_layer_visible(id, false);
            let after = crate::document::LayerMetaCommand::capture(&state.document);
            state.execute(Box::new(crate::document::LayerMetaCommand::new(
                before,
                after,
                "显示/隐藏",
            )));
        }
        view.update();
        assert!(view.can_undo());
        assert!(view.undo());
        let visible = view.state.borrow().document.layers[0].visible;
        assert!(visible, "撤销后图层恢复可见");
    }

    #[test]
    fn the_active_tool_button_paints_the_selection_color() {
        let mut view = EditorView::new(Theme::dark(), AppState::default());
        view.layout(viewport());
        let selection = view.theme().palette.selection;
        assert!(
            paint_commands(&view).iter().any(|command| matches!(
                command,
                DrawCommand::FillRoundedRect { paint, .. } if paint.color == selection
            )),
            "当前工具的按钮应画出 selection 高亮"
        );
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
    fn transparent_document_pixels_show_the_checkerboard() {
        let mut view = EditorView::new(Theme::dark(), AppState::default());
        view.layout(viewport());
        // 默认文档带白色「背景」图层：棋盘格被盖住。
        view.mark_texture_dirty();
        let covered = view.take_texture_upload().expect("应产出合成结果");
        assert_eq!(covered.get_pixel(0, 0), Color::WHITE);

        // 隐藏背景图层后，透明区域应露出棋盘格。
        let background = view.state.borrow().document.layers[0].id;
        view.state
            .borrow_mut()
            .document
            .set_layer_visible(background, false);
        view.mark_texture_dirty();
        let revealed = view.take_texture_upload().expect("应产出合成结果");
        assert_eq!(revealed.get_pixel(0, 0), crate::canvas::color_at(0, 0));
        assert_eq!(revealed.get_pixel(8, 0), crate::canvas::color_at(8, 0));
    }

    #[test]
    fn the_checkerboard_can_be_toggled_off() {
        let mut view = EditorView::new(Theme::dark(), AppState::default());
        view.layout(viewport());
        // 隐藏背景图层，让透明区域露出来。
        let background = view.state.borrow().document.layers[0].id;
        view.state
            .borrow_mut()
            .document
            .set_layer_visible(background, false);
        view.mark_texture_dirty();
        let shown = view.take_texture_upload().expect("应产出合成结果");
        assert_eq!(shown.get_pixel(0, 0), crate::canvas::color_at(0, 0));

        view.apply_menu_action(super::menu::MenuAction::ToggleCheckerboard);
        let hidden = view.take_texture_upload().expect("应产出合成结果");
        assert_eq!(hidden.get_pixel(0, 0), Color::TRANSPARENT, "关掉后保持透明");
    }

    #[test]
    fn the_pixel_grid_shows_only_when_zoomed_in_and_in_pixel_mode() {
        let mut view = EditorView::new(Theme::dark(), AppState::default());
        view.layout(viewport());
        let line_count = |view: &EditorView| {
            paint_commands(view)
                .iter()
                .filter(|command| matches!(command, DrawCommand::Line { .. }))
                .count()
        };

        // 像素模式关：放大也不画网格。
        view.pixel_mode.set(false);
        view.state.borrow_mut().canvas.zoom = 12.0;
        view.state.borrow_mut().canvas.offset = Vec2::new(120.0, 120.0);
        let without = line_count(&view);

        // 打开像素模式：多出网格线。
        view.pixel_mode.set(true);
        let with = line_count(&view);
        assert!(
            with > without,
            "放大 + 像素模式应画网格（{with} vs {without}）"
        );

        // 缩到阈值以下：回到没有网格。
        view.state.borrow_mut().canvas.zoom = 1.0;
        assert_eq!(line_count(&view), without, "缩小后不应画网格");
    }

    #[test]
    fn the_first_layout_fits_and_centers_the_document() {
        let mut view = EditorView::new(Theme::dark(), AppState::default());
        view.layout(viewport());
        let area = view.canvas_area_rect().expect("canvas area");
        let camera = view.canvas_camera();
        assert!(camera.zoom > 0.0);
        let center = camera.document_to_screen(Vec2::new(64.0, 64.0));
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

    /// 切到某个标签页（等同点标签：写共享状态后 `update` + `layout`）。
    fn select_tab(view: &mut EditorView, tab: SidebarTab) {
        view.active_tab.set(tab);
        view.update();
        view.layout(viewport());
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
        select_tab(&mut view, SidebarTab::Properties);
        assert!(frame_texts(&view).iter().any(|text| text.contains("上层")));
    }

    #[test]
    fn the_properties_panel_shows_layer_geometry() {
        let mut view = EditorView::new(Theme::dark(), AppState::default());
        view.layout(viewport());
        let id = view.state.borrow().document.active_layer().unwrap().id;
        view.state
            .borrow_mut()
            .document
            .set_layer_position(id, Point::new(3, -2));
        select_tab(&mut view, SidebarTab::Properties);
        let texts = frame_texts(&view);
        assert!(
            texts.iter().any(|text| text.contains("偏移 (3, -2)")),
            "texts = {texts:?}"
        );
        assert!(
            texts.iter().any(|text| text.contains("缓冲 128×128")),
            "texts = {texts:?}"
        );
    }

    #[test]
    fn the_crop_action_shrinks_a_moved_layer_back_to_the_document() {
        let mut view = EditorView::new(Theme::dark(), AppState::default());
        view.layout(viewport());
        let id = view.state.borrow().document.active_layer().unwrap().id;
        {
            let mut state = view.state.borrow_mut();
            state.document.set_layer_position(id, Point::new(10, 0));
            state.document.ensure_layer_covers_document(id);
        }
        assert!(view.state.borrow().document.layer(id).unwrap().pixels.width > 128);

        view.apply_menu_action(super::menu::MenuAction::CropLayerToDocument);
        {
            let state = view.state.borrow();
            let layer = state.document.layer(id).unwrap();
            assert_eq!((layer.pixels.width, layer.pixels.height), (128, 128));
            assert_eq!(layer.position, Point::ZERO);
        }
        assert!(view.take_texture_upload().is_some(), "裁剪后要重合成");
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
            .document_to_screen(Vec2::new(64.5, 64.5));
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
            .get_pixel(64, 64);
        assert_eq!(painted, Color::BLACK);
        assert!(view.take_texture_upload().is_some(), "画笔改动要重传纹理");
    }

    #[test]
    fn the_eraser_clears_instead_of_painting() {
        let mut view = EditorView::new(Theme::dark(), AppState::default());
        view.layout(viewport());
        // 走真实路径：点工具栏的橡皮按钮（不是快捷键）。
        let eraser = view.tool_center(ActiveTool::Eraser).expect("eraser button");
        view.event(&InputEvent::PointerDown {
            position: eraser,
            button: PointerButton::Left,
        });
        view.event(&InputEvent::PointerUp {
            position: eraser,
            button: PointerButton::Left,
        });
        view.update();
        assert_eq!(view.active_tool(), ActiveTool::Eraser);

        let point = view
            .canvas_camera()
            .document_to_screen(Vec2::new(64.5, 64.5));
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
            .get_pixel(64, 64)
            .a;
        assert_eq!(alpha, 0, "橡皮把背景擦透明");
    }

    #[test]
    fn erasing_removes_a_brush_stroke() {
        let mut view = EditorView::new(Theme::dark(), AppState::default());
        view.layout(viewport());
        paint_a_stroke(&mut view);
        let point = view
            .canvas_camera()
            .document_to_screen(Vec2::new(64.5, 64.5));
        assert_eq!(
            view.state
                .borrow()
                .document
                .active_layer()
                .unwrap()
                .pixels
                .get_pixel(64, 64),
            Color::BLACK,
            "画笔应在 (64,64) 留下黑色"
        );

        // 点工具栏的橡皮，再在同一位置擦一下。
        let eraser = view.tool_center(ActiveTool::Eraser).expect("eraser button");
        view.event(&InputEvent::PointerDown {
            position: eraser,
            button: PointerButton::Left,
        });
        view.event(&InputEvent::PointerUp {
            position: eraser,
            button: PointerButton::Left,
        });
        view.update();
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
            .get_pixel(64, 64)
            .a;
        assert_eq!(alpha, 0, "橡皮应把这一笔擦掉（alpha=0）");
    }

    /// 用画笔在画布中央画一笔（屏幕坐标 -> 文档坐标由视图换算）。
    fn paint_a_stroke(view: &mut EditorView) {
        view.event(&InputEvent::KeyDown {
            key: Key::Character('b'),
        });
        view.update();
        let point = view
            .canvas_camera()
            .document_to_screen(Vec2::new(64.5, 64.5));
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
        assert_eq!(painted.get_pixel(64, 64), Color::BLACK);

        view.undo();
        let undone = view.take_texture_upload().expect("撤销后应重合成");
        assert_eq!(undone.get_pixel(64, 64), Color::WHITE);

        view.redo();
        let redone = view.take_texture_upload().expect("重做后应重合成");
        assert_eq!(redone.get_pixel(64, 64), Color::BLACK);
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
    fn the_icon_pack_covers_the_toolbar_icons() {
        let view = EditorView::new(Theme::dark(), AppState::default());
        let needed = ActiveTool::ALL.len() + HistoryAction::ALL.len();
        assert!(view.icon_count() >= needed, "图标包应覆盖工具栏图标");
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

    // -- Phase 7：导入导出 --------------------------------------------------

    /// 测试用唯一临时 PNG 路径。
    fn temp_png(tag: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU32, Ordering};
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "image_editor_ui_{}_{n}_{tag}.png",
            std::process::id()
        ))
    }

    #[test]
    fn exporting_writes_the_composite_to_the_path() {
        let mut view = EditorView::new(Theme::dark(), AppState::default());
        view.layout(viewport());
        paint_a_stroke(&mut view);

        let path = temp_png("export");
        view.set_io_path(path.to_string_lossy().into_owned());
        let export = view.file_center(IoAction::Export).expect("导出按钮");
        click(&mut view, export);
        view.update();

        let exported = crate::io::read_png(&path).expect("导出的 PNG 应能解码");
        assert_eq!(exported.get_pixel(0, 0), Color::WHITE);
        assert_eq!(exported.get_pixel(64, 64), Color::BLACK);
        assert!(label_text(&view, view.message_label).contains("已导出"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn importing_a_png_adds_a_layer_and_shows_a_message() {
        let mut view = EditorView::new(Theme::dark(), AppState::default());
        view.layout(viewport());
        let path = temp_png("import");
        crate::io::write_png(&path, &PixelBuffer::filled(2, 2, Color::RED)).unwrap();

        let before = view.state.borrow().document.layers.len();
        view.set_io_path(path.to_string_lossy().into_owned());
        let import = view.file_center(IoAction::Import).expect("导入按钮");
        click(&mut view, import);
        view.update();

        assert_eq!(view.state.borrow().document.layers.len(), before + 1);
        assert!(label_text(&view, view.message_label).contains("已导入"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_failed_export_reports_the_error_and_keeps_running() {
        let mut view = EditorView::new(Theme::dark(), AppState::default());
        view.layout(viewport());
        let missing = std::env::temp_dir()
            .join("image_editor_definitely_missing_dir")
            .join("x.png");
        view.set_io_path(missing.to_string_lossy().into_owned());
        let export = view.file_center(IoAction::Export).expect("导出按钮");
        click(&mut view, export);
        view.update();
        assert!(label_text(&view, view.message_label).contains("导出失败"));
    }

    #[test]
    fn the_path_can_be_edited_inline() {
        let mut view = EditorView::new(Theme::dark(), AppState::default());
        view.layout(viewport());
        let original = view.io_path();
        let edit = view.file_center(IoAction::EditPath).expect("改路径按钮");

        click(&mut view, edit);
        view.update();
        assert!(view.path_edit.is_some(), "应进入路径编辑");
        view.event(&InputEvent::TextInput { text: "X".into() });
        view.event(&InputEvent::KeyDown { key: Key::Enter });
        assert_eq!(view.io_path(), format!("{original}X"));

        click(&mut view, edit);
        view.update();
        view.event(&InputEvent::TextInput { text: "Y".into() });
        view.event(&InputEvent::KeyDown { key: Key::Escape });
        assert_eq!(view.io_path(), format!("{original}X"), "Esc 丢弃编辑");
    }

    // -- Phase 8：移动 / 框选 / 吸管 ---------------------------------------

    #[test]
    fn the_move_tool_offsets_the_active_layer() {
        let mut view = EditorView::new(Theme::dark(), AppState::default());
        view.layout(viewport());
        view.event(&InputEvent::KeyDown {
            key: Key::Character('v'),
        });
        view.update();

        let camera = view.canvas_camera();
        let start = camera.document_to_screen(Vec2::new(64.5, 64.5));
        let end = start + Vec2::new(12.0, -8.0);
        let a = camera.screen_to_document(start);
        let b = camera.screen_to_document(end);
        let expected = Point::new((b.x - a.x).round() as i32, (b.y - a.y).round() as i32);

        view.event(&InputEvent::PointerDown {
            position: start,
            button: PointerButton::Left,
        });
        view.event(&InputEvent::PointerMove { position: end });
        view.event(&InputEvent::PointerUp {
            position: end,
            button: PointerButton::Left,
        });

        let position = view
            .state
            .borrow()
            .document
            .active_layer()
            .unwrap()
            .position;
        assert_eq!(position, expected);
        assert!(view.take_texture_upload().is_some(), "移动后要重合成");
    }

    #[test]
    fn painting_after_moving_the_layer_lands_under_the_cursor() {
        let mut view = EditorView::new(Theme::dark(), AppState::default());
        view.layout(viewport());
        // 移动工具把当前图层右移 10 个文档像素。
        view.event(&InputEvent::KeyDown {
            key: Key::Character('v'),
        });
        view.update();
        let camera = view.canvas_camera();
        let start = camera.document_to_screen(Vec2::new(64.5, 64.5));
        let end = start + Vec2::new(10.0 * camera.zoom, 0.0);
        view.event(&InputEvent::PointerDown {
            position: start,
            button: PointerButton::Left,
        });
        view.event(&InputEvent::PointerMove { position: end });
        view.event(&InputEvent::PointerUp {
            position: end,
            button: PointerButton::Left,
        });
        view.update();
        assert_eq!(view.active_layer_position(), Some(Point::new(10, 0)));

        // 切画笔，在文档 (64, 64) 落笔：应落在光标下，而不是被位移顶到 (74, 64)。
        view.event(&InputEvent::KeyDown {
            key: Key::Character('b'),
        });
        view.update();
        let point = view
            .canvas_camera()
            .document_to_screen(Vec2::new(64.5, 64.5));
        view.event(&InputEvent::PointerDown {
            position: point,
            button: PointerButton::Left,
        });
        view.event(&InputEvent::PointerUp {
            position: point,
            button: PointerButton::Left,
        });
        view.update();

        let state = view.state.borrow();
        let layer = state.document.active_layer().unwrap();
        assert_eq!(layer.position, Point::ZERO, "落笔前把位移烘进像素");
        assert_eq!(layer.pixels.get_pixel(64, 64), Color::BLACK);
        // 合成后确实在光标下的文档坐标 (64, 64)，而不是被位移顶到 (74, 64)。
        assert_eq!(sample_pixel(&state.document, 64, 64), Color::BLACK);
        assert_eq!(sample_pixel(&state.document, 74, 64), Color::WHITE);
    }

    #[test]
    fn the_eyedropper_picks_the_composited_color_as_the_foreground() {
        let mut view = EditorView::new(Theme::dark(), AppState::default());
        view.layout(viewport());
        paint_a_stroke(&mut view);

        view.event(&InputEvent::KeyDown {
            key: Key::Character('i'),
        });
        view.update();
        let point = view
            .canvas_camera()
            .document_to_screen(Vec2::new(64.5, 64.5));
        view.event(&InputEvent::PointerDown {
            position: point,
            button: PointerButton::Left,
        });
        view.event(&InputEvent::PointerUp {
            position: point,
            button: PointerButton::Left,
        });

        assert_eq!(view.foreground(), Color::BLACK);
        view.update();
        assert!(label_text(&view, view.message_label).contains("吸管"));
    }

    #[test]
    fn a_selection_confines_the_brush_and_escape_clears_it() {
        let mut view = EditorView::new(Theme::dark(), AppState::default());
        view.layout(viewport());
        let camera = view.canvas_camera();

        // 框一块 54..74 × 54..74。
        view.event(&InputEvent::KeyDown {
            key: Key::Character('m'),
        });
        view.update();
        let start = camera.document_to_screen(Vec2::new(54.0, 54.0));
        let end = camera.document_to_screen(Vec2::new(74.0, 74.0));
        view.event(&InputEvent::PointerDown {
            position: start,
            button: PointerButton::Left,
        });
        view.event(&InputEvent::PointerMove { position: end });
        view.event(&InputEvent::PointerUp {
            position: end,
            button: PointerButton::Left,
        });
        let selection = view.selection().expect("应产生选区");
        assert!(selection.contains(64, 64));
        assert!(!selection.contains(100, 100));

        // 画笔只在选区里落笔。
        view.event(&InputEvent::KeyDown {
            key: Key::Character('b'),
        });
        view.update();
        let inside = camera.document_to_screen(Vec2::new(64.5, 64.5));
        let outside = camera.document_to_screen(Vec2::new(100.0, 100.0));
        for point in [inside, outside] {
            view.event(&InputEvent::PointerDown {
                position: point,
                button: PointerButton::Left,
            });
            view.event(&InputEvent::PointerUp {
                position: point,
                button: PointerButton::Left,
            });
        }
        let (inside_pixel, outside_pixel) = {
            let state = view.state.borrow();
            let layer = state.document.active_layer().unwrap();
            (
                layer.pixels.get_pixel(64, 64),
                layer.pixels.get_pixel(100, 100),
            )
        };
        assert_eq!(inside_pixel, Color::BLACK, "选区内落笔");
        assert_eq!(outside_pixel, Color::WHITE, "选区外不落笔");

        // Esc 清空选区。
        assert!(view
            .event(&InputEvent::KeyDown { key: Key::Escape })
            .is_handled());
        assert!(view.selection().is_none());
    }
}
