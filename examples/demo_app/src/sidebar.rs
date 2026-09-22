//! The gallery sidebar: the app header, one row per group and the theme toggle.

use std::rc::Rc;

use draw_components::{Button, Column, Component, Flex, NodeRef, Panel, Row, Text};
use draw_core::{Color, Edges};
use draw_theme::{radius, space, Mode, Theme, Tone};
use draw_ui::{Align, SizeBasis, SurfaceStyle};

use crate::catalog;
use crate::GalleryState;

/// Sidebar width in logical pixels.
pub const SIDEBAR_WIDTH: f32 = 248.0;
/// Top padding before any title-bar safe area is added.
pub const SIDEBAR_PADDING_TOP: f32 = space::MD;

/// Builds the sidebar. It writes the sidebar root to `sidebar_slot` and the
/// theme-toggle button to `primary_slot` (the tracked "primary" button hosts
/// use for pointer probes).
pub fn build(theme: &'static dyn Theme, state: &GalleryState, primary_slot: &NodeRef) -> Column {
    let header = Row::new()
        .align(Align::Center)
        .gap(space::SM)
        .child(app_icon(theme, 22.0))
        .child(Text::subheading("Component Gallery", theme));

    let groups = Text::caption("Groups", theme).tone(Tone::Subtle);

    let rows = catalog::GROUPS
        .iter()
        .enumerate()
        .map(|(index, group)| group_row(theme, group.name, index, &state.group));

    let next = match theme.mode() {
        Mode::Dark => Mode::Light,
        Mode::Light => Mode::Dark,
    };
    let label = if theme.is_dark() {
        "Switch to light"
    } else {
        "Switch to dark"
    };
    let toggle = {
        let state = state.clone();
        Button::primary(label, theme)
            .on_click(move || {
                state.clicks.set(state.clicks.get() + 1);
                state.theme_request.set(Some(next));
            })
            .ref_(primary_slot)
    };

    Column::new()
        .gap(space::MD)
        .padding(Edges::new(
            space::LG,
            SIDEBAR_PADDING_TOP,
            space::MD,
            space::MD,
        ))
        .basis(SizeBasis::Px(SIDEBAR_WIDTH))
        .shrink(0.0)
        .surface(SurfaceStyle::new(theme.palette().surface))
        .child(header)
        .child(groups)
        .children(rows)
        .child(Flex::new().padding(Edges::ZERO).grow(1.0))
        .child(toggle)
        .child(Text::caption("draw_components · v0.1.0", theme).tone(Tone::Subtle))
}

/// One selectable group row (selection + hover share the nav surface).
fn group_row(
    theme: &'static dyn Theme,
    label: &'static str,
    index: usize,
    selected: &Rc<std::cell::Cell<usize>>,
) -> Row {
    let held = selected.clone();
    let click = selected.clone();
    Row::new()
        .align(Align::Center)
        .gap(space::SM)
        .padding(Edges::new(space::SM, space::XS, space::SM, space::XS))
        .min_size(0.0, 32.0)
        .child(Text::small(label, theme))
        .dynamic_background(move |interact| {
            let fill = if held.get() == index {
                theme.palette().selection
            } else if interact.hovered {
                theme.palette().surface_hover
            } else {
                Color::TRANSPARENT
            };
            SurfaceStyle::new(fill).radius(radius::SM)
        })
        .on_click(move || click.set(index))
}

/// The small app mark: an accent square with a light inner notch.
fn app_icon(theme: &'static dyn Theme, size: f32) -> Panel {
    Panel::new()
        .color(Color::TRANSPARENT)
        .flat()
        .min_size(size, size)
        .surface(SurfaceStyle::new(theme.palette().accent).radius(radius::SM))
        .foreground(move |ctx, rect, _| {
            draw_ui::fill_rounded_rect(
                ctx,
                draw_ui::inset(rect, 5.0),
                1.0,
                theme.palette().on_accent,
            );
        })
}
