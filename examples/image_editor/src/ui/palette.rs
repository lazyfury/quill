//! 调色盘面板：取色器（HSV）+ 当前前景 / 背景色 + 预设色块。
//!
//! 是工具栏右边的一个独立竖直面板（有自己的宽度，可以拖动）。取色器用
//! `on_pointer`（press + move 给绝对位置）把指针映射到自己的矩形上：上面是
//! 饱和度 / 明度方块，下面是色相条，拖动即改 `AppState.foreground`。下面还有
//! 16 个预设色块，点一下直接选色。
//!
//! 文档色是 8 位 RGBA（[`crate::document::Color`]），UI 的 `SurfaceStyle` 要
//! 浮点 RGBA（`draw_core::Color`）；转换只在这一层做。取色器没有渐变图元，用
//! 一小片实心色块拼出方块 / 色相条（分辨率见 `SV_CELLS` / `HUE_CELLS`）。

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use draw_components::{Button, Component, Flex, NodeRef, Text};
use draw_core::{Color, Edges, Rect, Size, Vec2};
use draw_render::PaintContext;
use draw_theme::{radius, space, Theme};
use draw_ui::{Align, MouseFilter, SurfaceStyle};

use crate::app::state::AppState;
use crate::document::Color as DocColor;
use crate::ui::card::Card;

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

/// 饱和度 / 明度方块与色相条的实心块分辨率（没有渐变图元的替代品）。
const SV_CELLS: u32 = 12;
const HUE_CELLS: u32 = 24;
/// 取色器方块 / 色相条高度（逻辑像素）。
const SV_SIZE: f32 = 72.0;
const HUE_HEIGHT: f32 = 14.0;

/// 文档色（8 位）-> UI 色（0..1）。
fn to_ui(color: DocColor) -> Color {
    Color::new(
        color.r as f32 / 255.0,
        color.g as f32 / 255.0,
        color.b as f32 / 255.0,
        color.a as f32 / 255.0,
    )
}

/// HSV（h 0..1, s/v 0..1）-> 文档色。
fn hsv_to_rgb(h: f32, s: f32, v: f32) -> DocColor {
    let h = h.rem_euclid(1.0) * 6.0;
    let i = h.floor() as i32 % 6;
    let f = h - h.floor();
    let p = v * (1.0 - s);
    let q = v * (1.0 - s * f);
    let t = v * (1.0 - s * (1.0 - f));
    let (r, g, b) = match i {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    let byte = |x: f32| (x.clamp(0.0, 1.0) * 255.0).round() as u8;
    DocColor::rgb(byte(r), byte(g), byte(b))
}

fn norm(value: f32, extent: f32) -> f32 {
    if extent <= 0.0 {
        0.0
    } else {
        (value / extent).clamp(0.0, 1.0)
    }
}

/// 竖直调色盘面板：标题 + 取色器 + 当前色 + 预设色块网格。
///
/// 面板宽度由外层 flex 的 `basis` 决定，网格按宽度自动换行（`slots` 收集每个
/// 色块节点，测试 / 自检靠它模拟点击）。
pub fn palette_panel(
    theme: Theme,
    state: Rc<RefCell<AppState>>,
    slots: &mut Vec<NodeRef>,
    picker_ref: &NodeRef,
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

    let hsv: PickerHsv = Rc::new(Cell::new((0.0, 1.0, 1.0)));

    Card::new(theme)
        .grow(1.0)
        .child(Text::subheading("颜色", theme))
        .child(picker(theme, state.clone(), hsv, picker_ref))
        .child(current_colors(theme, state))
        .child(grid)
}

/// 取色器的 HSV 状态（h/s/v 都是 0..1）。
type PickerHsv = Rc<Cell<(f32, f32, f32)>>;

/// 取色器：饱和/明度方块 + 色相条。
fn picker(
    theme: Theme,
    app: Rc<RefCell<AppState>>,
    hsv: PickerHsv,
    picker_ref: &NodeRef,
) -> impl Component {
    Flex::column()
        .gap(space::XXS)
        .padding(Edges::ZERO)
        .mouse_filter(MouseFilter::Ignore)
        .child(sv_area(theme, app.clone(), hsv.clone()).ref_(picker_ref))
        .child(hue_strip(theme, app, hsv))
}

fn sv_area(theme: Theme, app: Rc<RefCell<AppState>>, hsv: PickerHsv) -> impl Component {
    let pointer = hsv.clone();
    let draw = hsv.clone();
    Flex::new()
        .min_size(SV_SIZE, SV_SIZE)
        .on_pointer(move |rect, at| {
            let s = norm(at.x - rect.left(), rect.size.width);
            let v = 1.0 - norm(at.y - rect.top(), rect.size.height);
            let h = pointer.get().0;
            pointer.set((h, s, v));
            app.borrow_mut().foreground = hsv_to_rgb(h, s, v);
        })
        .foreground(move |ctx, rect, _| {
            let (h, s, v) = draw.get();
            draw_sv_square(ctx, rect, h);
            draw_cursor(ctx, rect, s, 1.0 - v, theme);
        })
}

fn hue_strip(theme: Theme, app: Rc<RefCell<AppState>>, hsv: PickerHsv) -> impl Component {
    let pointer = hsv.clone();
    let draw = hsv.clone();
    Flex::new()
        .min_size(SV_SIZE, HUE_HEIGHT)
        .on_pointer(move |rect, at| {
            let h = norm(at.x - rect.left(), rect.size.width);
            let (_, s, v) = pointer.get();
            pointer.set((h, s, v));
            app.borrow_mut().foreground = hsv_to_rgb(h, s, v);
        })
        .foreground(move |ctx, rect, _| {
            let (h, _, _) = draw.get();
            draw_hue_strip(ctx, rect);
            draw_cursor(ctx, rect, h, 0.5, theme);
        })
}

/// 用实心块拼出饱和 / 明度方块（左上白 -> 右上纯色 -> 下黑）。
fn draw_sv_square(ctx: &mut PaintContext, rect: Rect, h: f32) {
    let cells = SV_CELLS as f32;
    let (cw, ch) = (rect.size.width / cells, rect.size.height / cells);
    for row in 0..SV_CELLS {
        for col in 0..SV_CELLS {
            let s = (col as f32 + 0.5) / cells;
            let v = 1.0 - (row as f32 + 0.5) / cells;
            // 0.5px 重叠，避免相邻块之间露出缝隙。
            let cell = Rect::from_min_size(
                Vec2::new(rect.left() + col as f32 * cw, rect.top() + row as f32 * ch),
                Size::new(cw + 0.5, ch + 0.5),
            );
            ctx.fill_rect(cell, to_ui(hsv_to_rgb(h, s, v)));
        }
    }
}

fn draw_hue_strip(ctx: &mut PaintContext, rect: Rect) {
    let cells = HUE_CELLS as f32;
    let cw = rect.size.width / cells;
    for col in 0..HUE_CELLS {
        let h = (col as f32 + 0.5) / cells;
        let cell = Rect::from_min_size(
            Vec2::new(rect.left() + col as f32 * cw, rect.top()),
            Size::new(cw + 0.5, rect.size.height),
        );
        ctx.fill_rect(cell, to_ui(hsv_to_rgb(h, 1.0, 1.0)));
    }
}

/// 在 `(x, y)`（0..1，相对 `rect`）画一个黑白双圈的取色光标。
fn draw_cursor(ctx: &mut PaintContext, rect: Rect, x: f32, y: f32, _theme: Theme) {
    let center = Vec2::new(
        rect.left() + x.clamp(0.0, 1.0) * rect.size.width,
        rect.top() + y.clamp(0.0, 1.0) * rect.size.height,
    );
    ctx.stroke_circle(center, 4.0, 2.0, Color::BLACK);
    ctx.stroke_circle(center, 4.0, 1.0, Color::WHITE);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hsv_primaries_map_to_rgb() {
        assert_eq!(hsv_to_rgb(0.0, 1.0, 1.0), DocColor::RED);
        assert_eq!(hsv_to_rgb(1.0 / 3.0, 1.0, 1.0), DocColor::rgb(0, 255, 0));
        assert_eq!(hsv_to_rgb(2.0 / 3.0, 1.0, 1.0), DocColor::rgb(0, 0, 255));
        assert_eq!(hsv_to_rgb(0.5, 0.0, 1.0), DocColor::WHITE);
        assert_eq!(hsv_to_rgb(0.5, 1.0, 0.0), DocColor::BLACK);
    }

    #[test]
    fn norm_clamps_and_handles_zero_extent() {
        assert_eq!(norm(-1.0, 10.0), 0.0);
        assert_eq!(norm(5.0, 10.0), 0.5);
        assert_eq!(norm(20.0, 10.0), 1.0);
        assert_eq!(norm(5.0, 0.0), 0.0);
    }
}
