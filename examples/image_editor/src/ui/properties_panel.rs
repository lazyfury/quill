//! 属性面板：显示当前图层的名字 / 不透明度 / 混合模式。
//!
//! 只读。可编辑的控件（滑杆、下拉）会随更丰富的组件一起加，这里先把当前
//! 图层的真实状态摆出来。

use draw_components::{Component, Flex, NodeRef, Text};
use draw_core::Edges;
use draw_theme::{space, Theme, Tone};
use draw_ui::MouseFilter;

/// 属性标签页的内容（标题由 [`TabsView`](crate::ui::tabs::TabsView) 的标签提供）。
/// 三个文本槽位由 [`EditorView`](crate::ui::EditorView) 回写。
pub fn properties_panel(
    theme: Theme,
    name: &NodeRef,
    detail: &NodeRef,
    geometry: &NodeRef,
) -> impl Component {
    Flex::column()
        .gap(space::SM)
        .padding(Edges::ZERO)
        .mouse_filter(MouseFilter::Ignore)
        .child(Text::small("", theme).ref_(name))
        .child(Text::caption("", theme).tone(Tone::Muted).ref_(detail))
        .child(Text::caption("", theme).tone(Tone::Muted).ref_(geometry))
}
