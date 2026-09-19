//! Deterministic fixtures and sizes shared by the benchmark scenarios.
//!
//! Fixtures build a fixed tree/UI, call `update`/`layout` once so the first
//! measured call starts from the same state as every later one, and expose the
//! ids a routine needs to mutate. No randomness, no I/O: two calls to the same
//! builder produce identical structure.

use draw_core::{Color, NodeId, Size, Vec2, Viewport};
use draw_scene::{SceneTree, Visual};
use draw_ui::Ui;

/// Entity counts every scenario is run at, to expose scaling curves.
pub const SIZES: [usize; 3] = [100, 1_000, 10_000];

/// Logical viewport used by UI scenarios.
pub const VIEWPORT_SIZE: Size = Size::new(1280.0, 800.0);

/// A flat scene: `n` visible `Node2D`s laid out on a grid under the root.
///
/// `update` has already run once, so `tree.update()` starts clean — that is what
/// the `update_clean` scenario measures.
pub struct SceneFixture {
    pub tree: SceneTree,
    /// Every child node, in creation order.
    pub ids: Vec<NodeId>,
}

impl SceneFixture {
    /// Builds a grid scene with `n` visible rect nodes.
    pub fn new(n: usize) -> Self {
        let mut tree = SceneTree::new();
        let root = tree.root();
        let columns = (n as f64).sqrt().ceil().max(1.0) as usize;
        let mut ids = Vec::with_capacity(n);

        for i in 0..n {
            let id = tree.add_node2d(root, format!("item_{i}"));
            let x = (i % columns) as f32 * 4.0;
            let y = (i / columns) as f32 * 4.0;
            tree.set_position(id, Vec2::new(x, y));
            tree.set_visual(
                id,
                Visual::Rect {
                    size: Size::new(2.0, 2.0),
                    color: Color::RED,
                },
            );
            ids.push(id);
        }

        tree.update();
        Self { tree, ids }
    }

    /// Draw commands emitted by [`SceneTree::paint`] for this fixture.
    pub fn expected_commands(&self) -> usize {
        // Each visible node emits Save + SetTransform + FillRect + Restore.
        self.ids.len() * 4
    }
}

/// A UI with `n` labels inside a panel/vbox, already laid out once.
pub struct UiFixture {
    pub ui: Ui,
    pub viewport: Viewport,
    /// The label controls, in creation order.
    pub ids: Vec<NodeId>,
    /// Center of the *first* label — the worst case for reverse hit testing.
    pub first_center: Vec2,
}

impl UiFixture {
    /// Builds a panel > vbox > `n` labels UI and lays it out.
    pub fn new(n: usize) -> Self {
        let viewport = Viewport::new(VIEWPORT_SIZE);
        let mut ui = Ui::new();
        let panel = ui.add_panel(ui.root());
        let vbox = ui.add_vbox(panel);

        let mut ids = Vec::with_capacity(n);
        for i in 0..n {
            ids.push(ui.add_label(vbox, format!("Item {i}")));
        }

        ui.layout(viewport);
        let first_center = ui
            .control(ids[0])
            .map(|control| control.rect.center())
            .unwrap_or(Vec2::ZERO);

        Self {
            ui,
            viewport,
            ids,
            first_center,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scene_fixture_is_deterministic() {
        let a = SceneFixture::new(50);
        let b = SceneFixture::new(50);
        assert_eq!(a.ids.len(), 50);
        assert_eq!(a.tree.node_count(), 51);

        let mut ctx_a = draw_render::PaintContext::new();
        let mut ctx_b = draw_render::PaintContext::new();
        a.tree.paint(&mut ctx_a);
        b.tree.paint(&mut ctx_b);
        assert_eq!(ctx_a.into_draw_list(), ctx_b.into_draw_list());
        assert_eq!(a.expected_commands(), a.ids.len() * 4);
    }

    #[test]
    fn scene_fixture_starts_clean() {
        let mut fixture = SceneFixture::new(100);
        // setup already updated, so the next update recomputes nothing
        assert_eq!(fixture.tree.update(), 0);
    }

    #[test]
    fn ui_fixture_lays_out_and_hit_tests() {
        let fixture = UiFixture::new(100);
        assert_eq!(fixture.ids.len(), 100);
        let hit = fixture.ui.hit_test(fixture.first_center);
        assert_eq!(hit, Some(fixture.ids[0]));
    }

    #[test]
    fn ui_fixture_is_deterministic() {
        let mut a = UiFixture::new(20);
        let mut b = UiFixture::new(20);
        a.ui.layout(a.viewport);
        b.ui.layout(b.viewport);

        let mut ctx_a = draw_render::PaintContext::new();
        let mut ctx_b = draw_render::PaintContext::new();
        a.ui.paint(&mut ctx_a);
        b.ui.paint(&mut ctx_b);
        assert_eq!(ctx_a.into_draw_list(), ctx_b.into_draw_list());
    }
}
