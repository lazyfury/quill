//! quill Canvas 2D demo.
//!
//! Demonstrates the `Scene -> DrawList -> Canvas2dBackend` path plus the WASM
//! runner. Shows a rectangle, a circle, a rotated `Node2D` with a child, text,
//! and viewport-responsive layout (resize the window).

#[cfg(target_arch = "wasm32")]
mod demo {
    use wasm_bindgen::prelude::*;

    use draw_core::{Color, NodeId, Rect, Size, Vec2, Viewport};
    use draw_render::{Paint, PaintContext, TextAlign};
    use draw_scene::{SceneTree, Visual};
    use draw_wasm::App;

    const BACKGROUND: Color = Color::new(0.09, 0.10, 0.13, 1.0);
    const PANEL: Color = Color::new(0.17, 0.20, 0.28, 1.0);
    const ACCENT: Color = Color::new(0.30, 0.62, 0.98, 1.0);
    const WARN: Color = Color::new(0.98, 0.66, 0.25, 1.0);
    const TEXT: Color = Color::new(0.92, 0.94, 0.98, 1.0);

    struct DemoApp {
        tree: SceneTree,
        rotating: NodeId,
        child: NodeId,
        viewport: Viewport,
        time: f32,
    }

    impl DemoApp {
        fn new() -> Self {
            let mut tree = SceneTree::new();
            let root = tree.root();

            // A rotated parent (rectangle) with a circular child.
            let rotating = tree.add_node2d(root, "Rotating");
            tree.set_visual(
                rotating,
                Visual::Rect {
                    size: Size::new(120.0, 80.0),
                    color: ACCENT,
                },
            );
            let child = tree.add_node2d(rotating, "Child");
            tree.set_position(child, Vec2::new(80.0, 0.0));
            tree.set_visual(
                child,
                Visual::Circle {
                    radius: 16.0,
                    color: WARN,
                },
            );
            tree.update();

            Self {
                tree,
                rotating,
                child,
                viewport: Viewport::new(Size::new(800.0, 600.0)),
                time: 0.0,
            }
        }
    }

    impl App for DemoApp {
        fn update(&mut self, viewport: Viewport) {
            self.viewport = viewport;
            self.time += 0.016;

            let size = viewport.logical_size();
            let center = Vec2::new(size.width * 0.5, size.height * 0.42);
            self.tree.set_position(self.rotating, center);
            self.tree.set_rotation(self.rotating, self.time);
            let _ = self.child;
            self.tree.update();
        }

        fn paint(&mut self, ctx: &mut PaintContext) {
            let size = self.viewport.logical_size();
            let full = Rect::from_min_size(Vec2::ZERO, size);

            // background + a resize-responsive side panel
            ctx.fill_rect(full, BACKGROUND);
            let panel_w = (size.width * 0.28).clamp(160.0, 320.0);
            ctx.fill_rect(
                Rect::from_min_size(
                    Vec2::new(size.width - panel_w - 16.0, 16.0),
                    Size::new(panel_w, size.height - 32.0),
                ),
                PANEL,
            );

            // 1) rectangle
            ctx.fill_rect(
                Rect::from_min_size(Vec2::new(40.0, 40.0), Size::new(160.0, 100.0)),
                Color::new(0.35, 0.78, 0.55, 1.0),
            );
            ctx.stroke_rect(
                Rect::from_min_size(Vec2::new(40.0, 40.0), Size::new(160.0, 100.0)),
                2.0,
                TEXT,
            );

            // 2) circle
            ctx.fill_circle(Vec2::new(120.0, 220.0), 48.0, WARN);

            // 3) transformed scene node + child
            self.tree.paint(ctx);

            // 4) text (centered, responsive)
            ctx.draw_text(
                "quill - Canvas 2D backend",
                Vec2::new(size.width * 0.5, size.height - 40.0),
                22.0,
                TextAlign::Center,
                Paint::new(TEXT),
            );
            ctx.draw_text(
                &format!("logical size: {:.0} x {:.0}", size.width, size.height),
                Vec2::new(24.0, size.height - 28.0),
                14.0,
                TextAlign::Left,
                Paint::new(TEXT.with_alpha(0.7)),
            );
        }
    }

    #[wasm_bindgen(start)]
    pub fn main() -> Result<(), JsValue> {
        draw_wasm::start("canvas", DemoApp::new())
    }
}
