//! quill Canvas 2D demo.
//!
//! Demonstrates the full pipeline in the browser:
//! `Scene/UI -> DrawList -> Canvas2dBackend` driven by the WASM runner.
//!
//! Shows a `Node2D` transform scene, a `Panel { Label, Button }` UI, pointer +
//! keyboard input, and viewport-responsive layout (resize the window).

#[cfg(target_arch = "wasm32")]
mod demo {
    use std::cell::Cell;
    use std::rc::Rc;

    use wasm_bindgen::prelude::*;

    use draw_core::{Color, Edges, NodeId, Rect, Size, Vec2, Viewport};
    use draw_render::{Paint, PaintContext, TextAlign};
    use draw_scene::{SceneTree, Visual};
    use draw_ui::Ui;
    use draw_wasm::App;

    const BACKGROUND: Color = Color::new(0.09, 0.10, 0.13, 1.0);
    const ACCENT: Color = Color::new(0.30, 0.62, 0.98, 1.0);
    const WARN: Color = Color::new(0.98, 0.66, 0.25, 1.0);
    const TEXT: Color = Color::new(0.92, 0.94, 0.98, 1.0);

    struct DemoApp {
        scene: SceneTree,
        rotating: NodeId,
        ui: Ui,
        button: NodeId,
        status: NodeId,
        clicks: Rc<Cell<u32>>,
        viewport: Viewport,
        time: f32,
    }

    impl DemoApp {
        fn new() -> Self {
            // Node2D scene: a rotated rectangle with a circular child.
            let mut scene = SceneTree::new();
            let root = scene.root();
            let rotating = scene.add_node2d(root, "Rotating");
            scene.set_visual(
                rotating,
                Visual::Rect {
                    size: Size::new(120.0, 80.0),
                    color: ACCENT,
                },
            );
            let child = scene.add_node2d(rotating, "Child");
            scene.set_position(child, Vec2::new(80.0, 0.0));
            scene.set_visual(
                child,
                Visual::Circle {
                    radius: 16.0,
                    color: WARN,
                },
            );
            scene.update();

            // UI: Panel { Label, Button, status Label } in a right-hand card.
            let mut ui = Ui::new();
            let panel = ui.add_panel(ui.root());
            ui.set_anchors(panel, Edges::new(1.0, 0.0, 1.0, 0.0));
            ui.set_offsets(panel, Edges::new(-360.0, 40.0, -40.0, 340.0));
            let vbox = ui.add_vbox(panel);
            ui.add_label(vbox, "quill UI");
            let button = ui.add_button(vbox, "Click me");
            let status = ui.add_label(vbox, "Clicked 0 times");

            let clicks = Rc::new(Cell::new(0));
            let counter = clicks.clone();
            ui.set_on_click(button, move || counter.set(counter.get() + 1));

            Self {
                scene,
                rotating,
                ui,
                button,
                status,
                clicks,
                viewport: Viewport::new(Size::new(800.0, 600.0)),
                time: 0.0,
            }
        }

        /// Publishes probe values so a headless check can drive a real click.
        fn report_probe(&self) {
            let Some(window) = web_sys::window() else {
                return;
            };
            let Some(document) = window.document() else {
                return;
            };
            let Some(body) = document.body() else {
                return;
            };
            if let Some(rect) = self.ui.control(self.button).map(|control| control.rect) {
                let _ = body.set_attribute(
                    "data-quill-button",
                    &format!("{},{}", rect.center().x, rect.center().y),
                );
            }
            let _ = body.set_attribute("data-quill-clicks", &self.clicks.get().to_string());
        }
    }

    impl App for DemoApp {
        fn update(&mut self, viewport: Viewport) {
            self.viewport = viewport;
            self.time += 0.016;

            let size = viewport.logical_size();
            self.scene.set_position(
                self.rotating,
                Vec2::new(size.width * 0.30, size.height * 0.42),
            );
            self.scene.set_rotation(self.rotating, self.time);
            self.scene.update();

            self.ui
                .set_text(self.status, format!("Clicked {} times", self.clicks.get()));
            self.ui.layout(viewport);
        }

        fn paint(&mut self, ctx: &mut PaintContext) {
            let size = self.viewport.logical_size();
            ctx.fill_rect(Rect::from_min_size(Vec2::ZERO, size), BACKGROUND);

            // Scene / Node2D demo
            self.scene.paint(ctx);

            // UI demo (Panel / Label / Button)
            self.ui.paint(ctx);

            ctx.draw_text(
                "quill - Control / Layout / Input",
                Vec2::new(size.width * 0.5, size.height - 32.0),
                20.0,
                TextAlign::Center,
                Paint::new(TEXT),
            );

            self.report_probe();
        }

        fn event(&mut self, event: &draw_core::InputEvent) -> draw_core::EventResult {
            let result = self.ui.handle_input(event);
            // Update probe data immediately so a headless check does not depend
            // on the next animation frame.
            self.report_probe();
            result
        }
    }

    #[wasm_bindgen(start)]
    pub fn main() -> Result<(), JsValue> {
        draw_wasm::start("canvas", DemoApp::new())
    }
}
