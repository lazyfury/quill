//! `draw_components` — a themed component library for `draw_ui`.
//!
//! The crate only contains component builders. Components implement
//! [`draw_app::Component`], read the active [`Theme`] from
//! `draw_ui::theme()`, and attach their chrome with the styling primitives from
//! `draw_ui` (`SurfaceStyle`, `Tone`, `surface_decor`, …). There is no runtime
//! object and no second paint pass:
//!
//! ```ignore
//! use draw_components::{Card, Checkbox, Text};
//! use draw_scene::SceneTree;
//! use draw_theme::{space, Theme};
//! use draw_ui as ui;
//!
//! let mut tree = SceneTree::new();
//! ui::set_theme(&mut tree, Theme::dark());
//!
//! let root = ui::add_flex(&mut tree, tree.root(), ui::FlexStyle::column());
//! ui::mount(&mut tree, root, Card::new().gap(space::MD)
//!     .child(Text::heading("Settings"))
//!     .child(Checkbox::new("Verbose output")));
//!
//! ui::layout(&mut tree, viewport);
//! ui::paint(&tree, &mut ctx);      // surfaces + content + marks, in tree order
//! ui::route_input(&mut tree, &event);
//! ```
//!
//! ## Implemented
//!
//! - Text: [`Text`] (display/title/heading/subheading/body/small/caption).
//! - Surfaces: [`Card`], [`Divider`], [`Badge`], [`CodeBlock`], [`Terminal`],
//!   [`EmptyState`].
//! - Controls: [`Button`], [`Checkbox`], [`Switch`].
//! - Floating: [`Overlays`] (`confirm`, `popover`, `tips`, `message`).
//!
//! Inputs, selects, tabs and tables are staged next.

/// Crate name, kept for lightweight smoke checks.
pub const CRATE: &str = "draw_components";

mod components;
mod overlay;

pub use components::{
    Badge, Button, ButtonVariant, Card, Checkbox, CodeBlock, Divider, EmptyState, Switch, Terminal,
    Text,
};
pub use overlay::{OverlayId, Overlays, Placement};

// Re-exported so component users need one import for the common surface.
pub use draw_app::{child, BuildContext, Child, Column, Component, ControlRef, Row, View, ViewExt};
pub use draw_render::CornerRadii;
pub use draw_theme::{self as theme, SurfaceTone, Theme, Tone};

/// Resets a control to top-left anchors so it sizes to its own content.
///
/// `draw_ui` container components default to `fill_parent`; leaf components
/// call this so they do not stretch when mounted directly under the root.
/// The override is ignored when the parent is a flex/grid container.
pub(crate) fn detach(tree: &mut draw_scene::SceneTree, id: draw_core::NodeId) {
    draw_app::update_control(tree, id, |data| {
        data.anchors = draw_core::Edges::ZERO;
        data.offsets = draw_core::Edges::ZERO;
    });
}
