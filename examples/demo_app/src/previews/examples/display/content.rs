//! Surface and content previews: cards, dividers, badges, code, terminal and
//! empty states.

use draw_components::{
    Badge, Card, CodeBlock, Column, Component, Divider, EmptyState, Glyph, Grid, Icon, Row,
    Terminal, Text,
};
use draw_core::Edges;
use draw_theme::{space, Tone};
use draw_ui::{Align, Track};

use super::super::Ctx;

pub(crate) fn card(card: Card, ctx: &mut Ctx) -> Card {
    let theme = ctx.theme;
    card.child(
        Card::flat(theme)
            .gap(space::XXS)
            .padding(Edges::all(space::MD))
            .child(Text::small("Nested card", theme))
            .child(
                Text::caption("A card surfaces content on a raised fill.", theme).tone(Tone::Muted),
            ),
    )
}

pub(crate) fn divider(card: Card, ctx: &mut Ctx) -> Card {
    let theme = ctx.theme;
    card.child(
        Column::new()
            .gap(space::SM)
            .child(Text::small("Above the rule", theme))
            .child(Divider::horizontal(theme))
            .child(Text::small("Below the rule", theme)),
    )
}

pub(crate) fn badge(card: Card, ctx: &mut Ctx) -> Card {
    let theme = ctx.theme;
    card.child(
        Column::new()
            .gap(space::SM)
            .child(
                Row::new()
                    .gap(space::SM)
                    .child(Badge::new("Default", theme))
                    .child(Badge::new("Accent", theme).tone(Tone::Accent))
                    .child(Badge::new("Success", theme).tone(Tone::Success)),
            )
            .child(
                Row::new()
                    .gap(space::SM)
                    .child(Badge::pill("Pill", theme))
                    .child(Badge::new("Solid", theme).solid()),
            ),
    )
}

pub(crate) fn code_block(card: Card, ctx: &mut Ctx) -> Card {
    let theme = ctx.theme;
    card.child(
        CodeBlock::new(
            "fn main() {\n    let theme = Theme::dark();\n    println!(\"{:?}\", theme);\n}",
            theme,
        )
        .filename("main.rs")
        .language("rust"),
    )
}

pub(crate) fn terminal(card: Card, ctx: &mut Ctx) -> Card {
    let theme = ctx.theme;
    card.child(
        Terminal::new(theme)
            .command("cargo test --workspace")
            .outputs(["running 128 tests", "test result: ok. 128 passed"]),
    )
}

pub(crate) fn empty_state(card: Card, ctx: &mut Ctx) -> Card {
    let theme = ctx.theme;
    card.child(EmptyState::new("No results", theme).description("Try a different search."))
}

pub(crate) fn glyphs(card: Card, ctx: &mut Ctx) -> Card {
    let theme = ctx.theme;
    let items = [
        ("Check", Glyph::Check),
        ("Cross", Glyph::Cross),
        ("Dash", Glyph::Dash),
        ("Minus", Glyph::Minus),
        ("Plus", Glyph::Plus),
        ("Chevron", Glyph::ChevronDown),
        ("Search", Glyph::Search),
        ("Warning", Glyph::Warning),
        ("Info", Glyph::Info),
    ];
    let mut grid = Grid::new(vec![Track::Fr(1.0), Track::Fr(1.0), Track::Fr(1.0)]).gap(space::SM);
    for (name, glyph) in items {
        grid = grid.child(
            Row::new()
                .align(Align::Center)
                .gap(space::XS)
                .child(Icon::new(glyph, theme))
                .child(Text::caption(name, theme).tone(Tone::Muted)),
        );
    }
    card.child(grid).child(
        Row::new()
            .align(Align::Center)
            .gap(space::SM)
            .child(
                Icon::new(Glyph::Warning, theme)
                    .tone(Tone::Warning)
                    .size(18.0),
            )
            .child(Text::small("Saved with warnings", theme)),
    )
}
