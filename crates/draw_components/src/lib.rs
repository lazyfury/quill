//! `draw_components` — a themed component library for `draw_app`.
//!
//! Components implement [`draw_app::Component`], receive the active [`Theme`] as
//! a value, and attach their chrome with the styling primitives from `draw_ui`
//! (`SurfaceStyle`, `Tone`, a foreground/surface decorator). There is no runtime
//! object, no theme on the tree and no second paint pass:
//!
//! ```ignore
//! use draw_components::{Card, Checkbox, Text};
//! use draw_scene::SceneTree;
//! use draw_theme::{space, Theme};
//!
//! let theme = Theme::dark();
//! let mut tree = SceneTree::new();
//! let root = tree.root();
//!
//! let panel = tree.add_child(root, Card::new(theme).gap(space::MD)
//!     .child(Text::heading("Settings", theme))
//!     .child(Checkbox::new("Verbose output", theme)));
//!
//! draw_ui::layout(&mut tree, viewport);
//! draw_ui::paint(&tree, &mut ctx);      // surfaces + content + marks, in tree order
//! draw_ui::route_input(&mut tree, &event);
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

pub use draw_app::{Component, Flex, Grid, Label, Panel, Spec};
pub use draw_render::CornerRadii;
pub use draw_theme::{self as theme, SurfaceTone, Theme, Tone};
