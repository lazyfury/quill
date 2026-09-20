//! 多窗口测试用的第二个窗口：一张无边框、无装饰的小卡片，贴在桌面左下角。
//!
//! 视图**用组件搭**（[`Card`] + [`Text`]），不再手发绘制命令：和主视图同一套
//! `draw_ui::layout` / `draw_ui::paint`，只是这棵树小到只有一张卡片两行字。
//! 圆角、内边距、字号、颜色全部走 `draw_theme` 的 token（AGENTS.md 硬规则 8：
//! 不在组件里硬编码色值）。
//!
//! 与主窗口/面板一样，它不碰窗口、不碰网络 —— 自己的 window / surface /
//! backend 由宿主 `host.rs` 提供；拿到后端的真实字体度量后，布局引擎负责排版。

use std::rc::Rc;

use draw_components::{Card, Component, Flex, Text};
use draw_core::{Edges, ViewportSize};
use draw_render::PaintContext;
use draw_scene::{SceneChild, SceneTree};
use draw_theme::{radius, space, Theme, Tone};
use draw_ui::{self, MouseFilter, TextMeasurer};

/// 徽章窗口的逻辑尺寸。
pub const BADGE_WIDTH: f32 = 190.0;
pub const BADGE_HEIGHT: f32 = 84.0;

/// 卡片圆角：方角（桌面小卡片不需要圆角）。
pub const CARD_RADIUS: f32 = radius::NONE;
/// 卡片内边距。
pub const CARD_PADDING: f32 = space::LG;
/// 两行之间的间距。
pub const LINE_GAP: f32 = space::XXXS;

/// 标题行。
pub const TITLE: &str = "Deepseek";
/// 余额行 —— 多窗口测试用静态文案，不走数据层。
pub const BALANCE_LINE: &str = "余额：¥00.01";

/// 徽章视图：一棵「卡片 + 两行字」的组件树。
pub struct BadgeApp {
    tree: SceneTree,
    theme: Theme,
}

impl BadgeApp {
    /// Mounts the tree. The layout root's own children are placed by anchors and
    /// flex starts one level down, so the card doubles as the window's backdrop:
    /// it fills the surface, and whatever the card does not cover is the
    /// window's transparent clear colour.
    pub fn new(theme: Theme) -> Self {
        let card = Card::new(theme)
            .radius(CARD_RADIUS)
            .padding(Edges::all(CARD_PADDING))
            .gap(LINE_GAP)
            .mouse_filter(MouseFilter::Ignore)
            .child(Text::heading(TITLE, theme))
            .child(Text::small(BALANCE_LINE, theme).tone(Tone::Muted));
        let tree = Flex::column()
            .mouse_filter(MouseFilter::Ignore)
            .child(card)
            .into_tree();
        Self { tree, theme }
    }

    /// The theme this view was built with (a value, so it is `Copy`).
    pub fn theme(&self) -> Theme {
        self.theme
    }

    /// Read-only view of the tree, for tests.
    pub fn tree(&self) -> &SceneTree {
        &self.tree
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

    /// Paints the tree. The card's surface is part of the tree, so unlike the
    /// menu-bar panel there is no backdrop command to issue first.
    pub fn paint(&self, ctx: &mut PaintContext) {
        draw_ui::paint(&self.tree, ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use draw_core::{Size, Vec2};
    use draw_render::{DrawCommand, TextAlign};
    use draw_theme::{border::HAIRLINE, TextSize};

    /// Lays the view out for the badge window and paints one frame.
    fn frame(theme: Theme) -> Vec<DrawCommand> {
        let mut app = BadgeApp::new(theme);
        app.layout(ViewportSize::new(Size::new(BADGE_WIDTH, BADGE_HEIGHT)));
        let mut ctx = PaintContext::new();
        app.paint(&mut ctx);
        ctx.into_draw_list().commands().to_vec()
    }

    /// The card is the window's backdrop as well as its content: a border on the
    /// outer edge, a fill just inside it, then the text. Together they cover the
    /// whole surface, so nothing but the window's transparent clear colour is
    /// left showing.
    #[test]
    fn the_card_covers_the_whole_surface() {
        let commands = frame(Theme::dark());
        let (rect, corners) = commands
            .iter()
            .find_map(|command| match command {
                DrawCommand::FillRoundedRect { rect, corners, .. } => Some((*rect, *corners)),
                _ => None,
            })
            .expect("the card paints a fill");
        assert_eq!(
            rect.origin,
            Vec2::new(HAIRLINE, HAIRLINE),
            "the fill starts inside the border"
        );
        assert_eq!(
            rect.size,
            Size::new(BADGE_WIDTH - HAIRLINE * 2.0, BADGE_HEIGHT - HAIRLINE * 2.0)
        );
        assert_eq!(corners.top_left, CARD_RADIUS);

        let (border, width) = commands
            .iter()
            .find_map(|command| match command {
                DrawCommand::StrokeRoundedRect { rect, width, .. } => Some((*rect, *width)),
                _ => None,
            })
            .expect("the card paints its hairline border");
        assert_eq!(width, HAIRLINE);
        assert_eq!(
            border.origin,
            Vec2::new(HAIRLINE / 2.0, HAIRLINE / 2.0),
            "the stroke is centred on the edge"
        );
        assert_eq!(
            border.size,
            Size::new(BADGE_WIDTH - HAIRLINE, BADGE_HEIGHT - HAIRLINE),
            "border and fill together cover the surface"
        );
    }

    /// Two lines, in order, sharing the card's left padding.
    #[test]
    fn the_two_lines_stack_inside_the_card() {
        let commands = frame(Theme::dark());
        let lines: Vec<(String, Vec2, f32, TextAlign)> = commands
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
            .collect();
        assert_eq!(lines.len(), 2, "one line per text node");
        assert_eq!(lines[0].0, TITLE);
        assert_eq!(lines[1].0, BALANCE_LINE);
        assert_eq!(
            lines[0].1.x, CARD_PADDING,
            "the text starts at the card's padding"
        );
        assert_eq!(lines[1].1.x, CARD_PADDING, "both share the same inset");
        assert!(lines[0].1.y < lines[1].1.y, "the title sits above");
        assert_eq!(lines[0].2, TextSize::Heading.px());
        assert_eq!(lines[1].2, TextSize::Small.px());
        assert_eq!(lines[0].3, TextAlign::Left);
    }

    /// The content has to fit the fixed window: the badge is borderless, so
    /// anything that overflows is simply cut off. Guards the sizes above against
    /// a theme or type-scale change.
    #[test]
    fn the_content_fits_the_window() {
        let commands = frame(Theme::dark());
        let baselines: Vec<f32> = commands
            .iter()
            .filter_map(|command| match command {
                DrawCommand::DrawText { position, .. } => Some(position.y),
                _ => None,
            })
            .collect();
        let first = baselines.first().copied().expect("two lines were painted");
        let last = baselines.last().copied().expect("two lines were painted");
        assert!(
            first > CARD_PADDING,
            "the first baseline {first} must clear the card's top padding"
        );
        assert!(
            last < BADGE_HEIGHT - CARD_PADDING,
            "the last baseline {last} must clear the bottom padding"
        );
    }

    /// The composition itself: one card node with two label nodes under it. That
    /// is the point of building from components — the structure is inspectable
    /// instead of being baked into a hand-issued command sequence.
    #[test]
    fn the_tree_is_one_card_with_two_labels() {
        let app = BadgeApp::new(Theme::dark());
        let tree = app.tree();
        let ids: Vec<_> = tree.iter_visible().collect();
        // The scene tree's own root, our root flex, the card, the two lines.
        assert_eq!(ids.len(), 5);

        let labels: Vec<String> = ids
            .iter()
            .filter_map(|id| match draw_ui::widget(tree, *id) {
                Some(draw_ui::Widget::Label { text, .. }) => Some(text.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(labels, vec![TITLE.to_string(), BALANCE_LINE.to_string()]);
    }

    /// Dark is a token swap, not a second code path: same tree, same commands.
    #[test]
    fn both_themes_paint_the_same_shape() {
        let dark = frame(Theme::dark());
        let light = frame(Theme::light());
        assert_eq!(dark.len(), light.len());
        let texts = |commands: &[DrawCommand]| {
            commands
                .iter()
                .filter_map(|command| match command {
                    DrawCommand::DrawText { text, .. } => Some(text.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(texts(&dark), texts(&light));
    }
}
