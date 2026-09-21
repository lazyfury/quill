//! 状态栏：左边当前工具，中间提示，右边文档尺寸 / 缩放。

use draw_components::{Component, Flex, NodeRef, Text};
use draw_core::Edges;
use draw_theme::{space, SurfaceLevel, Theme, Tone};
use draw_ui::{Align, Justify, MouseFilter};

/// 状态栏。三个文本槽位由 [`crate::ui::EditorView`] 在构建后回写。
pub fn status_bar(
    theme: Theme,
    tool: &NodeRef,
    message: &NodeRef,
    zoom: &NodeRef,
) -> impl Component {
    Flex::row()
        .justify(Justify::SpaceBetween)
        .align(Align::Center)
        .gap(space::SM)
        .padding(Edges::symmetric(space::SM, space::XXS))
        .background(theme.surface(SurfaceLevel::Surface))
        .mouse_filter(MouseFilter::Ignore)
        .child(Text::small("", theme).ref_(tool))
        .child(Text::caption("", theme).tone(Tone::Muted).ref_(message))
        .child(Text::small("", theme).ref_(zoom))
}
