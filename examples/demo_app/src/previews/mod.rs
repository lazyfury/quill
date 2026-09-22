//! The gallery preview pages: one [`Router`] view per group, each a header plus
//! a two-column grid of live component cards.
//!
//! [`group_view`] builds a page; [`card_for`] wraps one catalog item in a
//! [`Card`] and appends its live example (from [`examples`]) and snippet.

use std::cell::RefCell;
use std::rc::Rc;

use draw_components::{
    Card, Column, Component, Grid, ListState, Router, ScrollView, ScrollViewState, Text,
};
use draw_core::Edges;
use draw_theme::{space, Theme, Tone};
use draw_ui::{AlignContent, Track};

use crate::catalog::{self, Item};
use crate::GalleryState;

mod examples;

pub use examples::menu_content;

/// Builds the preview page for `group`: a header plus a two-column grid of
/// cards, wrapped in a [`ScrollView`] so a page taller than the pane still
/// reaches every card. The view's [`ScrollViewState`] is appended to `scrolls`
/// for the app to sync each frame.
pub fn group_view(
    group: usize,
    theme: &'static dyn Theme,
    state: &GalleryState,
    lists: &mut Vec<ListState>,
    routers: &Rc<RefCell<Vec<Router>>>,
    scrolls: &mut Vec<ScrollViewState>,
) -> ScrollView {
    let meta = &catalog::GROUPS[group];
    let header = Column::new()
        .gap(space::XXS)
        .child(Text::heading(meta.name, theme))
        .child(Text::caption(meta.blurb, theme).tone(Tone::Muted));

    let mut grid = Grid::new(vec![Track::Fr(1.0), Track::Fr(1.0)])
        .align_content(AlignContent::Start)
        .gap(space::MD)
        .padding(Edges::ZERO);
    for (index, item) in catalog::ITEMS[group].iter().enumerate() {
        grid = grid.child(card_for(
            group, index, item, theme, state, lists, routers, scrolls,
        ));
    }

    let page = Column::new()
        .gap(space::LG)
        .padding(Edges::all(space::LG))
        .child(header)
        .child(grid);

    let view = ScrollView::new(theme).child(page);
    scrolls.push(view.state());
    view
}

fn card_for(
    group: usize,
    index: usize,
    item: &Item,
    theme: &'static dyn Theme,
    state: &GalleryState,
    lists: &mut Vec<ListState>,
    routers: &Rc<RefCell<Vec<Router>>>,
    scrolls: &mut Vec<ScrollViewState>,
) -> Card {
    let card = Card::new(theme)
        .gap(space::SM)
        .padding(Edges::all(space::LG))
        .child(Text::subheading(item.name, theme))
        .child(Text::caption(item.blurb, theme).tone(Tone::Muted));
    example(card, group, index, theme, state, lists, routers, scrolls)
        .child(Text::small(item.snippet, theme).tone(Tone::Subtle))
}

/// Appends the item's live example to its card.
fn example(
    card: Card,
    group: usize,
    index: usize,
    theme: &'static dyn Theme,
    state: &GalleryState,
    lists: &mut Vec<ListState>,
    routers: &Rc<RefCell<Vec<Router>>>,
    scrolls: &mut Vec<ScrollViewState>,
) -> Card {
    let table = examples::EXAMPLES;
    debug_assert!(group < table.len() && index < table[group].len());
    let mut ctx = examples::Ctx {
        theme,
        state,
        lists,
        routers,
        scrolls,
    };
    (table[group][index])(card, &mut ctx)
}
