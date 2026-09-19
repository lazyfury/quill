//! A `draw_ui::TextMeasurer` backed by the Canvas 2D `measureText` API.
//!
//! The core `ApproxTextMeasurer` guesses advances and (crucially) an `ascent`
//! of `0.8 * font_size`. The Canvas backend paints `DrawText` at the baseline
//! reported by that measurer, so a guessed ascent leaves text visibly off
//! centre. Measuring with the same font the backend draws with keeps layout and
//! rendering in agreement.
//!
//! Measure results are memoized per `(font_size, char)` / `font_size`, since
//! layout queries the measurer many times per pass.

use std::cell::RefCell;
use std::collections::HashMap;

use draw_backend_canvas::font_spec;
use draw_ui::TextMeasurer;
use web_sys::CanvasRenderingContext2d;

/// Measures text using a live [`CanvasRenderingContext2d`].
///
/// Construct one from the context the backend renders with (the runner hands it
/// to [`App::attach_context`](crate::App::attach_context)) and install it with
/// [`draw_ui::set_text_measurer`](draw_ui::set_text_measurer).
pub struct CanvasTextMeasurer {
    ctx: CanvasRenderingContext2d,
    advances: RefCell<HashMap<(u32, u32), f32>>,
    runs: RefCell<HashMap<(u32, String), f32>>,
    metrics: RefCell<HashMap<u32, FontMetrics>>,
}

#[derive(Clone, Copy)]
struct FontMetrics {
    ascent: f32,
    line_height: f32,
}

impl CanvasTextMeasurer {
    /// Wraps `ctx`. The context is cloned, so the backend can keep using it.
    pub fn new(ctx: CanvasRenderingContext2d) -> Self {
        Self {
            ctx,
            advances: RefCell::new(HashMap::new()),
            runs: RefCell::new(HashMap::new()),
            metrics: RefCell::new(HashMap::new()),
        }
    }

    fn metrics_for(&self, font_size: f32) -> FontMetrics {
        let key = font_size.to_bits();
        if let Some(metrics) = self.metrics.borrow().get(&key) {
            return *metrics;
        }
        let metrics = self.measure_metrics(font_size);
        self.metrics.borrow_mut().insert(key, metrics);
        metrics
    }

    fn measure_metrics(&self, font_size: f32) -> FontMetrics {
        self.ctx.set_font(&font_spec(font_size));

        // "Mg" exercises an ascender and a descender, so its actual bounding box
        // is a reasonable fallback when font bounding boxes are unavailable.
        let fallback_ascent = font_size * 0.8;
        let fallback_descent = font_size * 0.2;
        let (ascent, descent) = match self.ctx.measure_text("Mg") {
            Ok(measured) => {
                let ascent = finite(measured.font_bounding_box_ascent())
                    .or_else(|| finite(measured.actual_bounding_box_ascent()))
                    .filter(|value| *value > 0.0)
                    .unwrap_or(fallback_ascent);
                let descent = finite(measured.font_bounding_box_descent())
                    .or_else(|| finite(measured.actual_bounding_box_descent()))
                    .filter(|value| *value > 0.0)
                    .unwrap_or(fallback_descent);
                (ascent, descent)
            }
            Err(_) => (fallback_ascent, fallback_descent),
        };

        FontMetrics {
            ascent,
            line_height: ascent + descent,
        }
    }

    fn advance_for(&self, ch: char, font_size: f32) -> f32 {
        let key = (font_size.to_bits(), ch as u32);
        if let Some(advance) = self.advances.borrow().get(&key) {
            return *advance;
        }

        self.ctx.set_font(&font_spec(font_size));
        let advance = self
            .ctx
            .measure_text(&ch.to_string())
            .ok()
            .map(|measured| measured.width() as f32)
            .filter(|width| width.is_finite() && *width >= 0.0)
            .unwrap_or(font_size * 0.55);
        self.advances.borrow_mut().insert(key, advance);
        advance
    }

    fn measure_run(&self, text: &str, font_size: f32) -> f32 {
        let key = (font_size.to_bits(), text.to_string());
        if let Some(width) = self.runs.borrow().get(&key) {
            return *width;
        }

        self.ctx.set_font(&font_spec(font_size));
        // `measureText` on the whole run applies browser shaping (kerning,
        // ligatures, bidi), unlike summing per-character advances.
        let width = self
            .ctx
            .measure_text(text)
            .ok()
            .map(|measured| measured.width() as f32)
            .filter(|width| width.is_finite() && *width >= 0.0)
            .unwrap_or_else(|| text.chars().map(|ch| self.advance_for(ch, font_size)).sum());
        self.runs.borrow_mut().insert(key, width);
        width
    }
}

impl TextMeasurer for CanvasTextMeasurer {
    fn advance(&self, ch: char, font_size: f32) -> f32 {
        self.advance_for(ch, font_size)
    }

    fn line_height(&self, font_size: f32) -> f32 {
        self.metrics_for(font_size).line_height
    }

    fn ascent(&self, font_size: f32) -> f32 {
        self.metrics_for(font_size).ascent
    }

    fn measure_run(&self, text: &str, font_size: f32) -> f32 {
        CanvasTextMeasurer::measure_run(self, text, font_size)
    }
}

fn finite(value: f64) -> Option<f32> {
    if value.is_finite() {
        Some(value as f32)
    } else {
        None
    }
}
