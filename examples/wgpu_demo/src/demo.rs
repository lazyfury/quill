//! The wgpu demo: the shared [`demo_app::DemoApp`] plus text metrics from the
//! wgpu backend's loaded font.
//!
//! The shared app owns scene/UI/layout/input. Here we only wrap the backend's
//! [`FontMetrics`] in a `draw_ui::TextMeasurer`, so the layout engine measures
//! text with the exact advances the backend renders with (proportional Latin +
//! CJK when a system font is available).

use std::rc::Rc;

use draw_backend_wgpu::FontMetrics;
use draw_core::{EventResult, FontWeight, InputEvent, ViewportSize};
use draw_render::PaintContext;
use draw_ui::TextMeasurer;

use demo_app::DemoApp;

/// Adapts the backend's font metrics to the layout engine.
struct BackendTextMeasurer {
    metrics: FontMetrics,
}

impl TextMeasurer for BackendTextMeasurer {
    fn advance(&self, ch: char, font_size: f32) -> f32 {
        self.metrics.advance(ch, font_size)
    }

    fn advance_weighted(&self, ch: char, font_size: f32, weight: FontWeight) -> f32 {
        self.metrics.advance_weighted(ch, font_size, weight)
    }

    fn line_height(&self, font_size: f32) -> f32 {
        self.metrics.line_height(font_size)
    }

    fn ascent(&self, font_size: f32) -> f32 {
        self.metrics.ascent(font_size)
    }

    fn measure_run(&self, text: &str, font_size: f32) -> f32 {
        self.metrics.measure_run(text, font_size)
    }

    fn measure_run_weighted(&self, text: &str, font_size: f32, weight: FontWeight) -> f32 {
        self.metrics.measure_run_weighted(text, font_size, weight)
    }
}

/// Application state owned by the window runner.
pub struct Demo {
    app: DemoApp,
}

impl Default for Demo {
    fn default() -> Self {
        Self::new()
    }
}

impl Demo {
    pub fn new() -> Self {
        Self {
            app: DemoApp::new(),
        }
    }

    /// Injects the backend's real font metrics into the layout engine.
    ///
    /// Call after the [`draw_backend_wgpu::WgpuBackend`] is created.
    pub fn set_text_metrics(&mut self, metrics: FontMetrics) {
        self.app
            .set_text_measurer(Rc::new(BackendTextMeasurer { metrics }));
    }

    /// Reserves extra top padding on the sidebar for a transparent title bar
    /// (macOS traffic lights). See [`DemoApp::set_titlebar_inset`].
    pub fn set_titlebar_inset(&mut self, inset: f32) {
        self.app.set_titlebar_inset(inset);
    }

    /// Advances the animation and updates text for the new viewport.
    ///
    /// UI layout is deliberately *not* performed here: the host times it as a
    /// separate pipeline phase via [`Demo::layout`].
    pub fn update(&mut self, viewport: ViewportSize, dt: f32) {
        self.app.update(viewport, dt);
    }

    /// Resolves UI layout for `viewport` (the timed *layout* pipeline phase).
    pub fn layout(&mut self, viewport: ViewportSize) {
        self.app.layout(viewport);
    }

    /// Emits this frame's `DrawList` into `ctx`.
    pub fn paint(&self, ctx: &mut PaintContext) {
        self.app.paint(ctx);
    }

    /// Routes a backend-neutral input event through the UI.
    pub fn event(&mut self, event: &InputEvent) -> EventResult {
        self.app.event(event)
    }

    /// The scene tree shared by world and UI nodes.
    pub fn tree(&self) -> &draw_scene::SceneTree {
        self.app.tree()
    }

    /// Controls in the demo UI.
    pub fn control_count(&self) -> usize {
        self.app.control_count()
    }

    /// Cursor the host should show for the current pointer position.
    pub fn cursor(&self) -> draw_core::Cursor {
        self.app.cursor()
    }

    /// Whether the app still has work for another frame (animation, transient
    /// overlays, pending layout/paint). The host schedules a redraw while this
    /// is `true` and sleeps otherwise.
    pub fn needs_frame(&self) -> bool {
        self.app.needs_frame()
    }
}
