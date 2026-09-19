//! quill Canvas 2D (WASM) demo.
//!
//! Drives the shared, backend-neutral [`demo_app::DemoApp`] through the WASM
//! runner: `Input -> DemoApp -> DrawList -> Canvas2dBackend`. The same app is
//! used by the native `wgpu_demo`, so the two backends render identical
//! scene/UI/layout code.
//!
//! Exposes `data-quill-button` / `data-quill-clicks` DOM attributes so a
//! headless check can drive a real click without capturing pixels.

#[cfg(target_arch = "wasm32")]
mod demo {
    use wasm_bindgen::prelude::*;

    use draw_core::{EventResult, InputEvent, Viewport};
    use draw_render::PaintContext;
    use draw_wasm::App;

    use demo_app::DemoApp;

    struct WebDemo(DemoApp);

    impl WebDemo {
        fn new() -> Self {
            Self(DemoApp::new())
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
            if let Some(center) = self.0.button_center() {
                let _ =
                    body.set_attribute("data-quill-button", &format!("{},{}", center.x, center.y));
            }
            let _ = body.set_attribute("data-quill-clicks", &self.0.clicks().to_string());
        }
    }

    impl App for WebDemo {
        fn update(&mut self, viewport: Viewport) {
            self.0.update(viewport, 0.016);
            self.0.layout(viewport);
            self.report_probe();
        }

        fn paint(&mut self, ctx: &mut PaintContext) {
            self.0.paint(ctx);
            self.report_probe();
        }

        fn event(&mut self, event: &InputEvent) -> EventResult {
            let result = self.0.event(event);
            self.report_probe();
            result
        }
    }

    #[wasm_bindgen(start)]
    pub fn main() -> Result<(), JsValue> {
        draw_wasm::start("canvas", WebDemo::new())
    }
}
