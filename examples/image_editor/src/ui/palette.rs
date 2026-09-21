//! 调色盘面板：当前前景 / 背景色 + 预设色块。
//!
//! 是工具栏左边的一个独立竖直面板（有自己的宽度，可以拖动）；点预设色块把颜色
//! 写进 [`AppState::foreground`](crate::app::state::AppState)，前景 / 背景可一键
//! 交换。色块与当前色都用 `dynamic_background` 每帧从共享状态读，所以吸管取色
//! 后面板会跟着变，不需要重建树。
//!
//! 文档色是 8 位 RGBA（[`crate::document::Color`]），UI 的 `SurfaceStyle` 要
//! 浮点 RGBA（`draw_core::Color`）；转换只在这一层做。

use std::cell::RefCell;
use std::rc::Rc;

use draw_components::{Button, Component, Flex, NodeRef, Text};
use draw_core::{Color, Edges};
use draw_theme::{radius, space, SurfaceLevel, Theme};
use draw_ui::{Align, MouseFilter, SurfaceStyle};

use crate::app::state::AppState;
use crate::document::Color as DocColor;

/// 预设颜色（RGB 0..255）：黑白灰 + 一组常用色。
pub const SWATCHES: [(u8, u8, u8); 16] = [
    (0, 0, 0),
    (255, 255, 255),
    (68, 68, 68),
    (136, 136, 136),
    (255, 0, 0),
    (255, 128, 0),
    (255, 255, 0),
    (0, 255, 0),
    (0, 255, 255),
    (0, 128, 255),
    (0, 0, 255),
    (128, 0, 255),
    (255, 0, 255),
    (255, 128, 128),
    (128, 64, 0),
    (0, 128, 0),
];

/// 文档色（8 位）-> UI 色（0..1）。
fn to_ui(color: DocColor) -> Color {
    Color::new(
        color.r as f32 / 255.0,
        color.g as f32 / 255.0,
        color.b as f32 / 255.0,
        color.a as f32 / 255.0,
    )
}

/// 竖直调色盘面板：标题 + 当前色 + 预设色块网格。
///
/// 面板宽度由外层 flex 的 `basis` 决定，网格按宽度自动换行（`slots` 收集每个
/// 色块节点，测试 / 自检靠它模拟点击）。
pub fn palette_panel(
    theme: Theme,
    state: Rc<RefCell<AppState>>,
    slots: &mut Vec<NodeRef>,
) -> impl Component {
    let mut grid = Flex::row()
        .wrap(true)
        .gap(space::XXS)
        .padding(Edges::ZERO)
        .mouse_filter(MouseFilter::Ignore);
    for (r, g, b) in SWATCHES {
        let slot = NodeRef::new();
        slots.push(slot.clone());
        grid = grid.child(swatch(theme, DocColor::rgb(r, g, b), state.clone()).ref_(&slot));
    }

    Flex::column()
        .gap(space::SM)
        .padding(Edges::all(space::SM))
        .background(theme.surface(SurfaceLevel::Surface))
        .mouse_filter(MouseFilter::Ignore)
        .child(Text::subheading("颜色", theme))
        .child(current_colors(theme, state))
        .child(grid)
}

/// 当前前景 / 背景的两个色块 + 交换按钮。
fn current_colors(theme: Theme, state: Rc<RefCell<AppState>>) -> impl Component {
    let swap = {
        let state = state.clone();
        Button::ghost("⇄", theme)
            .min_size(18.0, 18.0)
            .on_click(move || {
                let mut state = state.borrow_mut();
                let AppState {
                    foreground,
                    background,
                    ..
                } = &mut *state;
                std::mem::swap(foreground, background);
            })
    };
    Flex::row()
        .gap(space::XXS)
        .padding(Edges::ZERO)
        .align(Align::Center)
        .mouse_filter(MouseFilter::Ignore)
        .child(current_swatch(theme, state.clone(), true))
        .child(current_swatch(theme, state, false))
        .child(swap)
}

/// 显示当前前景或背景色的色块（只读，随状态刷新）。
fn current_swatch(theme: Theme, state: Rc<RefCell<AppState>>, foreground: bool) -> impl Component {
    Flex::new()
        .padding(Edges::ZERO)
        .min_size(20.0, 20.0)
        .mouse_filter(MouseFilter::Ignore)
        .dynamic_background(move |_| {
            let color = {
                let state = state.borrow();
                if foreground {
                    state.foreground
                } else {
                    state.background
                }
            };
            SurfaceStyle::new(to_ui(color))
                .radius(radius::SM)
                .border(theme.palette.border)
        })
}

/// 一个预设色块：点击设为前景色，悬停时描一圈边。
fn swatch(theme: Theme, color: DocColor, state: Rc<RefCell<AppState>>) -> impl Component {
    let ui_color = to_ui(color);
    Flex::new()
        .padding(Edges::ZERO)
        .min_size(18.0, 18.0)
        .on_click(move || {
            state.borrow_mut().foreground = color;
        })
        .dynamic_background(move |interact| {
            let style = SurfaceStyle::new(ui_color).radius(2.0);
            if interact.hovered || interact.pressed {
                style.border(theme.palette.foreground)
            } else {
                style
            }
        })
}
