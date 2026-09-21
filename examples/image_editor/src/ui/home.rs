//! 首页（落地页）：启动时先看到的这一屏 —— 「新建窗口」和「画廊（占位）」。
//!
//! 点「新建窗口」只往共享格里写一个请求，宿主
//! （[`crate::app::application`]）取走后开一个**原生**的「新建文档」窗口
//! （[`crate::ui::NewDocumentView`]）—— 挑选尺寸 / 背景色，**不是**模态框；
//! 创建之后编辑器仍然在**主窗口**里。画廊是占位：一张 `EmptyState`。

use std::cell::Cell;
use std::rc::Rc;

use draw_components::{Button, Card, Component, EmptyState, Flex, NodeRef, Text};
use draw_core::{Edges, EventResult, InputEvent, NodeId, Vec2, ViewportSize};
use draw_render::PaintContext;
use draw_scene::{SceneChild, SceneTree};
use draw_theme::{space, SurfaceLevel, Theme, Tone};
use draw_ui::{MouseFilter, TextMeasurer};

/// 首页视图。一棵自己的 `SceneTree`，跟 [`EditorView`](crate::ui::EditorView)
/// 一样只负责排布 / 绘制 / 输入，不碰平台。
pub struct HomeView {
    tree: SceneTree,
    /// 「新建窗口」请求；宿主取走（[`take_new_window_request`]）后开新窗口。
    new_window: Rc<Cell<bool>>,
    /// 「新建窗口」按钮节点，测试靠它真的点一下。
    #[allow(dead_code)]
    new_window_button: NodeId,
}

impl HomeView {
    pub fn new(theme: Theme) -> Self {
        let new_window: Rc<Cell<bool>> = Rc::new(Cell::new(false));
        let button_ref = NodeRef::new();

        let request = new_window.clone();
        let new_window_card = Card::new(theme)
            .gap(space::SM)
            .grow(1.0)
            .child(Text::subheading("新建窗口", theme))
            .child(
                Text::small(
                    "在弹出的原生窗口里选画布尺寸和背景色，然后在主窗口里编辑。",
                    theme,
                )
                .tone(Tone::Muted),
            )
            .child(
                Button::primary("新建窗口", theme)
                    .on_click(move || request.set(true))
                    .ref_(&button_ref),
            );

        let gallery_card = Card::new(theme)
            .gap(space::SM)
            .grow(1.0)
            .child(Text::subheading("画廊", theme))
            .child(
                EmptyState::new("画廊", theme).description("占位：以后在这里展示最近打开的文档。"),
            );

        let page = Flex::column()
            .gap(space::XL)
            .padding(Edges::all(space::HUGE))
            .mouse_filter(MouseFilter::Ignore)
            .child(
                Flex::column()
                    .gap(space::XS)
                    .padding(Edges::ZERO)
                    .mouse_filter(MouseFilter::Ignore)
                    .child(Text::title("quill 图像编辑器", theme))
                    .child(
                        Text::small("用 quill 自己的 2D / UI 栈画的桌面图像编辑器。", theme)
                            .tone(Tone::Muted),
                    ),
            )
            .child(
                Flex::row()
                    .gap(space::LG)
                    .padding(Edges::ZERO)
                    .grow(1.0)
                    .align(draw_ui::Align::Start)
                    .mouse_filter(MouseFilter::Ignore)
                    .child(new_window_card)
                    .child(gallery_card),
            );

        let tree = Flex::new()
            .gap(0.0)
            .padding(Edges::ZERO)
            .background(theme.surface(SurfaceLevel::Base))
            .mouse_filter(MouseFilter::Ignore)
            .child(page)
            .into_tree();

        let new_window_button = button_ref.get().expect("新建窗口按钮已挂载");
        Self {
            tree,
            new_window,
            new_window_button,
        }
    }

    /// 用后端真实字体度量排版。
    pub fn set_text_measurer(&mut self, measurer: Rc<dyn TextMeasurer>) {
        draw_ui::set_text_measurer(&mut self.tree, measurer);
    }

    /// 这一帧有没有点过「新建窗口」；取走后清掉，宿主据此开原生窗口。
    pub fn take_new_window_request(&self) -> bool {
        self.new_window.replace(false)
    }

    pub fn event(&mut self, event: &InputEvent) -> EventResult {
        draw_ui::handle_input(&mut self.tree, event)
    }

    pub fn layout(&mut self, viewport: ViewportSize) {
        self.tree.set_viewport_size(viewport.logical_size());
        draw_ui::layout(&mut self.tree, viewport);
        self.tree.update();
    }

    pub fn paint(&self, ctx: &mut PaintContext) {
        self.tree.paint(ctx);
        draw_ui::paint(&self.tree, ctx);
    }

    /// 「新建窗口」按钮的心点（逻辑坐标）；测试模拟点击用。
    #[allow(dead_code)]
    pub fn new_window_center(&self) -> Option<Vec2> {
        draw_ui::control(&self.tree, self.new_window_button).map(|control| control.rect.center())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use draw_core::{PointerButton, Size, Vec2};

    fn viewport() -> ViewportSize {
        ViewportSize::new(Size::new(900.0, 620.0))
    }

    fn click(view: &mut HomeView, position: Vec2) {
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
    fn the_new_window_button_raises_one_request() {
        let mut view = HomeView::new(Theme::dark());
        view.layout(viewport());
        let center = view.new_window_center().expect("新建窗口按钮");
        click(&mut view, center);
        assert!(view.take_new_window_request(), "点按钮应留下一个请求");
        assert!(!view.take_new_window_request(), "请求只能被取走一次");
    }

    #[test]
    fn the_home_page_shows_both_entries() {
        let mut view = HomeView::new(Theme::dark());
        view.layout(viewport());
        let mut ctx = PaintContext::new();
        view.paint(&mut ctx);
        let texts: Vec<String> = ctx
            .into_draw_list()
            .commands()
            .iter()
            .filter_map(|command| match command {
                draw_render::DrawCommand::DrawText { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect();
        assert!(texts.iter().any(|text| text == "画廊"), "texts = {texts:?}");
        assert!(
            texts.iter().any(|text| text == "新建窗口"),
            "texts = {texts:?}"
        );
    }
}
