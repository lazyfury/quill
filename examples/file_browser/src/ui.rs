//! 文件浏览器视图：左边一个虚拟列表，右边一个可拖拽的预览栏。
//!
//! 全部后端无关 —— 视图是一棵 `SceneTree`，由 `draw_components` 搭起来、
//! `draw_ui` 排布。宿主（[`crate::host`]）拥有窗口、wgpu 后端和读盘的工作
//! 线程，跟 `demo_app` / `BalanceApp` 的分工一样。
//!
//! ```text
//! Input -> Browser::event -> Browser::layout -> Browser::paint
//! ```
//!
//! ## 右栏的两种看法
//!
//! 预览栏有 [`PreviewMode`] 两种模式（hexdump / 文本），**字节只读一次**，
//! 切换只是换"行怎么算"。实现上是两个 `List`（列数和列宽都不一样，而
//! `List` 的列是构建时定死的），靠 `SceneTree::set_visible` 二选一 —— 藏起来
//! 的那个连行池都不会挂（`ListState::sync` 看到零高度的容器直接返回），所以
//! 待命的那个几乎不花钱。
//!
//! ## 两栏怎么分
//!
//! 照 `demo_app` 的形状：一行 flex 里 **主栏 | 分隔条 | 预览栏**，分隔条的
//! `target` 是主栏（`draw_components::ResizeHandle`），拖动它改的是主栏的 flex
//! basis，预览栏 `grow(1.0)` 吃掉剩下的 —— 所以"拖中间那条"就是在调右栏宽度，
//! 不需要第二个手柄。
//!
//! 分隔条自己只管 `[min, max]`；**窗口变窄时要有人把主栏收回来**，否则右栏会
//! 被挤成 0 宽。这件事在 [`Browser::layout`] 里做（[`Browser::clamp_main_width`]）。
//!
//! ## 读目录 / 读文件都不阻塞
//!
//! 视图从不自己碰磁盘：点开一个目录只是把目标路径记进 [`Browser::take_navigation`]
//! 的"待办"，选中一个文件只是记进 [`Browser::take_preview_request`]。宿主拿到
//! 它们、在工作线程上跑 [`scan::scan`] / [`preview::Preview::read`]，再把结果送
//! 回来。所以扫 `/usr` 那种上万条目的目录、或者选中一个 4 GB 的镜像时，界面
//! 照常滚动 —— 这也是这份 demo 存在的理由。
//!
//! ## 每帧三步
//!
//! 两个列表的行池大小都取决于容器的**解析高度**，所以顺序不能反：
//!
//! ```text
//! layout   ->   ListState::sync（两个）   ->   改过树就再 layout 一次
//! ```
//!
//! 见 [`Browser::layout`]。

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use draw_components::{
    set_text, update_control, Component, Divider, Flex, List, ListColumn, ListState, NodeRef,
    ResizeHandle, Text,
};
use draw_core::{Edges, EventResult, InputEvent, Key, NodeId, Vec2, ViewportSize};
use draw_render::PaintContext;
use draw_scene::{SceneChild, SceneTree};
use draw_theme::{default_theme, space, Mode, SurfaceLevel, Theme, Tone};
use draw_ui::{MouseFilter, SizeBasis, SurfaceStyle, TextMeasurer};

use crate::preview::{Preview, PreviewMode};
use crate::scan::{self, Entry, Listing};

/// 目录列表一行的高度（逻辑像素）。列表的池大小 = `ceil(容器高 / 行高) + 1`。
pub const ROW_HEIGHT: f32 = 26.0;

/// hex dump 一行的高度：比目录行矮，一屏能多看好几行。
///
/// 两种模式共用：列不一样，行高一样（列表的行高也是构建时定死的）。
pub const PREVIEW_ROW_HEIGHT: f32 = 20.0;

/// 三列的宽度：名字吃掉剩下的空间，尺寸和日期是定宽的（右对齐更好扫）。
const SIZE_COLUMN: f32 = 84.0;
const TIME_COLUMN: f32 = 132.0;

/// hex 行的三列：偏移量、十六进制、ascii。
const OFFSET_COLUMN: f32 = 68.0;
const HEX_COLUMN: f32 = 300.0;

/// 文本行的两列：行号、内容。
const LINE_NUMBER_COLUMN: f32 = 52.0;

/// 分隔条的把手宽度（`ResizeHandle` 的默认值，画出来的线仍是 1px）。
pub const RESIZE_GUTTER: f32 = 6.0;

/// 主栏（目录列表）的初始宽度、最小宽度；预览栏的最小宽度。
pub const MAIN_WIDTH: f32 = 560.0;
pub const MAIN_MIN: f32 = 320.0;
pub const PREVIEW_MIN: f32 = 260.0;

/// 状态行的操作提示。
const HINTS: &str =
    "↑↓ 选择 · Enter 打开 · Backspace 上级 · R 重扫 · H 隐藏文件 · T 切文本/二进制 · 拖动分隔条调右栏";

/// 节点槽位，声明式构建时填、构建后读。
#[derive(Default)]
struct Refs {
    path: NodeRef,
    status: NodeRef,
    hints: NodeRef,
    /// 主栏节点 —— 分隔条要改它的 basis，所以得有个句柄。
    main: NodeRef,
    preview_title: NodeRef,
    preview_detail: NodeRef,
    /// 两个模式切换按钮 —— 自检要能真的点一下，所以留个句柄。
    text_tab: NodeRef,
    binary_tab: NodeRef,
}

/// 状态行正在说什么。
#[derive(Clone, Debug, PartialEq, Eq)]
enum Status {
    /// 目录读完了，`n` 是条目数。
    Ready(String),
    /// 工作线程正在读。
    Loading,
    /// 读失败的原因。
    Failed(String),
}

/// 文件浏览器视图。
pub struct Browser {
    tree: SceneTree,
    theme: &'static dyn Theme,
    /// 当前清单的行。列表的 `source` 闭包按需读它，所以数据不必变成控件。
    entries: Rc<RefCell<Vec<Entry>>>,
    /// 行数。列表每次 sync 都读它 —— 换目录只要 `set` 一下。
    count: Rc<Cell<usize>>,
    /// 选中行（**数据下标**，不跟行池槽位）。
    selected: Rc<Cell<Option<usize>>>,
    /// 点击请求打开的行：回调拿不到 `&mut self`，所以用一格共享状态转交。
    activated: Rc<Cell<Option<usize>>>,
    state: ListState,
    /// 主栏节点 + 它的宽度。宽度是共享的：分隔条写、`clamp_main_width` 也写。
    main_pane: NodeId,
    main_width: Rc<Cell<f32>>,
    /// 右栏正在显示的预览。两个列表的 `source` 按行读它。
    preview: Rc<RefCell<Preview>>,
    preview_count: Rc<Cell<usize>>,
    /// 右栏的两种看法：hexdump 和文本。字节只读一次，切换只换行的算法。
    preview_mode: PreviewMode,
    /// hexdump 列表（二进制模式）。
    hex_state: ListState,
    hex_container: NodeId,
    /// 文本列表（文本模式）。两个列表同时在树上，藏起来的那个不占行池。
    text_state: ListState,
    text_container: NodeId,
    /// 点切换按钮转交过来的模式（回调拿不到 `&mut self`）。
    mode_request: Rc<Cell<Option<PreviewMode>>>,
    text_tab: NodeId,
    binary_tab: NodeId,
    /// "我想看这个文件"：宿主取走。
    preview_pending: Option<PathBuf>,
    /// 覆盖右栏副标题的一句话（"读取中…"）。真正的预览回来时清空。
    preview_note: Option<String>,
    path_label: NodeId,
    status_label: NodeId,
    hints_label: NodeId,
    preview_title_label: NodeId,
    preview_detail_label: NodeId,
    path: PathBuf,
    /// 已经请求、还没回来的目录。宿主取走它。
    pending: Option<PathBuf>,
    /// 是否显示点开头的文件。
    hidden: bool,
    status: Status,
    viewport: ViewportSize,
}

impl Browser {
    /// 搭出整棵树，起始目录是 `path`。
    ///
    /// 目录的内容和选中文件的字节都由宿主稍后送进来；这里只把结构建好，所以
    /// 构造不碰磁盘。
    pub fn new(theme: &'static dyn Theme, path: PathBuf, hidden: bool) -> Self {
        let refs = Refs::default();
        let path_clone = path.clone();
        let entries: Rc<RefCell<Vec<Entry>>> = Rc::new(RefCell::new(Vec::new()));
        let count = Rc::new(Cell::new(0));
        let selected = Rc::new(Cell::new(None));
        let activated = Rc::new(Cell::new(None));
        let main_width = Rc::new(Cell::new(MAIN_WIDTH));

        // 行的数据源：只给**即将显示**的那几行调用，所以 10 万条目的目录
        // 不会变成 10 万个控件。
        let source_entries = entries.clone();
        let source = move |index: usize| {
            let entries = source_entries.borrow();
            match entries.get(index) {
                Some(entry) => vec![
                    scan::display_name(entry),
                    scan::format_size(entry),
                    scan::format_time(entry),
                ],
                None => Vec::new(),
            }
        };

        let clicked = activated.clone();
        let list = List::new(theme, ROW_HEIGHT, source)
            .columns(vec![
                ListColumn::flexible(),
                ListColumn::fixed(SIZE_COLUMN).tone(Tone::Muted),
                ListColumn::fixed(TIME_COLUMN).tone(Tone::Muted),
            ])
            .count(count.clone())
            .selected(selected.clone())
            .on_activate(move |index| clicked.set(Some(index)))
            .grow(1.0);
        // 组件会被 `child` 消费掉，所以先把手柄取出来。
        let state = list.state();

        // -- 右栏：标题 + 模式切换 + hex dump / 文本 --
        let preview_bytes: Rc<RefCell<Preview>> = Rc::new(RefCell::new(Preview::empty()));
        let preview_count = Rc::new(Cell::new(0));
        let mode_request: Rc<Cell<Option<PreviewMode>>> = Rc::new(Cell::new(None));

        let hex_entries = preview_bytes.clone();
        let hex_source = move |index: usize| {
            let preview = hex_entries.borrow();
            preview.row_cells_in(PreviewMode::Binary, index)
        };
        let hex_list = List::new(theme, PREVIEW_ROW_HEIGHT, hex_source)
            .columns(vec![
                ListColumn::fixed(OFFSET_COLUMN).tone(Tone::Muted),
                ListColumn::fixed(HEX_COLUMN),
                ListColumn::flexible().tone(Tone::Muted),
            ])
            .count(preview_count.clone())
            .grow(1.0);
        let hex_state = hex_list.state();

        let text_entries = preview_bytes.clone();
        let text_source = move |index: usize| {
            let preview = text_entries.borrow();
            preview.row_cells_in(PreviewMode::Text, index)
        };
        let text_list = List::new(theme, PREVIEW_ROW_HEIGHT, text_source)
            .columns(vec![
                ListColumn::fixed(LINE_NUMBER_COLUMN).tone(Tone::Muted),
                ListColumn::flexible(),
            ])
            .count(preview_count.clone())
            .grow(1.0);
        let text_state = text_list.state();

        // 两个切换按钮：点一下就换模式，当前那个有底色。
        let mode_flag: Rc<Cell<PreviewMode>> = Rc::new(Cell::new(PreviewMode::Binary));
        let tabs = Flex::row()
            .gap(space::XS)
            .mouse_filter(MouseFilter::Ignore)
            .child(
                mode_tab(theme, PreviewMode::Text, &mode_flag, &mode_request).ref_(&refs.text_tab),
            )
            .child(
                mode_tab(theme, PreviewMode::Binary, &mode_flag, &mode_request)
                    .ref_(&refs.binary_tab),
            );

        // 布局根的子节点按 anchors 摆，flex 从下一层才开始 —— 所以排页面的
        // column 是根的唯一子节点（`demo_app` 也是这个形状）。
        let main_pane = Flex::column()
            .mouse_filter(MouseFilter::Ignore)
            .gap(space::MD)
            .padding(Edges::all(space::LG))
            .basis(SizeBasis::Px(MAIN_WIDTH))
            .ref_(&refs.main)
            .child(
                Text::subheading("", theme)
                    .max_lines(1)
                    .ellipsis(true)
                    .ref_(&refs.path),
            )
            .child(Divider::horizontal(theme))
            .child(list)
            .child(Divider::horizontal(theme))
            .child(
                Flex::column()
                    .mouse_filter(MouseFilter::Ignore)
                    .gap(space::XS)
                    .child(Text::small("", theme).ref_(&refs.status))
                    .child(
                        Text::caption(HINTS, theme)
                            .tone(Tone::Muted)
                            .ref_(&refs.hints),
                    ),
            );

        let preview_pane = Flex::column()
            .mouse_filter(MouseFilter::Ignore)
            .gap(space::XS)
            .padding(Edges::all(space::LG))
            .child(
                Text::subheading("预览", theme)
                    .max_lines(1)
                    .ellipsis(true)
                    .ref_(&refs.preview_title),
            )
            .child(tabs)
            .child(
                Text::caption("", theme)
                    .tone(Tone::Muted)
                    .ref_(&refs.preview_detail),
            )
            .child(Divider::horizontal(theme))
            .child(hex_list)
            .child(text_list);

        let tree = Flex::column()
            .mouse_filter(MouseFilter::Ignore)
            .child(
                Flex::row()
                    .gap(0.0)
                    .padding(Edges::ZERO)
                    .mouse_filter(MouseFilter::Ignore)
                    .child(main_pane)
                    .child(
                        ResizeHandle::vertical(theme)
                            .target(refs.main.clone())
                            .width(main_width.clone())
                            .min(MAIN_MIN),
                    )
                    .child(preview_pane.grow(1.0)),
            )
            .into_tree();

        let hex_container = hex_state.container().expect("hex list mounted");
        let text_container = text_state.container().expect("text list mounted");

        let mut app = Self {
            tree,
            theme,
            entries,
            count,
            selected,
            activated,
            state,
            main_pane: refs.main.get().expect("main pane mounted"),
            main_width,
            preview: preview_bytes,
            preview_count,
            preview_mode: PreviewMode::Binary,
            hex_state,
            hex_container,
            text_state,
            text_container,
            mode_request,
            text_tab: refs.text_tab.get().expect("text tab mounted"),
            binary_tab: refs.binary_tab.get().expect("binary tab mounted"),
            preview_pending: None,
            preview_note: None,
            path_label: refs.path.get().expect("path label mounted"),
            status_label: refs.status.get().expect("status label mounted"),
            hints_label: refs.hints.get().expect("hints label mounted"),
            preview_title_label: refs.preview_title.get().expect("preview title mounted"),
            preview_detail_label: refs.preview_detail.get().expect("preview detail mounted"),
            path,
            // 起始目录立刻要读：宿主的第一帧就会把这个请求取走。
            pending: Some(path_clone),
            hidden,
            status: Status::Loading,
            viewport: ViewportSize::new(draw_core::Size::new(1100.0, 680.0)),
        };
        // 两个列表都在树上，但只有一个在用：藏起来的那个容器高度是 0，
        // `ListState::sync` 直接返回，连行池都不挂。
        app.set_mode(PreviewMode::Binary);
        app.sync_labels();
        app.sync_preview_labels();
        app
    }

    // -- 生命周期 --------------------------------------------------------

    /// 用后端真实字体的度量，让排版量到的宽度跟画出来的宽度一致。
    pub fn set_text_measurer(&mut self, measurer: Rc<dyn TextMeasurer>) {
        draw_ui::set_text_measurer(&mut self.tree, measurer);
    }

    /// 处理点击转交过来的两件事："打开某一行"和"换个预览模式"。
    ///
    /// 两个回调都拿不到 `&mut self`，所以它们只往共享格子里写，这里取走。
    pub fn update(&mut self) -> bool {
        if let Some(mode) = self.mode_request.take() {
            self.set_mode(mode);
        }
        let Some(index) = self.activated.take() else {
            return false;
        };
        self.selected.set(Some(index));
        self.open(index)
    }

    /// 排布：先量容器，再同步三个行池（目录 + 两种预览），池变了就再排一次。
    pub fn layout(&mut self, viewport: ViewportSize) {
        self.viewport = viewport;
        // 窗口变窄时先把主栏收回来，否则预览栏会被挤成 0 宽。
        self.clamp_main_width();
        draw_ui::layout(&mut self.tree, viewport);
        let mut changed = self.state.sync(&mut self.tree);
        if self.hex_state.sync(&mut self.tree) {
            changed = true;
        }
        if self.text_state.sync(&mut self.tree) {
            changed = true;
        }
        if changed {
            draw_ui::layout(&mut self.tree, viewport);
        }
    }

    /// 发出这一帧的绘制命令。
    pub fn paint(&self, ctx: &mut PaintContext) {
        draw_ui::paint(&self.tree, ctx);
    }

    /// 路由一个后端无关的输入事件。
    ///
    /// 键盘由视图自己处理（列表没有焦点概念），指针和滚轮交给
    /// `draw_ui::handle_input` —— 滚轮会沿着祖先链找到列表的滚动回调，拖动
    /// 会交给分隔条的 `on_drag`。
    pub fn event(&mut self, event: &InputEvent) -> EventResult {
        if let InputEvent::KeyDown { key } = event {
            if self.key(*key) {
                return EventResult::Handled;
            }
        }
        draw_ui::handle_input(&mut self.tree, event)
    }

    // -- 数据进出 --------------------------------------------------------

    /// 取走"请读这个目录"的请求。宿主每帧问一次，然后在工作线程上做。
    pub fn take_navigation(&mut self) -> Option<PathBuf> {
        self.pending.take()
    }

    /// 取走"请预览这个文件"的请求。
    pub fn take_preview_request(&mut self) -> Option<PathBuf> {
        self.preview_pending.take()
    }

    /// 当前目录。
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// 是否显示点开头的文件。
    pub fn hidden(&self) -> bool {
        self.hidden
    }

    /// 收下一份清单（宿主从工作线程送回来的）。
    pub fn apply_listing(&mut self, listing: Listing) {
        let count = listing.len();
        // 回来的正是刚才请求的那个目录，请求才算完成 —— 期间用户可能又点了
        // 别的目录，那个请求不能跟着一起被清掉。
        if self.pending.as_deref() == Some(listing.path.as_path()) {
            self.pending = None;
        }
        self.path = listing.path;
        self.status = match listing.error.clone() {
            Some(error) => Status::Failed(error),
            None if count == 0 => Status::Ready("空目录".to_string()),
            None => Status::Ready(format!("{count} 项")),
        };
        *self.entries.borrow_mut() = listing.entries;
        self.count.set(count);
        // 行数可能没变（重扫同一个目录）但内容变了，所以必须显式作废行池。
        self.state.invalidate();
        self.selected.set(if count > 0 { Some(0) } else { None });
        self.state.scroll_to(0);
        // 新目录的第一行是新的选中项 —— 目录就清掉预览，文件就请求预览。
        self.request_preview();
        self.sync_labels();
    }

    /// 收下一份预览（宿主从工作线程送回来的）。
    ///
    /// 字节跟模式无关，所以这里不重置模式 —— 正在看文本的人不会因为切了个
    /// 文件就被扔回 hexdump。
    pub fn apply_preview(&mut self, preview: Preview) {
        let rows = preview.rows_in(self.preview_mode);
        *self.preview.borrow_mut() = preview;
        self.preview_count.set(rows);
        // 行数可能没变但字节变了（重新选中同一个文件的不同版本），所以显式作废。
        self.hex_state.invalidate();
        self.text_state.invalidate();
        self.hex_state.scroll_to(0);
        self.text_state.scroll_to(0);
        self.preview_note = None;
        self.sync_preview_labels();
    }

    /// 重扫当前目录。
    pub fn reload(&mut self) {
        self.request(self.path.clone());
    }

    /// 切到上一级目录。
    pub fn go_up(&mut self) {
        let Some(parent) = scan::parent(&self.path) else {
            self.status = Status::Ready("已经是根目录".to_string());
            self.sync_labels();
            return;
        };
        self.request(parent);
    }

    /// 打开第 `index` 行：目录就进去，文件只是选中（顺便换掉右栏的预览）。
    pub fn open(&mut self, index: usize) -> bool {
        let entry = match self.entries.borrow().get(index).cloned() {
            Some(entry) => entry,
            None => return false,
        };
        if entry.is_dir {
            self.request(self.path.join(&entry.name));
            true
        } else {
            self.status = Status::Ready(format!(
                "{} · {} · 文件",
                entry.name,
                scan::format_size(&entry)
            ));
            self.select(index);
            self.sync_labels();
            false
        }
    }

    fn request(&mut self, path: PathBuf) {
        self.status = Status::Loading;
        self.pending = Some(path);
        self.sync_labels();
    }

    /// 选中项变了：文件就请宿主去读，目录就清掉右栏。
    ///
    /// 只记"想要哪个"，读盘由宿主的工作线程做 —— 按住方向键扫过一百个文件时
    /// 每次都覆盖 `preview_pending`，最终只有一个请求真的发出去。
    fn request_preview(&mut self) {
        let entry = self
            .selected
            .get()
            .and_then(|index| self.entries.borrow().get(index).cloned());
        match entry {
            Some(entry) if !entry.is_dir => {
                self.preview_pending = Some(self.path.join(&entry.name));
                self.preview_note = Some("读取中…".to_string());
                *self.preview.borrow_mut() = Preview::empty();
                self.preview_count.set(0);
                self.hex_state.invalidate();
                self.text_state.invalidate();
            }
            _ => {
                self.preview_pending = None;
                self.preview_note = None;
                *self.preview.borrow_mut() = Preview::empty();
                self.preview_count.set(0);
                self.hex_state.invalidate();
                self.text_state.invalidate();
            }
        }
        self.sync_preview_labels();
    }

    // -- 键盘 ------------------------------------------------------------

    /// 处理一个按键，返回是否消费掉了。
    fn key(&mut self, key: Key) -> bool {
        let count = self.count.get();
        match key {
            Key::ArrowDown => self.move_selection(1),
            Key::ArrowUp => self.move_selection(-1),
            Key::Home => self.select(0),
            Key::End => self.select(count.saturating_sub(1)),
            Key::Enter => {
                self.activate_selected();
            }
            Key::Backspace => self.go_up(),
            Key::Character('r') | Key::F5 => self.reload(),
            Key::Character('h') => self.toggle_hidden(),
            Key::Character('t') => self.toggle_mode(),
            _ => return false,
        }
        true
    }

    fn move_selection(&mut self, delta: isize) {
        let count = self.count.get();
        if count == 0 {
            return;
        }
        let current = self.selected.get().unwrap_or(0) as isize;
        let next = (current + delta).clamp(0, count as isize - 1) as usize;
        self.select(next);
    }

    /// 选中第 `index` 行，并顺带请求它的二进制预览。
    ///
    /// 公开的：宿主可以程序化地选中一行（比如"打开到某个文件"），自检也用它
    /// 直接摆出"选中了某个文件"的状态。
    pub fn select(&mut self, index: usize) {
        self.selected.set(Some(index));
        // 选中行可能已经滚出视口 —— 滚最短的距离把它带回来。
        self.state.scroll_to(index);
        self.request_preview();
    }

    fn activate_selected(&mut self) -> bool {
        match self.selected.get() {
            Some(index) => self.open(index),
            None => false,
        }
    }

    /// `H` 切换点文件的显示，然后重扫 —— 数据变了，得回磁盘。
    fn toggle_hidden(&mut self) {
        self.hidden = !self.hidden;
        self.reload();
    }

    // -- 预览模式 --------------------------------------------------------

    /// 右栏当前是 hexdump 还是文本。
    pub fn mode(&self) -> PreviewMode {
        self.preview_mode
    }

    /// `T`：换一种看法。
    pub fn toggle_mode(&mut self) {
        let next = self.preview_mode.toggled();
        self.set_mode(next);
    }

    /// 切换右栏的模式。
    ///
    /// **不重新读盘** —— 字节已经在 [`Preview`] 里了，换的只是"行怎么算"
    /// （以及行数，所以两个列表都要作废重绑）。两个列表靠可见性二选一：藏起
    /// 来的那个容器高度是 0，`sync` 根本不挂行。
    pub fn set_mode(&mut self, mode: PreviewMode) {
        self.preview_mode = mode;
        self.tree
            .set_visible(self.hex_container, mode == PreviewMode::Binary);
        self.tree
            .set_visible(self.text_container, mode == PreviewMode::Text);
        self.preview_count.set(self.preview.borrow().rows_in(mode));
        // 行数变了、行的内容也变了（同一个下标在两种模式下是不同的一行）。
        self.hex_state.invalidate();
        self.text_state.invalidate();
        self.hex_state.scroll_to(0);
        self.text_state.scroll_to(0);
        self.sync_preview_labels();
    }

    // -- 分栏 ------------------------------------------------------------

    /// 主栏当前的宽度（逻辑像素）。拖动分隔条改的就是它。
    pub fn main_width(&self) -> f32 {
        self.main_width.get()
    }

    /// 预览栏当前的宽度：整宽减去主栏和把手。
    pub fn preview_width(&self) -> f32 {
        let full = self.viewport.logical_size().width;
        (full - self.main_width.get() - RESIZE_GUTTER).max(0.0)
    }

    /// 分隔条只管 `[min, max]`，管不了"窗口变窄了"。那一半在这里：
    /// 主栏最多占到给预览栏留 [`PREVIEW_MIN`] 为止。
    fn clamp_main_width(&mut self) {
        let full = self.viewport.logical_size().width;
        let max = (full - PREVIEW_MIN - RESIZE_GUTTER).max(MAIN_MIN);
        let current = self.main_width.get();
        let next = current.clamp(MAIN_MIN, max);
        if (next - current).abs() > f32::EPSILON {
            self.main_width.set(next);
            update_control(&mut self.tree, self.main_pane, |data| {
                data.layout.basis = SizeBasis::Px(next);
            });
        }
    }

    // -- 读取 ------------------------------------------------------------

    /// 把状态写回文本节点。只在真正变化时写（`set_text` 自己会判断）。
    fn sync_labels(&mut self) {
        let status = self.status_text();
        set_text(
            &mut self.tree,
            self.path_label,
            scan::display_path(&self.path),
        );
        set_text(&mut self.tree, self.status_label, status);
        set_text(&mut self.tree, self.hints_label, HINTS);
    }

    /// 状态行当前的文字。
    pub fn status_text(&self) -> String {
        match &self.status {
            Status::Ready(text) => text.clone(),
            Status::Loading => format!("读取中… {}", scan::display_path(&self.path)),
            Status::Failed(error) => error.clone(),
        }
    }

    /// 写回右栏的两行文字。
    fn sync_preview_labels(&mut self) {
        let mode = self.preview_mode;
        let (title, detail) = {
            let preview = self.preview.borrow();
            (preview.headline(), preview.detail(mode))
        };
        let detail = self.preview_note.clone().unwrap_or(detail);
        set_text(&mut self.tree, self.preview_title_label, title);
        set_text(&mut self.tree, self.preview_detail_label, detail);
    }

    /// 是否正在等一次读目录。
    pub fn is_loading(&self) -> bool {
        self.status == Status::Loading
    }

    /// 当前选中的数据下标。
    pub fn selected_index(&self) -> Option<usize> {
        self.selected.get()
    }

    /// 滚动偏移（逻辑像素）。
    pub fn offset(&self) -> f32 {
        self.state.offset()
    }

    /// 当前挂在树上的行数（池大小，不是数据量）。
    pub fn pool_size(&self) -> usize {
        self.state.pool_size()
    }

    /// 显示在用的那个预览列表挂在树上的行数（池大小，不是数据量）。
    ///
    /// 藏起来的那个列表池是 0 —— 这正是"待命的那个不花钱"的证据。
    pub fn preview_pool_size(&self) -> usize {
        match self.preview_mode {
            PreviewMode::Binary => self.hex_state.pool_size(),
            PreviewMode::Text => self.text_state.pool_size(),
        }
    }

    /// 切换按钮的中心点。
    ///
    /// 自检要能**真的点一下**按钮来换模式，而不是直接调 `set_mode` —— 但它
    /// 不能靠猜坐标（按钮的位置取决于标题和字号），所以从解析出来的矩形里拿。
    pub fn tab_center(&self, mode: PreviewMode) -> Option<Vec2> {
        let id = match mode {
            PreviewMode::Text => self.text_tab,
            PreviewMode::Binary => self.binary_tab,
        };
        draw_ui::control(&self.tree, id).map(|data| {
            Vec2::new(
                (data.rect.left() + data.rect.right()) / 2.0,
                (data.rect.top() + data.rect.bottom()) / 2.0,
            )
        })
    }

    /// 待命的那个列表挂了几行（应该一直是 0）。
    pub fn idle_pool_size(&self) -> usize {
        match self.preview_mode {
            PreviewMode::Binary => self.text_state.pool_size(),
            PreviewMode::Text => self.hex_state.pool_size(),
        }
    }

    /// 右栏有多少行数据（当前模式下）。
    pub fn preview_rows(&self) -> usize {
        self.preview_count.get()
    }

    /// 右栏副标题的文字（`--selfcheck` 用它断言模式写清楚了）。
    pub fn preview_detail(&self) -> String {
        self.preview.borrow().detail(self.preview_mode)
    }

    /// 视口覆盖到的数据行区间。
    pub fn visible_range(&self) -> std::ops::Range<usize> {
        self.state.visible_range()
    }

    /// 直接滚动（自检和 `cargo test` 用；真实滚动走滚轮 -> `InputEvent::Wheel`）。
    #[allow(dead_code)]
    pub fn scroll_by(&mut self, delta: f32) {
        self.state.scroll_by(delta);
    }

    /// 滚最短的距离把 `index` 带进视口。`cargo test` 用它断言滚动后的行区间。
    #[allow(dead_code)]
    pub fn scroll_to(&mut self, index: usize) {
        self.state.scroll_to(index);
    }

    /// 控件数：跟数据量无关，这是虚拟化的证据。
    pub fn control_count(&self) -> usize {
        draw_ui::control_count(&self.tree)
    }

    pub fn theme(&self) -> &'static dyn Theme {
        self.theme
    }
}

/// 一个模式切换按钮：写着模式名，**在用的那个有底色**。
///
/// 底色是每帧算的（`dynamic_background`），所以切换模式不用重建这棵树；点击
/// 只往共享格子里写一个请求，由 [`Browser::update`] 取走（`on_click` 的回调
/// 拿不到 `&mut Browser`）。
fn mode_tab(
    theme: &'static dyn Theme,
    mode: PreviewMode,
    active: &Rc<Cell<PreviewMode>>,
    request: &Rc<Cell<Option<PreviewMode>>>,
) -> Flex {
    let flag = active.clone();
    let clicked = request.clone();
    Flex::row()
        .gap(0.0)
        .padding(Edges::new(space::SM, space::XS, space::SM, space::XS))
        .on_click(move || clicked.set(Some(mode)))
        .dynamic_background(move |_| {
            if flag.get() == mode {
                SurfaceStyle::new(theme.surface(SurfaceLevel::Raised))
            } else {
                SurfaceStyle::new(draw_core::Color::TRANSPARENT)
            }
        })
        .child(Text::small(mode.label(), theme))
}

#[cfg(test)]
mod tests {
    use super::*;
    use draw_core::{PointerButton, Size, Vec2};

    /// 一份不碰磁盘的清单：10 个目录 + `n` 个文件。
    fn fixture(count: usize) -> Listing {
        let entries = (0..count)
            .map(|index| Entry {
                name: format!("entry_{index:05}"),
                is_dir: index % 10 == 0,
                size: (index as u64) * 1_024,
                modified: Some(1_789_886_988 + index as u64),
            })
            .collect();
        Listing::fixture("/tmp/quill-fixture", entries)
    }

    fn viewport() -> ViewportSize {
        ViewportSize::new(Size::new(1100.0, 680.0))
    }

    fn browser_with(count: usize) -> Browser {
        let mut app = Browser::new(default_theme(Mode::Dark), PathBuf::from("/tmp"), false);
        // 起始目录的那一次读请求：这些测试不关心它（宿主会取走），丢掉。
        let _ = app.take_navigation();
        app.apply_listing(fixture(count));
        app.layout(viewport());
        app
    }

    #[test]
    fn the_row_pool_follows_the_viewport_not_the_data() {
        let small = browser_with(50);
        let huge = browser_with(100_000);
        assert_eq!(
            small.control_count(),
            huge.control_count(),
            "10 万行和 50 行的控件数一样"
        );
        assert_eq!(small.pool_size(), huge.pool_size());
        assert!(
            huge.pool_size() < 40,
            "池只有视口那几行，实际 {}",
            huge.pool_size()
        );
    }

    #[test]
    fn a_scroll_keeps_the_pool_size() {
        let mut app = browser_with(10_000);
        let before = app.pool_size();
        for _ in 0..40 {
            app.scroll_by(2.5 * ROW_HEIGHT);
            app.layout(viewport());
        }
        assert_eq!(app.pool_size(), before, "滚动只挪动并重用行，不新增行");
        assert!(app.offset() > 0.0);
    }

    #[test]
    fn the_header_shows_the_path_and_the_count() {
        let app = browser_with(7);
        assert_eq!(app.status_text(), "7 项");
        assert_eq!(app.path(), Path::new("/tmp/quill-fixture"));
    }

    #[test]
    fn a_failed_listing_shows_why() {
        let mut app = Browser::new(default_theme(Mode::Dark), PathBuf::from("/nope"), false);
        app.apply_listing(Listing::failed("/nope", "目录不存在：/nope"));
        assert_eq!(app.status_text(), "目录不存在：/nope");
        assert_eq!(app.selected_index(), None);
    }

    #[test]
    fn an_empty_directory_says_so() {
        let mut app = Browser::new(default_theme(Mode::Dark), PathBuf::from("/tmp"), false);
        app.apply_listing(Listing::fixture("/tmp/empty", Vec::new()));
        assert_eq!(app.status_text(), "空目录");
        assert_eq!(app.selected_index(), None);
    }

    #[test]
    fn arrows_move_the_selection() {
        let mut app = browser_with(100);
        assert_eq!(app.selected_index(), Some(0), "新清单选中第一行");
        app.key(Key::ArrowDown);
        assert_eq!(app.selected_index(), Some(1));
        app.key(Key::ArrowDown);
        app.key(Key::ArrowUp);
        assert_eq!(app.selected_index(), Some(1));
        app.key(Key::End);
        assert_eq!(app.selected_index(), Some(99));
        app.key(Key::Home);
        assert_eq!(app.selected_index(), Some(0));
    }

    /// 选到头不该绕回去 —— 文件浏览器的上下键不是循环列表。
    #[test]
    fn the_selection_clamps_at_the_ends() {
        let mut app = browser_with(3);
        app.key(Key::ArrowUp);
        assert_eq!(app.selected_index(), Some(0));
        app.key(Key::End);
        app.key(Key::ArrowDown);
        assert_eq!(app.selected_index(), Some(2));
    }

    #[test]
    fn opening_a_directory_requests_it() {
        let mut app = browser_with(10);
        // entry_00000 是目录（下标能被 10 整除的是目录）。
        app.key(Key::Enter);
        let pending = app.take_navigation().expect("请求了一次读目录");
        assert_eq!(pending, PathBuf::from("/tmp/quill-fixture/entry_00000"));
        assert!(app.is_loading(), "状态行显示读取中");
    }

    /// 点一个文件只选中，不该发起读盘 —— 但**会**请求一次二进制预览。
    #[test]
    fn opening_a_file_only_selects_but_asks_for_a_preview() {
        let mut app = browser_with(10);
        app.select(1);
        app.open(1);
        assert!(app.take_navigation().is_none());
        assert!(app.status_text().contains("entry_00001"));
        assert_eq!(
            app.take_preview_request(),
            Some(PathBuf::from("/tmp/quill-fixture/entry_00001"))
        );
    }

    /// 目录没有字节可看：选中它应该清掉右栏，而不是请求预览。
    #[test]
    fn a_directory_has_no_bytes_to_preview() {
        let mut app = browser_with(10);
        app.apply_preview(Preview::fixture("old.bin", vec![1, 2, 3]));
        app.select(0); // entry_00000 是目录
        assert!(app.take_preview_request().is_none());
        assert_eq!(app.preview_rows(), 0, "右栏被清空");
    }

    /// 快速移过一百行，最后只该剩一个请求 —— 中间那些被覆盖了。
    #[test]
    fn sweeping_the_selection_leaves_one_request() {
        let mut app = browser_with(200);
        for index in 0..100 {
            app.select(index);
        }
        assert_eq!(
            app.take_preview_request(),
            Some(PathBuf::from("/tmp/quill-fixture/entry_00099"))
        );
        assert!(app.take_preview_request().is_none(), "只发一个请求");
    }

    #[test]
    fn going_up_requests_the_parent() {
        let mut app = browser_with(10);
        app.go_up();
        assert_eq!(
            app.take_navigation(),
            Some(PathBuf::from("/tmp")),
            "上级是 /tmp"
        );
    }

    #[test]
    fn reloading_rereads_the_same_directory() {
        let mut app = browser_with(10);
        app.reload();
        assert_eq!(
            app.take_navigation(),
            Some(PathBuf::from("/tmp/quill-fixture"))
        );
    }

    /// 切换隐藏文件也要回磁盘 —— 光改标记不会让列表重新读行。
    #[test]
    fn toggling_hidden_rereads() {
        let mut app = browser_with(10);
        assert!(!app.hidden());
        app.toggle_hidden();
        assert!(app.hidden());
        assert!(app.take_navigation().is_some(), "重扫一次");
    }

    /// 点击转交：`on_activate` 回调拿不到 `&mut self`，所以 `update` 取走它。
    #[test]
    fn a_click_becomes_an_open_request() {
        let mut app = browser_with(10);
        app.activated.set(Some(0));
        app.update();
        assert!(app.take_navigation().is_some(), "点目录就进去");
    }

    #[test]
    fn scrolling_moves_the_visible_range() {
        let mut app = browser_with(1_000);
        assert_eq!(app.visible_range().start, 0);
        app.scroll_to(500);
        app.layout(viewport());
        assert!(
            app.visible_range().contains(&500),
            "滚到第 500 行后它应该在视口里：{:?}",
            app.visible_range()
        );
    }

    // -- 分栏 ------------------------------------------------------------

    #[test]
    fn the_two_panes_split_the_width() {
        let app = browser_with(10);
        assert!((app.main_width() - MAIN_WIDTH).abs() < 1e-3);
        // 1100 = 主栏 560 + 把手 6 + 预览栏 534。
        assert!((app.preview_width() - 534.0).abs() < 1e-3);
    }

    /// 拖分隔条：主栏变宽，预览栏让出同样的宽度。
    #[test]
    fn dragging_the_gutter_resizes_the_preview_pane() {
        let mut app = browser_with(10);
        // 把手在主栏右边界的 6px 里。
        let gutter = |app: &Browser| Vec2::new(app.main_width() + RESIZE_GUTTER / 2.0, 360.0);
        let start = gutter(&app);
        app.event(&InputEvent::PointerDown {
            position: start,
            button: PointerButton::Left,
        });
        app.event(&InputEvent::PointerMove {
            position: start + Vec2::new(60.0, 0.0),
        });
        app.event(&InputEvent::PointerUp {
            position: start + Vec2::new(60.0, 0.0),
            button: PointerButton::Left,
        });
        app.layout(viewport());

        assert!((app.main_width() - (MAIN_WIDTH + 60.0)).abs() < 1e-3);
        assert!((app.preview_width() - (534.0 - 60.0)).abs() < 1e-3);
    }

    /// 往左拖过头：主栏停在最小值，预览栏也就有了上限。
    #[test]
    fn the_main_pane_clamps_at_its_minimum() {
        let mut app = browser_with(10);
        let start = Vec2::new(app.main_width() + RESIZE_GUTTER / 2.0, 360.0);
        app.event(&InputEvent::PointerDown {
            position: start,
            button: PointerButton::Left,
        });
        app.event(&InputEvent::PointerMove {
            position: start - Vec2::new(1000.0, 0.0),
        });
        app.event(&InputEvent::PointerUp {
            position: start - Vec2::new(1000.0, 0.0),
            button: PointerButton::Left,
        });
        app.layout(viewport());
        assert!((app.main_width() - MAIN_MIN).abs() < 1e-3);
    }

    /// 分隔条管不了窗口变窄 —— 那一半由 `layout` 补上，否则预览栏会被挤没。
    #[test]
    fn a_narrow_window_keeps_the_preview_pane_alive() {
        let mut app = browser_with(10);
        app.layout(ViewportSize::new(Size::new(600.0, 600.0)));
        let expected = 600.0 - PREVIEW_MIN - RESIZE_GUTTER;
        assert!(
            (app.main_width() - expected).abs() < 1e-3,
            "主栏收到 {}，实际 {}",
            expected,
            app.main_width()
        );
        assert!((app.preview_width() - PREVIEW_MIN).abs() < 1e-3);
    }

    // -- 预览 ------------------------------------------------------------

    /// 64 KiB = 4096 行，但树上只有视口那几行 —— 预览不随文件大小增长。
    #[test]
    fn a_full_preview_mounts_only_the_visible_rows() {
        let mut app = browser_with(10);
        app.select(1);
        let small = app.control_count();
        app.apply_preview(Preview::fixture("big.bin", vec![0x5a; 64 * 1024]));
        app.layout(viewport());

        assert_eq!(app.preview_rows(), 64 * 1024 / 16);
        assert!(
            app.preview_pool_size() < 40,
            "hex 行池只有视口那点行，实际 {}",
            app.preview_pool_size()
        );
        // 挂了 4096 行的数据，控件数只增加了池里那几行。
        assert!(
            app.control_count() - small < 130,
            "新增控件 {} 个，应该只有行池那么大",
            app.control_count() - small
        );
    }

    #[test]
    fn a_preview_shows_its_name_in_the_pane() {
        let mut app = browser_with(10);
        app.select(1);
        assert_eq!(app.preview_rows(), 0, "还没回来");
        app.apply_preview(Preview::fixture("blob.bin", b"hello".to_vec()));
        app.layout(viewport());
        assert_eq!(app.preview_rows(), 1);
    }

    // -- 预览模式 --------------------------------------------------------

    /// 同一份字节，两种看法：**行数不一样**，这就是模式存在的意义。
    #[test]
    fn the_mode_decides_what_a_row_is() {
        let mut app = browser_with(10);
        app.select(1);
        app.apply_preview(Preview::fixture("a.txt", b"one\ntwo\nthree\n".to_vec()));
        app.layout(viewport());

        assert_eq!(app.mode(), PreviewMode::Binary, "默认看 hexdump");
        assert_eq!(app.preview_rows(), 1, "14 字节是一行 hex");
        app.set_mode(PreviewMode::Text);
        app.layout(viewport());
        assert_eq!(app.preview_rows(), 3, "同一个文件是三行文本");
    }

    /// 藏起来的那个列表连行池都不挂 —— 两种模式同时在树上也不花钱。
    #[test]
    fn only_the_visible_mode_owns_a_row_pool() {
        let mut app = browser_with(10);
        app.select(1);
        app.apply_preview(Preview::fixture("big.bin", vec![0x5a; 64 * 1024]));
        app.layout(viewport());
        assert!(app.preview_pool_size() > 1, "hexdump 挂了行池");
        assert_eq!(app.idle_pool_size(), 0, "待命的文本列表一行都没挂");

        app.set_mode(PreviewMode::Text);
        app.layout(viewport());
        // 64 KiB 没有换行符，文本模式只有一行 —— 池就只有一行。
        assert_eq!(app.preview_pool_size(), 1);
        assert!(app.idle_pool_size() > 1, "hexdump 的行池留着，随时能切回去");
    }

    /// `T` 键：换一种看法。
    #[test]
    fn t_key_toggles_the_mode() {
        let mut app = browser_with(10);
        assert_eq!(app.mode(), PreviewMode::Binary);
        app.key(Key::Character('t'));
        assert_eq!(app.mode(), PreviewMode::Text);
        app.key(Key::Character('t'));
        assert_eq!(app.mode(), PreviewMode::Binary);
    }

    /// 换文件不该把人从文本模式扔回 hexdump —— 模式是"我怎么看"，不是"这是什么"。
    #[test]
    fn the_mode_survives_a_new_selection() {
        let mut app = browser_with(10);
        app.select(1);
        app.apply_preview(Preview::fixture("a.txt", b"one\ntwo\n".to_vec()));
        app.set_mode(PreviewMode::Text);
        app.select(2);
        app.apply_preview(Preview::fixture("b.txt", b"x\ny\nz\n".to_vec()));
        app.layout(viewport());
        assert_eq!(app.mode(), PreviewMode::Text);
        assert_eq!(app.preview_rows(), 3);
    }

    /// 点一下按钮：`on_click` 只写共享格子，由 `update` 取走。
    #[test]
    fn clicking_a_tab_switches_the_mode() {
        let mut app = browser_with(10);
        app.layout(viewport());
        let point = app
            .tab_center(PreviewMode::Text)
            .expect("切换按钮已经排布过了");
        app.event(&InputEvent::PointerDown {
            position: point,
            button: PointerButton::Left,
        });
        app.event(&InputEvent::PointerUp {
            position: point,
            button: PointerButton::Left,
        });
        assert_eq!(app.mode(), PreviewMode::Binary, "还没 update");
        app.update();
        assert_eq!(app.mode(), PreviewMode::Text, "点一下就换过去了");
    }

    /// 副标题把模式写清楚 —— 否则看不出自己在看哪一种。
    #[test]
    fn the_detail_line_names_the_mode() {
        let mut app = browser_with(10);
        app.select(1);
        app.apply_preview(Preview::fixture("a.txt", b"one\ntwo\n".to_vec()));
        assert!(
            app.preview_detail().contains("二进制"),
            "{}",
            app.preview_detail()
        );
        app.set_mode(PreviewMode::Text);
        assert!(
            app.preview_detail().contains("文本"),
            "{}",
            app.preview_detail()
        );
    }

    /// 读失败的预览（目录、权限）把原因写进右栏，不留一个空的 hex 区。
    #[test]
    fn a_failed_preview_shows_why() {
        let mut app = browser_with(10);
        app.select(1);
        app.apply_preview(Preview::failed("/tmp/x.bin", "是一个目录"));
        app.layout(viewport());
        assert_eq!(app.preview_rows(), 0);
    }
}
