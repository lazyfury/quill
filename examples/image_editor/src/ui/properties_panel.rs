//! 属性面板：显示当前图层的名字 / 不透明度 / 混合模式。
//!
//! 只读。可编辑的控件（滑杆、下拉）会随更丰富的组件一起加，这里先把当前
//! 图层的真实状态摆出来。

use draw_components::{Component, Divider, NodeRef, Text};
use draw_theme::{space, Theme, Tone};

use crate::ui::card::Card;

/// 属性面板。三个文本槽位由 [`EditorView`](crate::ui::EditorView) 回写。
pub fn properties_panel(
    theme: Theme,
    name: &NodeRef,
    detail: &NodeRef,
    geometry: &NodeRef,
) -> impl Component {
    Card::new(theme)
        .gap(space::SM)
        .child(Text::subheading("属性", theme))
        .child(Divider::horizontal(theme))
        .child(Text::small("", theme).ref_(name))
        .child(Text::caption("", theme).tone(Tone::Muted).ref_(detail))
        .child(Text::caption("", theme).tone(Tone::Muted).ref_(geometry))
}
