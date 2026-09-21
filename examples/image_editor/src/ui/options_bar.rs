//! 工具选项栏：菜单栏下面的一行，显示当前工具的配置。
//!
//! 画笔 / 橡皮显示笔刷大小与不透明度（`−` / `+` 按钮）；其余工具显示一句操作
//! 提示。按钮只写一个共享请求格（[`BrushAdjust`]），真正的改动由
//! [`EditorView::update`](crate::ui::EditorView) 统一执行 —— 回调拿不到
//! `&mut self`，和菜单 / 文件面板一个套路。

use std::cell::Cell;
use std::rc::Rc;

use draw_components::{Button, Component, Flex, NodeRef, Text};
use draw_core::{Color, Edges};
use draw_theme::{radius, space, SurfaceLevel, Theme, Tone};
use draw_ui::{Align, MouseFilter, SurfaceStyle};

use crate::app::state::ActiveTool;

/// 选项栏里的一次笔刷调整。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrushAdjust {
    SizeDown,
    SizeUp,
    OpacityDown,
    OpacityUp,
}

impl BrushAdjust {
    /// 自检 / 报告里的短名字。
    pub const fn label(self) -> &'static str {
        match self {
            Self::SizeDown => "笔刷大小 −",
            Self::SizeUp => "笔刷大小 +",
            Self::OpacityDown => "不透明度 −",
            Self::OpacityUp => "不透明度 +",
        }
    }
}

/// 选项栏里的布尔开关（像素模式 / 方形笔）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrushToggle {
    /// 像素模式：硬边（不做抗锯齿）。
    Hard,
    /// 方形笔（否则圆头）。
    Square,
}

impl BrushToggle {
    /// 按钮上的短标签 / 自检报告里的名字。
    pub const fn label(self) -> &'static str {
        match self {
            Self::Hard => "像素",
            Self::Square => "方形",
        }
    }
}

/// 选项栏里需要回写的文本 / 容器槽位。
#[derive(Default)]
pub struct OptionsRefs {
    pub tool: NodeRef,
    pub brush: NodeRef,
    pub size: NodeRef,
    pub opacity: NodeRef,
    pub hint: NodeRef,
}

/// 工具选项栏。`request` 收下 `−` / `+` 的调整请求；`buttons` 收集按钮节点，
/// 测试与自检靠它们真的点一下。
pub fn options_bar(
    theme: Theme,
    request: Rc<Cell<Option<BrushAdjust>>>,
    hard_state: Rc<Cell<bool>>,
    square_state: Rc<Cell<bool>>,
    refs: &OptionsRefs,
    buttons: &mut Vec<(BrushAdjust, NodeRef)>,
    toggles: &mut Vec<(BrushToggle, NodeRef)>,
) -> impl Component {
    let brush = Flex::row()
        .align(Align::Center)
        .gap(space::XS)
        .padding(Edges::ZERO)
        .mouse_filter(MouseFilter::Ignore)
        .child(Text::caption("大小", theme).tone(Tone::Muted))
        .child(step_button(
            theme,
            "−",
            request.clone(),
            BrushAdjust::SizeDown,
            buttons,
        ))
        .child(Text::small("", theme).ref_(&refs.size))
        .child(step_button(
            theme,
            "+",
            request.clone(),
            BrushAdjust::SizeUp,
            buttons,
        ))
        .child(Text::caption("不透明度", theme).tone(Tone::Muted))
        .child(step_button(
            theme,
            "−",
            request.clone(),
            BrushAdjust::OpacityDown,
            buttons,
        ))
        .child(Text::small("", theme).ref_(&refs.opacity))
        .child(step_button(
            theme,
            "+",
            request.clone(),
            BrushAdjust::OpacityUp,
            buttons,
        ))
        .child(toggle_button(theme, BrushToggle::Hard, hard_state, toggles))
        .child(toggle_button(
            theme,
            BrushToggle::Square,
            square_state,
            toggles,
        ));

    Flex::row()
        .align(Align::Center)
        .gap(space::MD)
        .padding(Edges::symmetric(space::SM, space::XXS))
        .background(theme.surface(SurfaceLevel::Raised))
        .mouse_filter(MouseFilter::Ignore)
        .child(Text::small("", theme).ref_(&refs.tool))
        .child(brush.ref_(&refs.brush))
        .child(Text::caption("", theme).tone(Tone::Muted).ref_(&refs.hint))
}

/// 非画笔工具的提示文字（画笔 / 橡皮为空，因为它们的配置就在左边）。
pub const fn options_hint(tool: ActiveTool) -> &'static str {
    match tool {
        ActiveTool::Move => "在画布上拖动移动当前图层",
        ActiveTool::RectangleSelect => "拖动框选 · Esc 清空",
        ActiveTool::Eyedropper => "点击画布取合成后的颜色",
        ActiveTool::Brush | ActiveTool::Eraser => "",
    }
}

/// 画笔 / 橡皮才显示笔刷配置。
pub const fn tool_has_brush(tool: ActiveTool) -> bool {
    matches!(tool, ActiveTool::Brush | ActiveTool::Eraser)
}

fn step_button(
    theme: Theme,
    label: &str,
    request: Rc<Cell<Option<BrushAdjust>>>,
    adjust: BrushAdjust,
    buttons: &mut Vec<(BrushAdjust, NodeRef)>,
) -> impl Component {
    let slot = NodeRef::new();
    buttons.push((adjust, slot.clone()));
    Button::ghost(label, theme)
        .on_click(move || request.set(Some(adjust)))
        .ref_(&slot)
}

/// 一个布尔开关：点击翻转共享状态，`dynamic_background` 直接读它显示激活态。
fn toggle_button(
    theme: Theme,
    toggle: BrushToggle,
    state: Rc<Cell<bool>>,
    toggles: &mut Vec<(BrushToggle, NodeRef)>,
) -> impl Component {
    let slot = NodeRef::new();
    toggles.push((toggle, slot.clone()));
    let click_state = state.clone();
    Button::ghost(toggle.label(), theme)
        .on_click(move || click_state.set(!click_state.get()))
        .dynamic_background(move |interact| {
            if state.get() {
                SurfaceStyle::new(theme.palette.selection).radius(radius::SM)
            } else if interact.hovered || interact.pressed {
                SurfaceStyle::new(theme.palette.surface_hover).radius(radius::SM)
            } else {
                SurfaceStyle::new(Color::TRANSPARENT)
            }
        })
        .ref_(&slot)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_brush_and_eraser_show_the_brush_config() {
        assert!(tool_has_brush(ActiveTool::Brush));
        assert!(tool_has_brush(ActiveTool::Eraser));
        assert!(!tool_has_brush(ActiveTool::Move));
        assert!(!tool_has_brush(ActiveTool::RectangleSelect));
        assert!(!tool_has_brush(ActiveTool::Eyedropper));
    }

    #[test]
    fn every_tool_has_a_hint_or_brush_config() {
        for tool in ActiveTool::ALL {
            assert_eq!(tool_has_brush(tool), options_hint(tool).is_empty());
        }
    }
}
