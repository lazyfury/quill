//! 文件浏览器视图：一个路径栏、一个虚拟列表、一条状态行。
//!
//! 全部后端无关 —— 视图是一棵 `SceneTree`，由 `draw_components` 搭起来、
//! `draw_ui` 排布。宿主（[`crate::host`]）拥有窗口、wgpu 后端和读盘的工作
//! 线程，跟 `demo_app` / `BalanceApp` 的分工一样。
//!
//! ```text
//! Input -> Browser::event -> Browser::layout -> Browser::paint
//! ```
//!
//! ## 读目录不阻塞
//!
//! 视图从不自己碰磁盘：点开一个目录只是把目标路径记进 [`Browser::take_navigation`]
//! 的"待办"，宿主拿到它、在工作线程上跑 [`scan::scan`]，再把 [`Listing`] 通过
//! [`Browser::apply_listing`] 送回来。所以扫 `/usr` 那种上万条目的目录时，界面
//! 照常滚动 —— 这也是这份 demo 存在的理由：行数不再决定帧成本（见
//! `docs/benchmarking.md`）。
//!
//! ## 每帧三步
//!
//! 列表的行池大小取决于容器的**解析高度**，所以顺序不能反：
//!
//! ```text
//! layout   ->   ListState::sync   ->   改过树就再 layout 一次
//! ```
//!
//! 见 [`Browser::layout`]。

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use draw_components::{
    set_text, Component, Divider, Flex, List, ListColumn, ListState, NodeRef, Text,
};
use draw_core::{Edges, EventResult, InputEvent, Key, NodeId, ViewportSize};
use draw_render::PaintContext;
use draw_scene::{SceneChild, SceneTree};
use draw_theme::{space, Theme, Tone};
use draw_ui::{MouseFilter, TextMeasurer};

use crate::scan::{self, Entry, Listing};

/// 一行的高度（逻辑像素）。列表的池大小 = `ceil(容器高 / 行高) + 1`。
pub const ROW_HEIGHT: f32 = 26.0;

/// 三列的宽度：名字吃掉剩下的空间，尺寸和日期是定宽的（右对齐更好扫）。
const SIZE_COLUMN: f32 = 84.0;
const TIME_COLUMN: f32 = 132.0;

/// 状态行的操作提示。
const HINTS: &str = "↑↓ 选择 · Enter 打开 · Backspace 上级 · R 重扫 · H 隐藏文件 · 滚轮滚动";

/// 节点槽位，声明式构建时填、构建后读。
#[derive(Default)]
struct Refs {
    path: NodeRef,
    status: NodeRef,
    hints: NodeRef,
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
    theme: Theme,
    /// 当前清单的行。列表的 `source` 闭包按需读它，所以数据不必变成控件。
    entries: Rc<RefCell<Vec<Entry>>>,
    /// 行数。列表每次 sync 都读它 —— 换目录只要 `set` 一下。
    count: Rc<Cell<usize>>,
    /// 选中行（**数据下标**，不跟行池槽位）。
    selected: Rc<Cell<Option<usize>>>,
    /// 点击请求打开的行：回调拿不到 `&mut self`，所以用一格共享状态转交。
    activated: Rc<Cell<Option<usize>>>,
    state: ListState,
    path_label: NodeId,
    status_label: NodeId,
    hints_label: NodeId,
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
    /// 目录的内容由宿主稍后通过 [`Browser::apply_listing`] 送进来；这里只把
    /// 结构建好，所以构造不碰磁盘。
    pub fn new(theme: Theme, path: PathBuf, hidden: bool) -> Self {
        let refs = Refs::default();
        let path_clone = path.clone();
        let entries: Rc<RefCell<Vec<Entry>>> = Rc::new(RefCell::new(Vec::new()));
        let count = Rc::new(Cell::new(0));
        let selected = Rc::new(Cell::new(None));
        let activated = Rc::new(Cell::new(None));

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

        // 布局根的子节点按 anchors 摆，flex 从下一层才开始 —— 所以排页面的
        // column 是根的唯一子节点（`demo_app` 也是这个形状）。
        let content = Flex::column()
            .mouse_filter(MouseFilter::Ignore)
            .gap(space::MD)
            .padding(Edges::all(space::LG))
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

        let tree = Flex::column()
            .mouse_filter(MouseFilter::Ignore)
            .child(content)
            .into_tree();

        let mut app = Self {
            tree,
            theme,
            entries,
            count,
            selected,
            activated,
            state,
            path_label: refs.path.get().expect("path label mounted"),
            status_label: refs.status.get().expect("status label mounted"),
            hints_label: refs.hints.get().expect("hints label mounted"),
            path,
            // 起始目录立刻要读：宿主的第一帧就会把这个请求取走。
            pending: Some(path_clone),
            hidden,
            status: Status::Loading,
            viewport: ViewportSize::new(draw_core::Size::new(900.0, 600.0)),
        };
        app.sync_labels();
        app
    }

    // -- 生命周期 --------------------------------------------------------

    /// 用后端真实字体的度量，让排版量到的宽度跟画出来的宽度一致。
    pub fn set_text_measurer(&mut self, measurer: Rc<dyn TextMeasurer>) {
        draw_ui::set_text_measurer(&mut self.tree, measurer);
    }

    /// 处理一次点击的"打开"请求（回调里转交过来的），返回是否动了树。
    pub fn update(&mut self) -> bool {
        let Some(index) = self.activated.take() else {
            return false;
        };
        self.selected.set(Some(index));
        self.open(index)
    }

    /// 排布：先量容器，再同步行池，池变了就再排一次。
    pub fn layout(&mut self, viewport: ViewportSize) {
        self.viewport = viewport;
        draw_ui::layout(&mut self.tree, viewport);
        if self.state.sync(&mut self.tree) {
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
    /// `draw_ui::handle_input` —— 滚轮会沿着祖先链找到列表的滚动回调。
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
        self.sync_labels();
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

    /// 打开第 `index` 行：目录就进去，文件只是选中。
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
            self.sync_labels();
            false
        }
    }

    fn request(&mut self, path: PathBuf) {
        self.status = Status::Loading;
        self.pending = Some(path);
        self.sync_labels();
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

    fn select(&mut self, index: usize) {
        self.selected.set(Some(index));
        // 选中行可能已经滚出视口 —— 滚最短的距离把它带回来。
        self.state.scroll_to(index);
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

    // -- 读取 ------------------------------------------------------------

    /// 把状态写回两个文本节点。只在真正变化时写（`set_text` 自己会判断）。
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

    pub fn theme(&self) -> Theme {
        self.theme
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use draw_core::Size;

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
        ViewportSize::new(Size::new(900.0, 600.0))
    }

    fn browser_with(count: usize) -> Browser {
        let mut app = Browser::new(Theme::dark(), PathBuf::from("/tmp"), false);
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
        let mut app = Browser::new(Theme::dark(), PathBuf::from("/nope"), false);
        app.apply_listing(Listing::failed("/nope", "目录不存在：/nope"));
        assert_eq!(app.status_text(), "目录不存在：/nope");
        assert_eq!(app.selected_index(), None);
    }

    #[test]
    fn an_empty_directory_says_so() {
        let mut app = Browser::new(Theme::dark(), PathBuf::from("/tmp"), false);
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

    /// 点一个文件只选中，不该发起读盘。
    #[test]
    fn opening_a_file_only_selects() {
        let mut app = browser_with(10);
        app.select(1);
        app.open(1);
        assert!(app.take_navigation().is_none());
        assert!(app.status_text().contains("entry_00001"));
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
}
