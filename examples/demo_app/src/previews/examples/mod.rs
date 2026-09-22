//! One builder per catalog item: [`EXAMPLES`] is parallel to
//! [`crate::catalog::ITEMS`] and indexed `[group][item]`.
//!
//! Builders are split by concern: [`display`] is stateless (`layout`, `text`,
//! `content`, `theme`), while [`interactive`] writes requests into
//! [`crate::GalleryState`] or hands back [`ListState`] / [`Router`] handles.

use std::cell::RefCell;
use std::rc::Rc;

use draw_components::{Card, ListState, Router, ScrollViewState};
use draw_theme::Theme;

use crate::GalleryState;

mod display;
mod interactive;

pub use interactive::menu_content;

/// Everything a builder may need besides its card.
pub(super) struct Ctx<'a> {
    pub theme: &'static dyn Theme,
    pub state: &'a GalleryState,
    pub lists: &'a mut Vec<ListState>,
    pub routers: &'a Rc<RefCell<Vec<Router>>>,
    pub scrolls: &'a mut Vec<ScrollViewState>,
}

/// The signature of one preview builder.
pub(super) type ExampleFn = fn(Card, &mut Ctx) -> Card;

/// Builders in catalog order (`EXAMPLES[g][i]` previews `ITEMS[g][i]`).
pub(super) const EXAMPLES: &[&[ExampleFn]] = &[
    // Layout
    &[
        display::layout::flex,
        display::layout::grow_shrink,
        display::layout::grid,
        display::layout::anchors,
        display::layout::alignment,
        display::layout::padding_gap,
    ],
    // Text
    &[
        display::text::type_scale,
        display::text::weight,
        display::text::wrapping,
        display::text::word_break,
        display::text::tone_color,
    ],
    // Surfaces
    &[
        display::content::card,
        display::content::divider,
        display::content::badge,
    ],
    // Content
    &[
        display::content::code_block,
        display::content::terminal,
        display::content::empty_state,
    ],
    // Icons
    &[
        display::icons::set,
        display::icons::sizes,
        display::icons::tones,
        display::icons::context,
    ],
    // Controls
    &[
        interactive::button,
        interactive::checkbox,
        interactive::switch,
        interactive::resize,
    ],
    // Data
    &[
        interactive::list,
        interactive::scroll_view,
        interactive::router,
    ],
    // Overlays
    &[
        interactive::menu,
        interactive::confirm,
        interactive::message,
    ],
    // Theme & Platform
    &[
        display::theme::palette,
        display::theme::surface_levels,
        display::theme::tones,
        display::theme::density,
        display::theme::radius_spacing,
        display::theme::cursor,
    ],
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog;

    #[test]
    fn the_example_table_matches_the_catalog() {
        assert_eq!(EXAMPLES.len(), catalog::ITEMS.len());
        for (group, (items, examples)) in catalog::ITEMS.iter().zip(EXAMPLES).enumerate() {
            assert_eq!(items.len(), examples.len(), "group {group} count mismatch");
        }
    }
}
