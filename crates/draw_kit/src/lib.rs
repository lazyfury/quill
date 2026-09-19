//! `draw_kit` — a developer-native component library built on `draw_ui`.
//!
//! The crate layers the [`draw_theme`] design tokens on top of the existing
//! `draw_ui` primitives (panels, labels, flex boxes) **without extending the
//! core `Widget` enum**. Themed chrome is painted by [`Kit`]:
//!
//! ```ignore
//! use draw_kit::{Card, Checkbox, Kit, Text};
//! use draw_theme::Theme;
//!
//! let mut ui = draw_ui::Ui::new();
//! let mut kit = Kit::new(Theme::dark());
//!
//! let root = ui.root();
//! let card = kit.add(&mut ui, root, Card::new());
//! kit.add(&mut ui, card.id(), Text::heading("Settings"));
//! kit.add(&mut ui, card.id(), Checkbox::new("Verbose output"));
//!
//! ui.layout(viewport);
//! kit.paint_surfaces(&ui, &mut ctx);   // behind content
//! ui.paint(&mut ctx);
//! kit.paint_foreground(&ui, &mut ctx); // check marks, knobs, indicators
//! ```
//!
//! Interactions are tracked separately from `draw_ui`: call
//! [`Kit::handle_input`] alongside `Ui::handle_input`.
//!
//! ## Implemented
//!
//! - Text: [`Text`] (display/title/heading/subheading/body/small/caption).
//! - Surfaces: [`Card`], [`Divider`], [`Badge`], [`CodeBlock`], [`Terminal`],
//!   [`EmptyState`].
//! - Controls: [`Checkbox`], [`Switch`].
//!
//! Inputs, selects, tabs, tables, modals and toasts are staged next.

/// Crate name, kept for lightweight smoke checks.
pub const CRATE: &str = "draw_kit";

mod components;
mod kit;
mod overlay;
mod paint;
mod tone;

pub use components::{
    Badge, Button, ButtonVariant, Card, Checkbox, CodeBlock, Divider, EmptyState, Switch, Terminal,
    Text,
};
pub use kit::{InteractState, Kit};
pub use overlay::{OverlayId, Overlays, Placement};
pub use paint::{fill_rounded_rect, fill_rounded_rect_corners, inset, surface, SurfaceStyle};
pub use tone::{SurfaceTone, Tone};

pub use draw_render::CornerRadii;
pub use draw_theme::{self as theme, Theme};
pub use draw_ui::{ControlRef, Ui};

use draw_core::NodeId;

/// A builder that mounts itself into a [`Ui`] and registers chrome with [`Kit`].
///
/// This mirrors `draw_ui::Component` but also receives the themed runtime.
pub trait Component {
    fn mount(self, kit: &mut Kit, ui: &mut Ui, parent: NodeId) -> ControlRef;
}

/// Resets a control to top-left anchors so it sizes to its own content.
///
/// `draw_ui` container components default to `fill_parent`; leaf components
/// call this so they do not stretch when mounted directly under the root.
/// The override is ignored when the parent is a flex/grid container.
pub(crate) fn detach(ui: &mut Ui, id: NodeId) {
    ui.set_anchors(id, draw_core::Edges::ZERO);
    ui.set_offsets(id, draw_core::Edges::ZERO);
}

#[cfg(test)]
mod tests {
    use super::*;
    use draw_core::{Size, Viewport};

    #[test]
    fn crate_identity() {
        assert_eq!(CRATE, "draw_kit");
    }

    #[test]
    fn mount_and_paint_through_a_recording_backend() {
        use draw_backend_recording::RecordingBackend;
        use draw_render::{PaintContext, RenderBackend};

        let mut ui = Ui::new();
        let mut kit = Kit::new(Theme::dark());
        let root = ui.root();
        let card = kit.add(&mut ui, root, Card::new());
        kit.add(&mut ui, card.id(), Text::heading("Hello"));
        let vp = Viewport::new(Size::new(400.0, 300.0));
        ui.layout(vp);

        let mut ctx = PaintContext::new();
        kit.paint_surfaces(&ui, &mut ctx);
        ui.paint(&mut ctx);
        kit.paint_foreground(&ui, &mut ctx);
        let list = ctx.into_draw_list();

        let mut backend = RecordingBackend::new();
        backend.begin_frame(vp).unwrap();
        backend.submit(&list).unwrap();
        backend.end_frame().unwrap();
        assert!(backend.last_frame().is_some());
    }
}
