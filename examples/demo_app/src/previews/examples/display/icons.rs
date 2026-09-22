//! Built-in vector icons: the glyph set, their sizes and tones, and some
//! composed usages.

use draw_components::{Card, Column, Component, Glyph, Grid, Icon, Row, Text};
use draw_core::Edges;
use draw_theme::{radius, space, Theme, Tone};
use draw_ui::{Align, SurfaceStyle, Track};

use super::super::Ctx;

/// Every built-in glyph with a caption.
const GLYPHS: &[(&str, Glyph)] = &[
    ("Check", Glyph::Check),
    ("Cross", Glyph::Cross),
    ("Dash", Glyph::Dash),
    ("Minus", Glyph::Minus),
    ("Plus", Glyph::Plus),
    ("ChevronDown", Glyph::ChevronDown),
    ("ChevronUp", Glyph::ChevronUp),
    ("ChevronLeft", Glyph::ChevronLeft),
    ("ChevronRight", Glyph::ChevronRight),
    ("Warning", Glyph::Warning),
    ("Info", Glyph::Info),
    ("Search", Glyph::Search),
    ("Dot", Glyph::Dot),
    ("Grid", Glyph::Grid),
    ("List", Glyph::List),
    ("TextLines", Glyph::TextLines),
    ("Square", Glyph::Square),
    ("Toggle", Glyph::Toggle),
];

pub(crate) fn set(card: Card, ctx: &mut Ctx) -> Card {
    let theme = ctx.theme;
    let mut grid = Grid::new(vec![Track::Fr(1.0), Track::Fr(1.0), Track::Fr(1.0)]).gap(space::SM);
    for (name, glyph) in GLYPHS {
        grid = grid.child(
            Row::new()
                .align(Align::Center)
                .gap(space::XS)
                .child(Icon::new(*glyph, theme))
                .child(Text::caption(*name, theme).tone(Tone::Muted)),
        );
    }
    card.child(grid)
}

pub(crate) fn sizes(card: Card, ctx: &mut Ctx) -> Card {
    let theme = ctx.theme;
    card.child(
        Row::new()
            .align(Align::End)
            .gap(space::MD)
            .child(Icon::new(Glyph::Search, theme).size(12.0))
            .child(Icon::new(Glyph::Search, theme).size(16.0))
            .child(Icon::new(Glyph::Search, theme).size(20.0))
            .child(Icon::new(Glyph::Search, theme).size(28.0)),
    )
}

pub(crate) fn tones(card: Card, ctx: &mut Ctx) -> Card {
    let theme = ctx.theme;
    card.child(
        Column::new()
            .gap(space::XS)
            .child(tone_row(theme, Glyph::Check, Tone::Success, "Success"))
            .child(tone_row(theme, Glyph::Cross, Tone::Error, "Error"))
            .child(tone_row(theme, Glyph::Warning, Tone::Warning, "Warning"))
            .child(tone_row(theme, Glyph::Info, Tone::Info, "Info"))
            .child(tone_row(theme, Glyph::Plus, Tone::Accent, "Accent"))
            .child(tone_row(theme, Glyph::Dot, Tone::Muted, "Muted")),
    )
}

pub(crate) fn context(card: Card, ctx: &mut Ctx) -> Card {
    let theme = ctx.theme;
    card.child(
        Column::new()
            .gap(space::SM)
            .child(
                Row::new()
                    .align(Align::Center)
                    .gap(space::SM)
                    .padding(Edges::all(space::SM))
                    .surface(
                        SurfaceStyle::new(theme.palette().warning.with_alpha(0.16))
                            .radius(radius::SM),
                    )
                    .child(Icon::new(Glyph::Warning, theme).tone(Tone::Warning))
                    .child(Text::small("Disk almost full", theme)),
            )
            .child(
                Row::new()
                    .align(Align::Center)
                    .gap(space::SM)
                    .child(Icon::new(Glyph::Check, theme).tone(Tone::Success))
                    .child(Text::small("All changes saved", theme)),
            )
            .child(
                Row::new()
                    .align(Align::Center)
                    .gap(space::SM)
                    .child(Icon::new(Glyph::Search, theme).tone(Tone::Muted))
                    .child(Text::small("Search notes…", theme).tone(Tone::Muted)),
            ),
    )
}

fn tone_row(theme: &'static dyn Theme, glyph: Glyph, tone: Tone, name: &'static str) -> Row {
    Row::new()
        .align(Align::Center)
        .gap(space::SM)
        .child(Icon::new(glyph, theme).tone(tone))
        .child(Text::caption(name, theme).tone(Tone::Muted))
}
