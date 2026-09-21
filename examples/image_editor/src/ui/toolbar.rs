//! 工具栏：切换激活工具 + 撤销 / 重做。
//!
//! 每个入口是「图标按钮 + 小字标签」：按钮只负责点击 / 高亮（`dynamic_background`），
//! 图标由 `EditorView` 在树建好后用 `crate::icons` 描边上去（`draw_svg`，见
//! `assets/icons/`）。这样切换工具不需要重建这棵树，只有高亮的按钮换颜色。

use std::cell::RefCell;
use std::rc::Rc;

use draw_components::{Button, Component, Divider, Flex, NodeRef, Text};
use draw_core::{Color, Edges};
use draw_theme::{radius, space, SurfaceLevel, Theme};
use draw_ui::{Align, MouseFilter, SizeBasis, SurfaceStyle};

use crate::app::state::{ActiveTool, AppState, HistoryAction};
use crate::icons::{Icon, IconSet, TOOLBAR_ICON};
use crate::ui::TOOLBAR_WIDTH;

/// 竖直工具栏。`slots` 收集每个工具按钮的节点，`history_slots` 收集撤销 /
/// 重做按钮的节点；测试与自检靠它们模拟点击（点击目标是按钮，不是标签）。
pub fn tool_bar(
    theme: Theme,
    state: Rc<RefCell<AppState>>,
    message: Rc<RefCell<Option<String>>>,
    icons: Rc<IconSet>,
    slots: &mut Vec<(ActiveTool, NodeRef)>,
    history_slots: &mut Vec<(HistoryAction, NodeRef)>,
) -> impl Component {
    let mut bar = Flex::column()
        .gap(space::XXXS)
        .padding(Edges::symmetric(space::XXS, space::XXS))
        .basis(SizeBasis::Px(TOOLBAR_WIDTH))
        .shrink(0.0)
        .background(theme.surface(SurfaceLevel::Surface))
        .mouse_filter(MouseFilter::Ignore);
    for tool in ActiveTool::ALL {
        let slot = NodeRef::new();
        slots.push((tool, slot.clone()));
        bar = bar.child(tool_button(
            theme,
            tool,
            state.clone(),
            icons.clone(),
            &slot,
        ));
    }
    bar = bar.child(Divider::horizontal(theme));
    for action in HistoryAction::ALL {
        let slot = NodeRef::new();
        history_slots.push((action, slot.clone()));
        bar = bar.child(history_button(
            theme,
            action,
            state.clone(),
            message.clone(),
            icons.clone(),
            &slot,
        ));
    }
    bar
}

/// 一个工具按钮：图标 + 点击写入激活工具；激活时高亮，悬停时给一点反馈。
fn tool_button(
    theme: Theme,
    tool: ActiveTool,
    state: Rc<RefCell<AppState>>,
    icons: Rc<IconSet>,
    slot: &NodeRef,
) -> impl Component {
    let click_state = state.clone();
    let button = Button::ghost("", theme)
        .min_size(32.0, 28.0)
        .child(
            Icon::new(icons, tool.icon(), theme.palette.foreground, TOOLBAR_ICON)
                .min_size(12.0, 12.0),
        )
        .on_click(move || {
            click_state.borrow_mut().active_tool = tool;
        })
        .dynamic_background(move |interact| {
            if state.borrow().active_tool == tool {
                SurfaceStyle::new(theme.palette.selection).radius(radius::SM)
            } else if interact.hovered || interact.pressed {
                SurfaceStyle::new(theme.palette.surface_hover).radius(radius::SM)
            } else {
                SurfaceStyle::new(Color::TRANSPARENT)
            }
        })
        .ref_(slot);
    labeled(theme, tool.short_label(), button)
}

/// 撤销 / 重做按钮：图标 + 写共享状态里的 `History`，并把结果写到状态栏提示。
fn history_button(
    theme: Theme,
    action: HistoryAction,
    state: Rc<RefCell<AppState>>,
    message: Rc<RefCell<Option<String>>>,
    icons: Rc<IconSet>,
    slot: &NodeRef,
) -> impl Component {
    let button = Button::ghost("", theme)
        .min_size(32.0, 28.0)
        .child(Icon::new(
            icons,
            action.icon(),
            theme.palette.foreground,
            TOOLBAR_ICON,
        ))
        .on_click(move || {
            let result = {
                let mut state = state.borrow_mut();
                match action {
                    HistoryAction::Undo => state.undo(),
                    HistoryAction::Redo => state.redo(),
                }
            };
            *message.borrow_mut() = Some(match result {
                Some(label) => format!("{}：{label}", action.label()),
                None => format!("没有可{}的操作", action.label()),
            });
        })
        .ref_(slot);
    labeled(theme, action.label(), button)
}

/// 把「图标按钮 + 小字标签」竖着摞起来。
///
/// `Flex` 默认带 16px 内边距（和一个 8px gap），这里必须清零，否则每个入口
/// 会比内容高 32px，工具栏会显得很空。
fn labeled(theme: Theme, caption: &str, button: impl Component + 'static) -> impl Component {
    Flex::column()
        .gap(0.0)
        .padding(Edges::ZERO)
        .align(Align::Center)
        .mouse_filter(MouseFilter::Ignore)
        .child(button)
        .child(Text::caption(caption, theme))
}
