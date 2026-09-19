//! quill component demo.
//!
//! The recommended way to build a UI with `draw_ui`: compose reusable
//! components, set layout, react to events and attach the whole thing to a
//! Canvas via `draw_wasm`.
//!
//! ```text
//! Ui
//! ├── Panel (right card)          <- ui.add(..., Panel::new())
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

    use draw_core::{Color, Edges, NodeId, Rect, Size, Vec2, Viewport};
    use draw_render::{Paint, PaintContext, TextAlign};
    use draw_scene::{SceneTree, Visual};
    use draw_ui::{Button, Label, Panel, Ui, VBox};
    use draw_wasm::{App, CanvasTextMeasurer};

    const BACKGROUND: Color = Color::new(0.09, 0.10, 0.13, 1.0);
    const ACCENT: Color = Color::new(0.30, 0.62, 0.98, 1.0);
    const WARN: Color = Color::new(0.98, 0.66, 0.25, 1.0);
    const TEXT: Color = Color::new(0.92, 0.94, 0.98, 1.0);

    struct DemoApp {
        ui: Ui,
        button: NodeId,
        status: NodeId,
        clicks: Rc<Cell<u32>>,
        scene: SceneTree,
        rotating: NodeId,
        viewport: Viewport,
        time: f32,
    }

    impl DemoApp {
        fn new() -> Self {
            // --- UI: compose components -----------------------------------
            let mut ui = Ui::new();

            // A right-hand card.
            let panel = ui.add(ui.root(), Panel::new());
            ui.set_anchors(panel.id(), Edges::new(1.0, 0.0, 1.0, 0.0));
            ui.set_offsets(panel.id(), Edges::new(-360.0, 40.0, -40.0, 320.0));

            // Stack contents vertically.
            let vbox = ui.add(panel.id(), VBox::new().separation(12.0));
            ui.add(vbox.id(), Label::new("Hello"));
            ui.add(
                vbox.id(),
                Label::new("Drawing Core Component Demo")
                    .font_size(14.0)
                    .color(Color::new(0.70, 0.75, 0.85, 1.0)),
            );

            // React to events: mutate application state in the callback.
            let clicks = Rc::new(Cell::new(0));
            let counter = clicks.clone();
            let button = ui.add(
                vbox.id(),
                Button::new("Click me").on_click(move || counter.set(counter.get() + 1)),
            );

            // State is reflected back into the UI on the next frame.
            let status = ui.add(vbox.id(), Label::new("Status: Clicked 0 times"));

            // --- Scene: a rotated Node2D with a child ---------------------
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

            Self {
                ui,
                button: button.id(),
                status: status.id(),
                clicks,
                scene,
                rotating,
                viewport: Viewport::new(Size::new(800.0, 600.0)),
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
        fn attach_context(&mut self, ctx: &CanvasRenderingContext2d) {
            self.ui
                .set_text_measurer(Rc::new(CanvasTextMeasurer::new(ctx.clone())));
        }

        fn pointer_cursor(&self) -> bool {
            self.ui.hovered_is_button()
        }

        fn update(&mut self, viewport: Viewport) {
            self.viewport = viewport;
            self.time += 0.016;

            // Animate the scene.
            let size = viewport.logical_size();
            self.scene.set_position(
                self.rotating,
                Vec2::new(size.width * 0.30, size.height * 0.42),
            );
            self.scene.set_rotation(self.rotating, self.time);
            self.scene.update();

            // Reflect state -> UI, then lay out (resize-aware).
            self.ui.set_text(
                self.status,
                format!("Status: Clicked {} times", self.clicks.get()),
            );
            self.ui.layout(viewport);
        }

        fn paint(&mut self, ctx: &mut PaintContext) {
            let size = self.viewport.logical_size();
            ctx.fill_rect(Rect::from_min_size(Vec2::ZERO, size), BACKGROUND);

            // Scene / Node2D demo
            self.scene.paint(ctx);

            // UI components
            self.ui.paint(ctx);

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
            let result = self.ui.handle_input(event);
            self.report_probe();
            result
        }
    }

    #[wasm_bindgen(start)]
    pub fn main() -> Result<(), JsValue> {
        draw_wasm::start("canvas", DemoApp::new())
    }
}
