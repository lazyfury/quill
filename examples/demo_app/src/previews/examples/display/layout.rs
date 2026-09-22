//! Layout previews: flex distribution, grid tracks, anchors, alignment and the
//! spacing scale.

use draw_components::{Card, Column, Component, Grid, Panel, Row, Text};
use draw_core::{Color, Edges};
use draw_theme::{radius, space, Theme, Tone};
use draw_ui::{Align, Justify, SurfaceStyle, Track};

use super::super::Ctx;
use super::boxx;

pub(crate) fn flex(card: Card, ctx: &mut Ctx) -> Card {
    let p = ctx.theme.palette();
    card.child(
        Column::new()
            .gap(space::SM)
            .child(
                Row::new()
                    .gap(space::SM)
                    .min_size(0.0, 28.0)
                    .child(boxx(p.accent).grow(1.0))
                    .child(boxx(p.info).grow(1.0))
                    .child(boxx(p.success).grow(1.0)),
            )
            .child(
                Row::new()
                    .gap(space::SM)
                    .min_size(0.0, 28.0)
                    .child(boxx(p.warning).grow(2.0))
                    .child(boxx(p.error).grow(1.0)),
            ),
    )
}

pub(crate) fn grow_shrink(card: Card, ctx: &mut Ctx) -> Card {
    let p = ctx.theme.palette();
    card.child(
        Column::new()
            .gap(space::SM)
            .child(
                Row::new()
                    .gap(space::SM)
                    .min_size(0.0, 24.0)
                    .child(boxx(p.accent).grow(1.0))
                    .child(boxx(p.info).grow(2.0))
                    .child(boxx(p.success).grow(3.0)),
            )
            .child(
                Row::new()
                    .gap(space::SM)
                    .min_size(0.0, 24.0)
                    .child(boxx(p.warning).min_size(80.0, 24.0).shrink(0.0))
                    .child(boxx(p.error).grow(1.0)),
            ),
    )
}

pub(crate) fn grid(card: Card, ctx: &mut Ctx) -> Card {
    let p = ctx.theme.palette();
    let colors = [p.accent, p.info, p.success, p.warning, p.error, p.subtle];
    // Px pins the first column; Fr(1)/Fr(2) split the rest.
    let mut grid = Grid::new(vec![Track::Px(72.0), Track::Fr(1.0), Track::Fr(2.0)]).gap(space::SM);
    for color in colors {
        grid = grid.child(boxx(color).min_size(0.0, 28.0));
    }
    card.child(grid)
}

pub(crate) fn anchors(card: Card, ctx: &mut Ctx) -> Card {
    let p = ctx.theme.palette();
    card.child(
        Panel::new()
            .color(Color::TRANSPARENT)
            .flat()
            .min_size(0.0, 76.0)
            .surface(
                SurfaceStyle::new(p.surface_raised)
                    .border(p.border)
                    .radius(radius::MD),
            )
            .child(
                Panel::new()
                    .color(p.accent)
                    .flat()
                    .min_size(40.0, 26.0)
                    .anchors(Edges::new(1.0, 1.0, 1.0, 1.0))
                    .offsets(Edges::new(-48.0, -34.0, -8.0, -8.0)),
            ),
    )
}

pub(crate) fn alignment(card: Card, ctx: &mut Ctx) -> Card {
    let theme = ctx.theme;
    let p = theme.palette();
    card.child(
        Column::new()
            .gap(space::XS)
            .child(rail(theme, Justify::Start, p.accent))
            .child(rail(theme, Justify::Center, p.info))
            .child(rail(theme, Justify::End, p.success))
            .child(
                Row::new()
                    .align(Align::Stretch)
                    .padding(Edges::all(space::XS))
                    .min_size(0.0, 28.0)
                    .surface(SurfaceStyle::new(p.surface_raised).radius(radius::SM))
                    .child(Panel::new().color(p.warning).flat().grow(1.0)),
            ),
    )
}

pub(crate) fn padding_gap(card: Card, ctx: &mut Ctx) -> Card {
    let theme = ctx.theme;
    card.child(
        Column::new()
            .gap(space::MD)
            .padding(Edges::all(space::LG))
            .surface(SurfaceStyle::new(theme.palette().background).radius(radius::MD))
            .child(Text::caption("padding LG · gap MD", theme).tone(Tone::Muted))
            .child(boxx(theme.palette().accent).min_size(0.0, 18.0))
            .child(boxx(theme.palette().info).min_size(0.0, 18.0)),
    )
}

/// A fixed-height rail showing one main-axis alignment of a small box.
fn rail(theme: &'static dyn Theme, justify: Justify, color: Color) -> Row {
    Row::new()
        .justify(justify)
        .align(Align::Center)
        .padding(Edges::all(space::XS))
        .min_size(0.0, 28.0)
        .surface(SurfaceStyle::new(theme.palette().surface_raised).radius(radius::SM))
        .child(Panel::new().color(color).flat().min_size(40.0, 14.0))
}
