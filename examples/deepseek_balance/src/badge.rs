//! 多窗口测试用的第二个窗口：一张无边框、无装饰的浮层，贴在桌面左下角。
//!
//! 视图**用组件搭**（[`Column`] + [`Text`]），不再手发绘制命令：和主视图同一套
//! `draw_ui::layout` / `draw_ui::paint`，只是这棵树小到只有一列两行字。
//!
//! 它**没有背景**：整窗不透明的只有这两行字，其余像素是窗口的透明清屏色，所以
//! 桌面从字缝里透出来。也正因如此宿主把系统阴影也关了 —— AppKit 是拿窗口的 alpha
//! 当蒙版描阴影的，字以外全透明，阴影就会去描字形（见 `host.rs::init_badge`）。
//!
//! # 数据从哪来
//!
//! 徽章**不是第二个数据源**：它没有定时器、没有 worker，也不会自己发请求。宿主把
//! 主视图拿到的那**一份**状态交给它（`host.rs::App::refresh_badge`），标题、前缀和
//! 数值随当前标签页（DeepSeek 余额 / OpenCode Go 余量）切换，所以两个窗口不可能
//! 各说各话。
//!
//! 与主窗口/面板一样，它不碰窗口、不碰网络 —— 自己的 window / surface /
//! backend 由宿主 `host.rs` 提供；拿到后端的真实字体度量后，布局引擎负责排版。

use std::rc::Rc;

use draw_components::{Column, Component, Flex, NodeRef, Text};
use draw_core::{Color, Edges, NodeId, ViewportSize};
use draw_render::PaintContext;
use draw_scene::{SceneChild, SceneTree};
#[cfg(test)]
use draw_theme::{default_theme, Mode};
use draw_theme::{space, TextSize, Theme};
use draw_ui::{self, MouseFilter, TextMeasurer};

use crate::api::Balance;

/// 徽章窗口的逻辑尺寸。
pub const BADGE_WIDTH: f32 = 180.0;
/// 贴桌面用的**紧凑**高度：内容（内边距 32 + 标题 27.5 + 行距 2 + 余额 21.2 ≈ 83）
/// 其实比它高，底部的内边距因此被吃掉 —— 但两行文字本身仍在窗内，不会被裁。
/// 「不被裁」由 `the_text_stays_inside_the_window` 守着；想要内边距完整，把高度
/// 提到 83 以上即可（窗口无边框，超出即被裁掉，没有滚动可言）。
pub const BADGE_HEIGHT: f32 = 72.0;

/// 内容距窗口边缘的内边距。
pub const CONTENT_PADDING: f32 = space::LG;
/// 两行之间的间距。
pub const LINE_GAP: f32 = space::XXXS;

/// DeepSeek 标签页的标题行。
pub const TITLE: &str = "Deepseek";
/// DeepSeek 余额行的前缀。
pub const BALANCE_PREFIX: &str = "余额：";
/// OpenCode Go 标签页的标题行。
pub const GO_TITLE: &str = "OpenCode Go";
/// OpenCode Go 余量行的前缀。
pub const GO_PREFIX: &str = "余量：";
/// 还没拿到第一份回复时的占位值（与主视图同一套文案）。
pub const IDLE: &str = "—";
/// 请求在途。
pub const LOADING: &str = "刷新中…";
/// 上一次请求失败。
pub const FAILED: &str = "刷新失败";

/// 数值行显示什么（不含前缀）。
///
/// 这是**视图状态**，不是数据源：宿主把同一个请求结果同时喂给主视图和这里，
/// 也用它标记「请求已发出」，所以徽章自己不取数、不排期。标题和前缀由宿主按
/// 当前标签页给（`show`），同一份状态因此能画成 DeepSeek 余额或 Go 余量。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum State {
    /// 还没刷新过。
    Idle,
    /// 请求在途。
    Loading,
    /// 拿到回复，值是 [`Balance::headline`] / [`crate::go::GoUsage::headline`]。
    Ready(String),
    /// 上一次请求失败。
    Failed,
}

impl State {
    /// 数值本身，例如 `¥110.00`。
    pub fn value(&self) -> &str {
        match self {
            State::Idle => IDLE,
            State::Loading => LOADING,
            State::Ready(headline) => headline.as_str(),
            State::Failed => FAILED,
        }
    }

    /// 整行文案，例如 `余额：¥110.00`。
    pub fn line(&self, prefix: &str) -> String {
        format!("{prefix}{}", self.value())
    }

    /// 从一个已经完成的 DeepSeek 请求取状态 —— 宿主拿到什么就传什么。
    pub fn from_result(result: &Result<Balance, String>) -> Self {
        match result {
            Ok(balance) => State::Ready(balance.headline()),
            Err(_) => State::Failed,
        }
    }
}

/// 徽章视图：一棵「一列 + 两行字」的组件树。没有底色、没有边框，整窗只有字。
pub struct BadgeApp {
    tree: SceneTree,
    theme: &'static dyn Theme,
    /// 标题行节点：切标签页时改写它。
    title: NodeId,
    /// 数值行节点：换数据就是往它写一次文本。
    balance: NodeId,
}

impl BadgeApp {
    /// Mounts the tree. The layout root's own child is placed by anchors, so the
    /// column fills the window and its padding is what insets the text; flex
    /// starts one level down. Nothing in here paints a backdrop, which is what
    /// leaves the window's transparent clear colour showing through as the
    /// badge's background.
    pub fn new(theme: &'static dyn Theme) -> Self {
        let title = NodeRef::new();
        let balance = NodeRef::new();
        let content = Column::new()
            .padding(Edges::all(CONTENT_PADDING))
            .gap(LINE_GAP)
            .mouse_filter(MouseFilter::Ignore)
            .child(Text::heading(TITLE, theme).color(Color::WHITE).ref_(&title))
            .child(
                Text::new(State::Idle.line(BALANCE_PREFIX), theme)
                    .size(TextSize::Subheading)
                    .color(Color::WHITE)
                    .ref_(&balance),
            );
        let tree = Flex::column()
            .mouse_filter(MouseFilter::Ignore)
            .child(content)
            .into_tree();
        Self {
            tree,
            theme,
            title: title.get().expect("the title line is mounted"),
            balance: balance.get().expect("the balance line is mounted"),
        }
    }

    /// The theme this view was built with (a value, so it is `Copy`).
    pub fn theme(&self) -> &'static dyn Theme {
        self.theme
    }

    /// Read-only view of the tree, for tests.
    pub fn tree(&self) -> &SceneTree {
        &self.tree
    }

    /// The title line as it stands.
    #[cfg(test)]
    pub fn title_text(&self) -> Option<&str> {
        draw_ui::widget(&self.tree, self.title).and_then(|widget| widget.text())
    }

    /// The value line as it stands (what the next frame would paint).
    pub fn balance_text(&self) -> Option<&str> {
        draw_ui::widget(&self.tree, self.balance).and_then(|widget| widget.text())
    }

    /// Shows the active tab: its title, its value prefix and the state.
    ///
    /// The host calls this with the very same `Result` it hands the main view,
    /// so the two windows cannot disagree, and with the loading/failed states
    /// around it.
    pub fn show(&mut self, title: &str, prefix: &str, state: &State) {
        draw_components::set_text(&mut self.tree, self.title, title);
        draw_components::set_text(&mut self.tree, self.balance, state.line(prefix));
    }

    /// Installs the backend's real font metrics so measured text matches
    /// rendered text — the same adapter the main view uses.
    pub fn set_text_measurer(&mut self, measurer: Rc<dyn TextMeasurer>) {
        draw_ui::set_text_measurer(&mut self.tree, measurer);
    }

    /// Arranges the tree for a surface of `viewport` logical pixels.
    pub fn layout(&mut self, viewport: ViewportSize) {
        draw_ui::layout(&mut self.tree, viewport);
    }

    /// Paints the tree. Unlike the menu-bar panel there is no backdrop command
    /// to issue first: the tree paints nothing but text, and every pixel it does
    /// not paint stays transparent — which is what lets the desktop show
    /// through.
    pub fn paint(&self, ctx: &mut PaintContext) {
        draw_ui::paint(&self.tree, ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::BalanceInfo;
    use draw_core::{Size, Vec2};
    use draw_render::{DrawCommand, TextAlign};

    /// The canned reply the host would have fetched: one currency, so the
    /// headline is `¥110.00`.
    fn reply() -> Balance {
        Balance {
            is_available: true,
            balance_infos: vec![BalanceInfo {
                currency: "CNY".to_string(),
                total_balance: "110.00".to_string(),
                granted_balance: "10.00".to_string(),
                topped_up_balance: "100.00".to_string(),
            }],
        }
    }

    /// Lays `app` out for the badge window and paints one frame.
    fn frame_of(app: &mut BadgeApp) -> Vec<DrawCommand> {
        app.layout(ViewportSize::new(Size::new(BADGE_WIDTH, BADGE_HEIGHT)));
        let mut ctx = PaintContext::new();
        app.paint(&mut ctx);
        ctx.into_draw_list().commands().to_vec()
    }

    /// [`frame_of`] for a freshly mounted view.
    fn frame(theme: &'static dyn Theme) -> Vec<DrawCommand> {
        frame_of(&mut BadgeApp::new(theme))
    }

    /// Every text command in paint order: text, position, size, alignment.
    fn lines(commands: &[DrawCommand]) -> Vec<(String, Vec2, f32, TextAlign)> {
        commands
            .iter()
            .filter_map(|command| match command {
                DrawCommand::DrawText {
                    text,
                    position,
                    font_size,
                    align,
                    ..
                } => Some((text.clone(), *position, *font_size, *align)),
                _ => None,
            })
            .collect()
    }

    /// The badge is an overlay, not a card: every command it emits is text. An
    /// opaque command here — a fill, a border — would cover the desktop, and
    /// would put the window-server shadow back too (AppKit traces the shadow
    /// from the window's alpha, so it would hug whatever is opaque).
    #[test]
    fn nothing_but_the_text_is_painted() {
        let commands = frame(default_theme(Mode::Dark));
        assert_eq!(commands.len(), 2, "two lines of text and nothing else");
        assert!(
            commands
                .iter()
                .all(|command| matches!(command, DrawCommand::DrawText { .. })),
            "a badge frame may only draw text, got {commands:?}"
        );
    }

    /// Two lines, in order, sharing the content padding.
    #[test]
    fn the_two_lines_stack_inside_the_window() {
        let commands = frame(default_theme(Mode::Dark));
        let lines = lines(&commands);
        assert_eq!(lines.len(), 2, "one line per text node");
        assert_eq!(lines[0].0, TITLE);
        assert_eq!(lines[1].0, State::Idle.line(BALANCE_PREFIX));
        assert_eq!(
            lines[0].1.x, CONTENT_PADDING,
            "the text starts at the content padding"
        );
        assert_eq!(lines[1].1.x, CONTENT_PADDING, "both share the same inset");
        assert!(lines[0].1.y < lines[1].1.y, "the title sits above");
        assert_eq!(lines[0].2, TextSize::Heading.px());
        assert_eq!(lines[1].2, TextSize::Subheading.px());
        assert_eq!(lines[0].3, TextAlign::Left);
    }

    /// The window is borderless and deliberately tight, so the one thing that
    /// must hold is that nothing is cut off: every line's laid-out box ends
    /// inside the surface. The *bottom* padding is what gives way — the content
    /// is taller than `BADGE_HEIGHT - 2 * CONTENT_PADDING` — so this asserts the
    /// top inset and the window edges, not symmetric padding. Guards the sizes
    /// above against a type-scale change.
    #[test]
    fn the_text_stays_inside_the_window() {
        let mut app = BadgeApp::new(default_theme(Mode::Dark));
        app.layout(ViewportSize::new(Size::new(BADGE_WIDTH, BADGE_HEIGHT)));
        let tree = app.tree();

        let mut boxes = Vec::new();
        for id in tree.iter_visible() {
            if matches!(
                draw_ui::widget(tree, id),
                Some(draw_ui::Widget::Label { .. })
            ) {
                boxes.push(
                    draw_ui::control(tree, id)
                        .expect("a label has a control")
                        .rect,
                );
            }
        }
        assert_eq!(boxes.len(), 2, "one box per line");

        let first = boxes[0];
        assert!(
            first.top() >= CONTENT_PADDING,
            "the title must clear the top padding: {first:?}"
        );
        assert!(first.top() < boxes[1].top(), "the title sits above");
        for rect in boxes {
            assert!(
                rect.top() >= 0.0 && rect.bottom() <= BADGE_HEIGHT,
                "a line is cut off by the {BADGE_HEIGHT}pt window: {rect:?}"
            );
            assert!(
                rect.left() >= 0.0 && rect.right() <= BADGE_WIDTH,
                "a line runs past the {BADGE_WIDTH}pt width: {rect:?}"
            );
        }
    }

    /// The value line answers to its ref: a new state is one `show` away, and
    /// every state has a line of its own.
    #[test]
    fn the_balance_line_follows_the_state() {
        let mut app = BadgeApp::new(default_theme(Mode::Dark));
        assert_eq!(
            app.balance_text(),
            Some(State::Idle.line(BALANCE_PREFIX).as_str())
        );

        app.show(TITLE, BALANCE_PREFIX, &State::Loading);
        assert_eq!(
            app.balance_text(),
            Some(State::Loading.line(BALANCE_PREFIX).as_str())
        );

        let ready = State::from_result(&Ok(reply()));
        app.show(TITLE, BALANCE_PREFIX, &ready);
        assert_eq!(app.balance_text(), Some("余额：¥110.00"));

        let failed = State::from_result(&Err("连接超时".to_string()));
        app.show(TITLE, BALANCE_PREFIX, &failed);
        assert_eq!(
            app.balance_text(),
            Some(State::Failed.line(BALANCE_PREFIX).as_str())
        );
    }

    /// The Go tab reuses the same handshake with its own title and prefix, so
    /// the second window follows whichever page is active.
    #[test]
    fn the_badge_can_show_the_go_tab() {
        let mut app = BadgeApp::new(default_theme(Mode::Dark));
        app.show(GO_TITLE, GO_PREFIX, &State::Ready("Go 88%".to_string()));
        assert_eq!(app.title_text(), Some(GO_TITLE));
        assert_eq!(app.balance_text(), Some("余量：Go 88%"));
    }

    /// The point of the handshake: a result the host applied reaches the pixels.
    /// A re-paint carries the new line, not the placeholder.
    #[test]
    fn an_applied_result_reaches_the_frame() {
        let mut app = BadgeApp::new(default_theme(Mode::Dark));
        let ready = State::from_result(&Ok(reply()));
        app.show(TITLE, BALANCE_PREFIX, &ready);
        assert_eq!(lines(&frame_of(&mut app))[1].0, "余额：¥110.00");
    }

    /// The composition itself: one column with two label nodes under it. That is
    /// the point of building from components — the structure is inspectable
    /// instead of being baked into a hand-issued command sequence.
    #[test]
    fn the_tree_is_one_column_with_two_labels() {
        let app = BadgeApp::new(default_theme(Mode::Dark));
        let tree = app.tree();
        let ids: Vec<_> = tree.iter_visible().collect();
        // The scene tree's own root, our root flex, the column, the two lines.
        assert_eq!(ids.len(), 5);

        let labels: Vec<String> = ids
            .iter()
            .filter_map(|id| match draw_ui::widget(tree, *id) {
                Some(draw_ui::Widget::Label { text, .. }) => Some(text.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(
            labels,
            vec![TITLE.to_string(), State::Idle.line(BALANCE_PREFIX)]
        );
    }

    /// Dark is a token swap, not a second code path: same tree, same commands.
    #[test]
    fn both_themes_paint_the_same_shape() {
        let dark = frame(default_theme(Mode::Dark));
        let light = frame(default_theme(Mode::Light));
        assert_eq!(dark.len(), light.len());
        assert_eq!(lines(&dark), lines(&light));
    }
}
