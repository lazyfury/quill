//! Component debug drawing: yellow bounds plus a `name#id` label per control.
//!
//! [`DebugOverlay`] is a lightweight, togglable wrapper around
//! [`draw_ui::paint_debug`](draw_ui::paint_debug). Unlike
//! [`PerformanceOverlay`](crate::PerformanceOverlay) it owns no UI tree: it
//! simply draws over whatever `Ui` you pass to [`DebugOverlay::paint`], so it
//! works for the application's own UI.

use draw_render::PaintContext;
use draw_scene::SceneTree;
use draw_ui::DebugDrawOptions;

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
    pub fn paint(&self, tree: &SceneTree, ctx: &mut PaintContext) {
        if self.open {
            draw_ui::paint_debug(tree, ctx, &self.options);
        }
    }
}
