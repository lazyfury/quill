//! The application runtime: owns a [`SceneTree`], routes input and submits the
//! painted frame to a [`RenderBackend`].
//!
//! Layout and paint stay in `draw_ui`; `App` is the wiring between them and the
//! host. A host drives one frame as:
//!
//! ```ignore
//! app.event(&event);
//! app.render(viewport, &mut backend)?;
//! ```

use draw_core::ViewportSize;
use draw_render::{PaintContext, RenderBackend};
use draw_scene::SceneTree;

use crate::input;

/// A backend-neutral application: a [`SceneTree`] plus the input/rendering loop.
///
/// `App` owns the tree so a host never has to thread it separately.
pub struct App {
    tree: SceneTree,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    /// Creates an app with an empty scene tree.
    pub fn new() -> Self {
        Self {
            tree: SceneTree::new(),
        }
    }

    /// Wraps an existing scene tree.
    pub fn with_tree(tree: SceneTree) -> Self {
        Self { tree }
    }

    /// The shared scene tree.
    pub fn tree(&self) -> &SceneTree {
        &self.tree
    }

    /// The shared scene tree, mutably.
    pub fn tree_mut(&mut self) -> &mut SceneTree {
        &mut self.tree
    }

    /// Routes one event (`_input` -> world -> GUI -> `_unhandled_input`).
    pub fn event(&mut self, event: &draw_core::InputEvent) -> draw_core::EventResult {
        input::route_input(&mut self.tree, event)
    }

    /// Runs Update -> Layout -> Paint and submits the frame to `backend`.
    pub fn render<B: RenderBackend>(
        &mut self,
        viewport: ViewportSize,
        backend: &mut B,
    ) -> Result<(), B::Error> {
        self.tree.update();
        draw_ui::layout(&mut self.tree, viewport);

        let mut ctx = PaintContext::new();
        draw_ui::paint(&self.tree, &mut ctx);
        let list = ctx.into_draw_list();

        backend.begin_frame(viewport)?;
        backend.submit(&list)?;
        backend.end_frame()
    }
}
