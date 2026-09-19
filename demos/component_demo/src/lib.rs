//! quill component demo.
//!
//! The recommended way to build a UI with `draw_ui`: compose reusable
//! components, set layout, react to events and attach the whole thing to a
//! Canvas via `draw_wasm`.
//!
//! ```text
//! Ui
//! ├── Panel (right card)          <- tree.add_child(root, Panel::new())
//! │   └── VBox                    <- component composition
//! │       ├── Label("Hello")      <- component
//! │       ├── Button("Click me")  <- component with on_click
//! │       └── Label(status)       <- updated on state change
//! └── Node2D scene (left)         <- draw_scene transform demo
//! ```

#[cfg(target_arch = "wasm32")]
mod demo {
    use std::cell::Cell;
    use std::rc::Rc;

    use wasm_bindgen::prelude::*;
    use web_sys::CanvasRenderingContext2d;

    use draw_app::{Button, Component, Label, Panel, VBox};
    use draw_core::{Color, Edges, NodeId, Rect, Size, Vec2, ViewportSize};
    use draw_render::{Paint, PaintContext, TextAlign};
    use draw_scene::{SceneTree, Visual};
    use draw_wasm::{App, CanvasTextMeasurer};

    const BACKGROUND: Color = Color::new(0.09, 0.10, 0.13, 1.0);
    const ACCENT: Color = Color::new(0.30, 0.62, 0.98, 1.0);
    const WARN: Color = Color::new(0.98, 0.66, 0.25, 1.0);
    const TEXT: Color = Color::new(0.92, 0.94, 0.98, 1.0);

    struct DemoApp {
        tree: SceneTree,
        button: NodeId,
        status: NodeId,
        clicks: Rc<Cell<u32>>,
        rotating: NodeId,
        viewport: ViewportSize,
        time: f32,
    }

    impl DemoApp {
        fn new() -> Self {
            // --- One tree for world (Node2D) and UI (Control) -----------
            let mut tree = SceneTree::new();
            let tree_root = tree.root();

            // A right-hand card.
            let panel = tree.add_child(
                tree_root,
                Panel::new()
                    .anchors(Edges::new(1.0, 0.0, 1.0, 0.0))
                    .offsets(Edges::new(-360.0, 40.0, -40.0, 320.0)),
            );

            // Stack contents vertically.
            let vbox = tree.add_child(panel, VBox::new().separation(12.0));
            tree.add_child(vbox, Label::new("Hello"));
            tree.add_child(
                vbox,
                Label::new("Drawing Core Component Demo")
                    .font_size(14.0)
                    .color(Color::new(0.70, 0.75, 0.85, 1.0)),
            );

            // React to events: mutate application state in the callback.
            let clicks = Rc::new(Cell::new(0));
            let counter = clicks.clone();
            let button = tree.add_child(
                vbox,
                Button::new("Click me").on_click(move || counter.set(counter.get() + 1)),
            );

            // State is reflected back into the UI on the next frame.
            let status = tree.add_child(vbox, Label::new("Status: Clicked 0 times"));

            // --- Scene: a rotated Node2D with a child, same tree ----------
            let rotating = tree.add_node2d(tree_root, "Rotating");
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
                button,
                status,
                clicks,
                rotating,
                viewport: ViewportSize::new(Size::new(800.0, 600.0)),
                time: 0.0,
            }
        }

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
            if let Some(rect) =
                draw_ui::control(&self.tree, self.button).map(|control| control.rect)
            {
                let _ = body.set_attribute(
                    "data-quill-button",
                    &format!("{},{}", rect.center().x, rect.center().y),
                );
            }
            let _ = body.set_attribute("data-quill-clicks", &self.clicks.get().to_string());
        }
    }

    impl App for DemoApp {
        fn attach_context(&mut self, ctx: &CanvasRenderingContext2d) {
            draw_ui::set_text_measurer(
                &mut self.tree,
                Rc::new(CanvasTextMeasurer::new(ctx.clone())),
            );
        }

        fn cursor(&self) -> draw_core::Cursor {
            draw_app::hovered_cursor(&self.tree)
        }

        fn update(&mut self, viewport: ViewportSize) {
            self.viewport = viewport;
            self.time += 0.016;

            // Animate the scene.
            let size = viewport.logical_size();
            self.tree.set_position(
                self.rotating,
                Vec2::new(size.width * 0.30, size.height * 0.42),
            );
            self.tree.set_rotation(self.rotating, self.time);
            self.tree.update();

            // Reflect state -> UI, then lay out (resize-aware).
            draw_app::set_text(
                &mut self.tree,
                self.status,
                format!("Status: Clicked {} times", self.clicks.get()),
            );
            draw_ui::layout(&mut self.tree, viewport);
        }

        fn paint(&mut self, ctx: &mut PaintContext) {
            let size = self.viewport.logical_size();
            ctx.fill_rect(Rect::from_min_size(Vec2::ZERO, size), BACKGROUND);

            // Scene / Node2D demo (same tree as the UI)
            self.tree.paint(ctx);

            // UI components
            draw_ui::paint(&self.tree, ctx);

            ctx.draw_text(
                "Scene / Node2D Demo",
                Vec2::new(40.0, 60.0),
                18.0,
                TextAlign::Left,
                Paint::new(TEXT.with_alpha(0.85)),
            );
            ctx.draw_text(
                "quill - Component Demo",
                Vec2::new(size.width * 0.5, size.height - 32.0),
                20.0,
                TextAlign::Center,
                Paint::new(TEXT),
            );

            self.report_probe();
        }

        fn event(&mut self, event: &draw_core::InputEvent) -> draw_core::EventResult {
            let result = draw_app::route_input(&mut self.tree, event);
            self.report_probe();
            result
        }
    }

    #[wasm_bindgen(start)]
    pub fn main() -> Result<(), JsValue> {
        draw_wasm::start("canvas", DemoApp::new())
    }
}
