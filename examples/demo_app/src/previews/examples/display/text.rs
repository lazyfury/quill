//! Text previews: the type scale, weights, overflow/ellipsis and word breaking.

use draw_components::{Card, Column, Component, Grid, Label, Panel, Text};
use draw_core::{Color, Edges};
use draw_theme::{space, Tone};
use draw_ui::{Track, WordBreak};

use super::super::Ctx;

/// A body sample long enough to wrap in a card-width column.
const SAMPLE: &str = "Quill wraps text to the resolved width, hard-breaks overlong \
                      words when it must, and clamps to a line budget with an ellipsis.";

pub(crate) fn type_scale(card: Card, ctx: &mut Ctx) -> Card {
    let theme = ctx.theme;
    card.child(
        Grid::new(vec![Track::Fr(1.0), Track::Fr(1.0)])
            .gap(space::SM)
            .child(Text::heading("Heading", theme))
            .child(Text::small("Small", theme))
            .child(Text::subheading("Subheading", theme))
            .child(Text::caption("Caption", theme).tone(Tone::Muted))
            .child(Text::new("Body text", theme)),
    )
}

pub(crate) fn weight(card: Card, ctx: &mut Ctx) -> Card {
    let theme = ctx.theme;
    card.child(
        Column::new()
            .gap(space::XS)
            .child(Text::new("Regular · 400", theme).weight(draw_core::FontWeight::NORMAL))
            .child(Text::new("Medium · 500", theme).weight(draw_core::FontWeight::MEDIUM))
            .child(Text::new("Bold · 700", theme).weight(draw_core::FontWeight::BOLD)),
    )
}

pub(crate) fn wrapping(card: Card, ctx: &mut Ctx) -> Card {
    let theme = ctx.theme;
    card.child(
        Column::new()
            .gap(space::SM)
            .child(
                Panel::new()
                    .color(Color::TRANSPARENT)
                    .flat()
                    .min_size(0.0, 52.0)
                    .child(
                        Label::new(SAMPLE)
                            .max_lines(2)
                            .ellipsis(true)
                            .color(theme.palette().foreground)
                            .anchors(Edges::ZERO)
                            .offsets(Edges::all(space::SM)),
                    ),
            )
            .child(
                Text::new(SAMPLE, theme)
                    .max_lines(1)
                    .ellipsis(true)
                    .tone(Tone::Muted),
            ),
    )
}

pub(crate) fn word_break(card: Card, ctx: &mut Ctx) -> Card {
    let theme = ctx.theme;
    card.child(
        Column::new()
            .gap(space::SM)
            .child(
                Text::small(
                    "中文换行：这是一段很长的中文文本，用来演示宽字符之间的自动分行。",
                    theme,
                )
                .word_break(WordBreak::KeepAll),
            )
            .child(
                Text::small("supercalifragilisticexpialidocious", theme)
                    .word_break(WordBreak::BreakAll),
            )
            .child(Text::caption("KeepAll 中文 · BreakAll 长单词", theme).tone(Tone::Subtle)),
    )
}

pub(crate) fn tone_color(card: Card, ctx: &mut Ctx) -> Card {
    let theme = ctx.theme;
    card.child(
        Column::new()
            .gap(space::XXS)
            .child(Text::small("Accent", theme).tone(Tone::Accent))
            .child(Text::small("Success", theme).tone(Tone::Success))
            .child(Text::small("Warning", theme).tone(Tone::Warning))
            .child(Text::small("Error", theme).tone(Tone::Error))
            .child(Text::small("Muted", theme).tone(Tone::Muted))
            .child(Text::small("Explicit colour", theme).color(theme.palette().success)),
    )
}
