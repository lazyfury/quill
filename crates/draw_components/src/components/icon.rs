//! A themed glyph icon.

use draw_core::{Color, Size};
use draw_theme::{Theme, Tone};
use draw_ui::Widget;

use crate::base::{Component, Spec};
use crate::glyph::{paint_glyph, Glyph};

/// A small monochrome [`Glyph`], sized and tinted by the theme.
///
/// The glyph is stroke geometry from [`crate::glyph`], not an image or an SVG
/// file, so an `Icon` is backend-neutral like every other component.
pub struct Icon {
    spec: Spec,
    theme: &'static dyn Theme,
    glyph: Glyph,
    size: f32,
    color: Option<Color>,
    tone: Tone,
    stroke: Option<f32>,
}

impl Icon {
    /// A 16px glyph in the theme's foreground.
    pub fn new(glyph: Glyph, theme: &'static dyn Theme) -> Self {
        Self {
            spec: Spec::leaf(),
            theme,
            glyph,
            size: 16.0,
            color: None,
            tone: Tone::Default,
            stroke: None,
        }
    }

    /// Square size in logical pixels.
    pub fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    /// Explicit colour, overriding [`Icon::tone`].
    pub fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }

    /// Semantic colour resolved from the palette (default `Tone::Default`).
    pub fn tone(mut self, tone: Tone) -> Self {
        self.tone = tone;
        self
    }

    /// Stroke width in logical pixels (default scales with the size).
    pub fn stroke(mut self, stroke: f32) -> Self {
        self.stroke = Some(stroke);
        self
    }
}

impl Component for Icon {
    fn spec(&mut self) -> &mut Spec {
        &mut self.spec
    }

    fn name(&self) -> &'static str {
        "Icon"
    }

    fn widget(&self) -> Widget {
        Widget::Panel {
            color: Color::TRANSPARENT,
            border: None,
        }
    }

    fn prepare(&mut self) {
        let theme = self.theme;
        let glyph = self.glyph;
        let size = self.size.max(1.0);
        let color = self.color.unwrap_or_else(|| self.tone.color(theme));
        let stroke = self.stroke.unwrap_or_else(|| (size * 0.1).clamp(1.0, 2.0));
        self.spec.data.min_size = Size::new(size, size);
        self.spec.foreground = Some(Box::new(move |ctx, rect, _| {
            paint_glyph(glyph, ctx, rect, color, stroke);
        }));
    }
}

crate::impl_scene_child!(Icon);
