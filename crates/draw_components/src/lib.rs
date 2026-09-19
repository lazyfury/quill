//! `draw_components` — a themed component library for `draw_ui`.
//!
//! The crate only contains component builders. Components implement
//! [`draw_ui::Component`], read the active [`Theme`](draw_theme::Theme) from
//! `ui.theme()`, and attach their chrome with the styling primitives from
//! `draw_ui` (`SurfaceStyle`, `Tone`, `surface_decor`, …). There is no runtime
//! object and no second paint pass:
//!
//! ```ignore
//! use draw_components::{Card, Checkbox, Text};
//! use draw_theme::{space, Theme};
//! use draw_ui::Ui;
//!
//! let mut ui = Ui::new();
//! ui.set_theme(Theme::dark());
//!
//! let root = ui.root();
//! let card = ui.add(root, Card::new().gap(space::MD));
//! ui.add(card.id(), Text::heading("Settings"));
//! ui.add(card.id(), Checkbox::new("Verbose output"));
//!
//! ui.layout(viewport);
//! ui.paint(&mut ctx);      // surfaces + content + marks, in tree order
//! ui.handle_input(&event);
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
pub use draw_render::CornerRadii;
pub use draw_theme::{self as theme, Theme};
pub use draw_ui::{Component, ControlRef, Ui};

/// Resets a control to top-left anchors so it sizes to its own content.
///
/// `draw_ui` container components default to `fill_parent`; leaf components
/// call this so they do not stretch when mounted directly under the root.
/// The override is ignored when the parent is a flex/grid container.
pub(crate) fn detach(ui: &mut Ui, id: draw_core::NodeId) {
    ui.set_anchors(id, draw_core::Edges::ZERO);
    ui.set_offsets(id, draw_core::Edges::ZERO);
}

#[cfg(test)]
mod tests {
    use super::*;
    use draw_core::{Size, Viewport};

    #[test]
    fn crate_identity() {
        assert_eq!(CRATE, "draw_components");
    }

    #[test]
    fn mount_and_paint_through_a_recording_backend() {
        use draw_backend_recording::RecordingBackend;
        use draw_render::{PaintContext, RenderBackend};

        let mut ui = Ui::new();
        ui.set_theme(Theme::dark());
        let root = ui.root();
        let card = ui.add(root, Card::new());
        ui.add(card.id(), Text::heading("Hello"));
        let vp = Viewport::new(Size::new(400.0, 300.0));
        ui.layout(vp);

        let mut ctx = PaintContext::new();
        ui.paint(&mut ctx);
        let list = ctx.into_draw_list();

        let mut backend = RecordingBackend::new();
        backend.begin_frame(vp).unwrap();
        backend.submit(&list).unwrap();
        backend.end_frame().unwrap();
        assert!(backend.last_frame().is_some());
    }
}
