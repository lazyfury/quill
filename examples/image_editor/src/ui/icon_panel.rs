//! 图标面板：把加载到的 Lucide 图标按网格画出来（见 `crate::icons`）。
//!
//! 面板本身只是一个固定高度的容器；真正的图标由 [`EditorView`] 在树建好后
//! 用 `IconSet::attach_grid` 挂一个 foreground decorator 描边上去。
//!
//! [`EditorView`]: crate::ui::EditorView

use draw_components::{Card, Component, Divider, Flex, NodeRef, Text};
use draw_core::Edges;
use draw_theme::{space, Theme};
use draw_ui::{MouseFilter, SizeBasis};

/// 画布高度：4 行 × 34px 的格子（`crate::icons` 的 `CELL`）。
pub const ICON_GALLERY_HEIGHT: f32 = 4.0 * 34.0;

/// 图标面板。`surface` 是画图标的那个容器的节点槽位。
pub fn icon_panel(theme: Theme, surface: &NodeRef) -> impl Component {
    Card::new(theme)
        .gap(space::SM)
        .child(Text::subheading("图标", theme))
        .child(Divider::horizontal(theme))
        .child(
            Flex::column()
                .basis(SizeBasis::Px(ICON_GALLERY_HEIGHT))
                .shrink(0.0)
                // `Flex` 默认 16px 内边距；图标网格自己算格子，这里清零。
                .padding(Edges::ZERO)
                .mouse_filter(MouseFilter::Ignore)
                .ref_(surface),
        )
}
