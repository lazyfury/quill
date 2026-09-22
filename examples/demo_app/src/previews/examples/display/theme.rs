//! Theme token previews: the palette, surface levels, semantic tones, density
//! metrics, the radius / spacing scales and cursor feedback.

use draw_components::{Card, Column, Component, Grid, Label, Panel, Row, Text};
use draw_core::{Color, Cursor, Edges};
use draw_theme::{radius, space, ControlSize, Space, SurfaceLevel, Theme, Tone};
use draw_ui::{Align, Track};

use super::super::Ctx;
use super::{chip, framed, rounded};

pub(crate) fn palette(card: Card, ctx: &mut Ctx) -> Card {
    let theme = ctx.theme;
    let p = theme.palette();
    let swatches = [
        ("accent", p.accent),
        ("success", p.success),
        ("warning", p.warning),
        ("error", p.error),
        ("info", p.info),
        ("selection", p.selection),
        ("focus_ring", p.focus_ring),
        ("border", p.border),
    ];
    let mut grid = Grid::new(vec![Track::Fr(1.0), Track::Fr(1.0)]).gap(space::XS);
    for (name, color) in swatches {
        grid = grid.child(chip_row(color, name, theme));
    }
    card.child(grid)
}

pub(crate) fn surface_levels(card: Card, ctx: &mut Ctx) -> Card {
    let theme = ctx.theme;
    card.child(
        Column::new()
            .gap(space::XS)
            .child(level_row("Base", theme.surface(SurfaceLevel::Base), theme))
            .child(level_row(
                "Surface",
                theme.surface(SurfaceLevel::Surface),
                theme,
            ))
            .child(level_row(
                "Raised",
                theme.surface(SurfaceLevel::Raised),
                theme,
            ))
            .child(level_row(
                "Floating",
                theme.surface(SurfaceLevel::Floating),
                theme,
            )),
    )
}

pub(crate) fn tones(card: Card, ctx: &mut Ctx) -> Card {
    let theme = ctx.theme;
    card.child(
        Column::new()
            .gap(space::XXS)
            .child(chip_row(Tone::Accent.color(theme), "Accent", theme))
            .child(chip_row(Tone::Success.color(theme), "Success", theme))
            .child(chip_row(Tone::Warning.color(theme), "Warning", theme))
            .child(chip_row(Tone::Error.color(theme), "Error", theme))
            .child(chip_row(Tone::Info.color(theme), "Info", theme))
            .child(chip_row(Tone::Muted.color(theme), "Muted", theme)),
    )
}

pub(crate) fn density(card: Card, ctx: &mut Ctx) -> Card {
    let theme = ctx.theme;
    card.child(
        Column::new()
            .gap(space::XS)
            .child(Text::small(
                format!(
                    "control_height  regular {:.0} · mini {:.0}",
                    theme.control_height(ControlSize::Regular),
                    theme.control_height(ControlSize::Mini)
                ),
                theme,
            ))
            .child(Text::small(
                format!("row_height  {:.0}", theme.row_height()),
                theme,
            ))
            .child(Text::small(
                format!(
                    "spacing  SM {:.0} · LG {:.0}",
                    theme.spacing(Space::SM),
                    theme.spacing(Space::LG)
                ),
                theme,
            ))
            .child(
                Text::caption("Density scales spacing and metrics, not colours.", theme)
                    .tone(Tone::Muted),
            ),
    )
}

pub(crate) fn radius_spacing(card: Card, ctx: &mut Ctx) -> Card {
    let theme = ctx.theme;
    let accent = theme.palette().accent;
    card.child(
        Column::new()
            .gap(space::SM)
            .child(
                Row::new()
                    .gap(space::SM)
                    .child(rounded(accent, radius::NONE))
                    .child(rounded(accent, radius::SM))
                    .child(rounded(accent, radius::MD))
                    .child(rounded(accent, radius::LG))
                    .child(rounded(accent, radius::PANEL)),
            )
            .child(
                Row::new()
                    .gap(space::XS)
                    .child(spacing_bar(theme, Space::XS))
                    .child(spacing_bar(theme, Space::SM))
                    .child(spacing_bar(theme, Space::MD))
                    .child(spacing_bar(theme, Space::LG))
                    .child(spacing_bar(theme, Space::XL)),
            )
            .child(Text::caption("radius NONE→PANEL · spacing XS→XL", theme).tone(Tone::Subtle)),
    )
}

pub(crate) fn cursor(card: Card, ctx: &mut Ctx) -> Card {
    let theme = ctx.theme;
    card.child(
        Column::new()
            .gap(space::SM)
            .child(
                Panel::new()
                    .color(theme.palette().surface_hover)
                    .flat()
                    .min_size(0.0, 44.0)
                    .cursor(Cursor::Pointer)
                    .child(
                        Label::new("Hover me — the host shows a pointer")
                            .color(theme.palette().muted)
                            .anchors(Edges::ZERO)
                            .offsets(Edges::all(space::SM)),
                    ),
            )
            .child(
                Text::caption(
                    "Hit-testing is backend-neutral; the host reads the cursor.",
                    theme,
                )
                .tone(Tone::Muted),
            ),
    )
}

fn chip_row(color: Color, name: &'static str, theme: &'static dyn Theme) -> Row {
    Row::new()
        .align(Align::Center)
        .gap(space::SM)
        .child(chip(color))
        .child(Text::caption(name, theme).tone(Tone::Muted))
}

fn level_row(name: &'static str, fill: Color, theme: &'static dyn Theme) -> Row {
    Row::new()
        .align(Align::Center)
        .gap(space::SM)
        .child(framed(fill, theme, 22.0, 22.0))
        .child(Text::caption(name, theme).tone(Tone::Muted))
}

fn spacing_bar(theme: &'static dyn Theme, step: Space) -> Panel {
    Panel::new()
        .color(theme.palette().info)
        .flat()
        .min_size(theme.spacing(step), 8.0)
}
