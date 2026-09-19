//! Component debug drawing: yellow bounds plus a `name#id` label per control.
//!
//! [`DebugOverlay`] is a lightweight, togglable wrapper around
//! [`Ui::paint_debug`](draw_ui::Ui::paint_debug). Unlike
//! [`PerformanceOverlay`](crate::PerformanceOverlay) it owns no UI tree: it
//! simply draws over whatever `Ui` you pass to [`DebugOverlay::paint`], so it
//! works for the application's own UI.

use draw_render::PaintContext;
use draw_ui::{DebugDrawOptions, Ui};

/// Debug drawing of every visible component: a border (yellow by default) and a
/// `Name #id` label in each control's top-left corner.
///
/// ```ignore
/// let mut debug = DebugOverlay::new();
///
/// // per frame, after painting the app UI into `ctx`:
/// debug.paint(&app_ui, &mut ctx);
///
/// // toggle at runtime (e.g. an F3 key binding)
/// debug.toggle();
/// ```
pub struct DebugOverlay {
    open: bool,
    options: DebugDrawOptions,
}

impl Default for DebugOverlay {
    fn default() -> Self {
        Self::new()
    }
}

impl DebugOverlay {
    /// A visible overlay with [`DebugDrawOptions::default`].
    pub fn new() -> Self {
        Self::with_options(DebugDrawOptions::default())
    }

    pub fn with_options(options: DebugDrawOptions) -> Self {
        Self {
            open: true,
            options,
        }
    }

    pub fn options(&self) -> &DebugDrawOptions {
        &self.options
    }

    pub fn set_options(&mut self, options: DebugDrawOptions) {
        self.options = options;
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Shows/hides the overlay and returns the new state.
    pub fn set_open(&mut self, open: bool) -> bool {
        self.open = open;
        self.open
    }

    /// Flips visibility and returns the new state.
    pub fn toggle(&mut self) -> bool {
        self.open = !self.open;
        self.open
    }

    /// Draws debug bounds for every visible control in `ui` into `ctx`.
    ///
    /// No-op while closed. Call after the application UI is painted so the
    /// boxes render on top.
    pub fn paint(&self, ui: &Ui, ctx: &mut PaintContext) {
        if self.open {
            ui.paint_debug(ctx, &self.options);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use draw_core::{Color, Size, Viewport};
    use draw_render::DrawCommand;
    use draw_ui::{Button, Panel, VBox};

    fn ui() -> Ui {
        let mut ui = Ui::new();
        let panel = ui.add(ui.root(), Panel::new());
        let vbox = ui.add(panel.id(), VBox::new());
        ui.add(vbox.id(), Button::new("Click me"));
        ui.layout(Viewport::new(Size::new(640.0, 480.0)));
        ui
    }

    #[test]
    fn paints_yellow_bounds_and_name_id_labels() {
        let ui = ui();
        let overlay = DebugOverlay::new();
        let mut ctx = PaintContext::new();
        overlay.paint(&ui, &mut ctx);
        let list = ctx.into_draw_list();

        let strokes: Vec<Color> = list
            .commands()
            .iter()
            .filter_map(|command| match command {
                DrawCommand::StrokeRect { paint, .. } => Some(paint.color),
                _ => None,
            })
            .collect();
        // root + panel + vbox + button
        assert_eq!(strokes.len(), 4);
        assert!(strokes.iter().all(|color| *color == Color::YELLOW));

        let labels: Vec<String> = list
            .commands()
            .iter()
            .filter_map(|command| match command {
                DrawCommand::DrawText { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(labels.len(), 4);
        assert!(labels.iter().any(|label| label.starts_with("Button #")));
        assert!(labels.iter().any(|label| label.starts_with("Panel #")));
    }

    #[test]
    fn closed_overlay_paints_nothing() {
        let ui = ui();
        let mut overlay = DebugOverlay::new();
        overlay.set_open(false);
        let mut ctx = PaintContext::new();
        overlay.paint(&ui, &mut ctx);
        assert!(ctx.is_empty());
    }

    #[test]
    fn toggle_flips_state() {
        let mut overlay = DebugOverlay::new();
        assert!(overlay.is_open());
        assert!(!overlay.toggle());
        assert!(overlay.toggle());
    }

    #[test]
    fn options_control_the_label() {
        let ui = ui();
        let mut overlay = DebugOverlay::with_options(DebugDrawOptions {
            show_ids: false,
            ..DebugDrawOptions::default()
        });
        let mut ctx = PaintContext::new();
        overlay.paint(&ui, &mut ctx);
        let has_hash = ctx.draw_list().iter().any(|command| match command {
            DrawCommand::DrawText { text, .. } => text.contains('#'),
            _ => false,
        });
        assert!(!has_hash, "ids should be hidden");
        overlay.set_options(DebugDrawOptions::default());
        let mut ctx = PaintContext::new();
        overlay.paint(&ui, &mut ctx);
        let has_hash = ctx.draw_list().iter().any(|command| match command {
            DrawCommand::DrawText { text, .. } => text.contains('#'),
            _ => false,
        });
        assert!(has_hash);
    }
}
