//! 文件面板（Phase 7）：导入 / 导出 PNG，以及一个可编辑的路径。
//!
//! winit 不带原生文件对话框，所以这里用「路径标签 + 改路径」这种最小交互：
//! 路径存在共享格里，`EditorView` 的内联编辑把它改掉再回写标签；导入 / 导出
//! 按钮只设置一个 [`IoAction`] 请求，真正的文件 IO 由
//! [`EditorView::update`](crate::ui::EditorView) 统一执行（回调拿不到 `&mut self`）。

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use draw_components::{Button, Component, Flex, NodeRef, Text};
use draw_core::Edges;
use draw_theme::{space, TextSize, Theme, Tone};
use draw_ui::MouseFilter;

/// 文件面板上的动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IoAction {
    /// 进入路径内联编辑。
    EditPath,
    /// 从路径导入一张 PNG 为新图层。
    Import,
    /// 把当前文档合成后导出到路径。
    Export,
}

impl IoAction {
    /// 按钮标签。
    pub const fn label(self) -> &'static str {
        match self {
            Self::EditPath => "改路径",
            Self::Import => "导入 PNG",
            Self::Export => "导出 PNG",
        }
    }
}

/// 文件标签页的内容（标题由 [`TabsView`](crate::ui::tabs::TabsView) 的标签提供）。
/// `buttons` 收集三个按钮的节点，测试与自检靠它们真的点一下。
pub fn file_panel(
    theme: Theme,
    path: Rc<RefCell<String>>,
    request: Rc<Cell<Option<IoAction>>>,
    path_label: &NodeRef,
    buttons: &mut Vec<(IoAction, NodeRef)>,
) -> impl Component {
    let mut row = Flex::row()
        .wrap(true)
        .gap(space::XXS)
        .padding(Edges::ZERO)
        .mouse_filter(MouseFilter::Ignore);
    for action in [IoAction::EditPath, IoAction::Import, IoAction::Export] {
        let slot = NodeRef::new();
        buttons.push((action, slot.clone()));
        row = row.child(action_button(theme, action, request.clone(), &slot));
    }
    let initial_path = path.borrow().clone();

    Flex::column()
        .gap(space::SM)
        .padding(Edges::ZERO)
        .mouse_filter(MouseFilter::Ignore)
        .child(Text::caption("路径（点「改路径」编辑）", theme).tone(Tone::Muted))
        .child(
            Text::caption(initial_path, theme)
                .max_lines(2)
                .ellipsis(true)
                .ref_(path_label),
        )
        .child(row)
}

/// 一个动作按钮：点击只设置请求，`EditorView::update` 执行。
fn action_button(
    theme: Theme,
    action: IoAction,
    request: Rc<Cell<Option<IoAction>>>,
    slot: &NodeRef,
) -> impl Component {
    let request = request.clone();
    Button::secondary(action.label(), theme)
        .font_size(TextSize::Small.px())
        .on_click(move || request.set(Some(action)))
        .ref_(slot)
}
