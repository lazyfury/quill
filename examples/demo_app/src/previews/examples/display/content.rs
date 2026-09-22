//! Surface and content previews: cards, dividers, badges, code, terminal and
//! empty states.

use draw_components::{
    Badge, Card, CodeBlock, Column, Component, Divider, EmptyState, Row, Terminal, Text,
};
use draw_core::Edges;
use draw_theme::{space, Tone};

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
