//! Stateless preview builders, split by concern.
//!
//! Every builder takes its item's [`Card`] and appends an example. The shared
//! helpers here paint solid or framed swatches without the default [`Panel`]
//! fill (which would otherwise show through as a grid of grey boxes).

use draw_components::{Component, Panel};
use draw_core::Color;
use draw_theme::{radius, Theme};
use draw_ui::SurfaceStyle;

pub(crate) mod content;
pub(crate) mod icons;
pub(crate) mod layout;
pub(crate) mod text;
pub(crate) mod theme;

/// A small solid bar used by the layout previews.
fn boxx(color: Color) -> Panel {
    Panel::new().color(color).flat().min_size(0.0, 24.0)
}

/// A solid colour chip for palette / tone rows.
fn chip(color: Color) -> Panel {
    Panel::new().color(color).flat().min_size(14.0, 14.0)
}

/// A bordered swatch whose fill is painted by the surface decorator, so a
/// background-coloured level stays visible on the card.
fn framed(fill: Color, theme: &'static dyn Theme, width: f32, height: f32) -> Panel {
    Panel::new()
        .color(Color::TRANSPARENT)
        .flat()
        .min_size(width, height)
        .surface(
            SurfaceStyle::new(fill)
                .border(theme.palette().border)
                .radius(radius::SM),
        )
}

/// A rounded solid block (radius-scale preview).
fn rounded(color: Color, corner: f32) -> Panel {
    Panel::new()
        .color(Color::TRANSPARENT)
        .flat()
        .min_size(30.0, 30.0)
        .surface(SurfaceStyle::new(color).radius(corner))
}
