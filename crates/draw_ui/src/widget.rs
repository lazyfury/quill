use draw_core::{Color, Edges, Size};

use crate::layout::{
    self, layout_text, measure_with, ContentSize, FlexStyle, GridStyle, TextMeasurer, TextOptions,
};

/// Runtime state of a button.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ButtonState {
    pub hovered: bool,
    pub pressed: bool,
    pub click_count: u32,
}

/// Layout parameters shared by vertical and horizontal box containers.
///
/// Deprecated in favor of [`FlexStyle`]; kept so existing code that constructs
/// `BoxLayout` continues to compile and maps onto a flex container.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoxLayout {
    pub separation: f32,
    pub padding: Edges,
}

impl Default for BoxLayout {
    fn default() -> Self {
        Self {
            separation: 8.0,
            padding: Edges::all(16.0),
        }
    }
}

/// Visual + behavioral payload of a control.
#[derive(Debug, Clone, PartialEq)]
pub enum Widget {
    Panel {
        color: Color,
        border: Option<Color>,
    },
    /// A flex container (row/column) with justify/align/grow support.
    Flex(FlexStyle),
    /// A grid container with fixed/`fr`/auto tracks.
    Grid(GridStyle),
    Label {
        text: String,
        font_size: f32,
        color: Color,
        options: TextOptions,
    },
    Button(ButtonData),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ButtonData {
    pub text: String,
    pub font_size: f32,
    pub state: ButtonState,
    pub color: Color,
    pub hover_color: Color,
    pub pressed_color: Color,
    pub text_color: Color,
    pub options: TextOptions,
}

impl ButtonData {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            font_size: 18.0,
            state: ButtonState::default(),
            color: Color::new(0.24, 0.28, 0.38, 1.0),
            hover_color: Color::new(0.32, 0.38, 0.50, 1.0),
            pressed_color: Color::new(0.20, 0.45, 0.78, 1.0),
            text_color: Color::new(0.95, 0.97, 1.0, 1.0),
            // Buttons default to a single line to preserve compact sizing.
            options: TextOptions::no_wrap(),
        }
    }

    pub fn fill(&self) -> Color {
        if self.state.pressed {
            self.pressed_color
        } else if self.state.hovered {
            self.hover_color
        } else {
            self.color
        }
    }
}

impl Widget {
    pub fn is_container(&self) -> bool {
        matches!(self, Self::Flex(_) | Self::Grid(_))
    }

    pub fn is_button(&self) -> bool {
        matches!(self, Self::Button(_))
    }

    /// Intrinsic size using the default [`TextMeasurer`].
    pub fn measure(&self, available: Size) -> ContentSize {
        self.measure_with(available, &layout::ApproxTextMeasurer)
    }

    /// Intrinsic size given the space the parent can offer and a text measurer.
    ///
    /// Text controls return a `preferred` size that already accounts for soft
    /// wrapping within `available.width`, so a stretched label reports the
    /// taller height it needs for its wrapped (and possibly clipped) lines.
    pub fn measure_with(&self, available: Size, measurer: &dyn TextMeasurer) -> ContentSize {
        match self {
            Self::Panel { .. } | Self::Flex(_) | Self::Grid(_) => ContentSize::ZERO,
            Self::Label {
                text,
                font_size,
                options,
                ..
            } => {
                let line_h = measurer.line_height(*font_size);
                let natural = measure_with(measurer, text, *font_size);
                let mut min_width =
                    layout::longest_unit_width_with(measurer, text, *font_size, options.word_break);
                // A wrapping label hard-breaks an overlong word when it paints
                // (see `layout::text::hard_break`), so its minimum must never
                // exceed the width the parent offered. Without this cap a giant
                // unbreakable token — one line of JSON from an error reply, a
                // long URL — pushes the whole flex chain wider than the
                // viewport and shoves siblings off the surface.
                if options.wrap && available.width > 0.0 {
                    min_width = min_width.min(available.width);
                }
                let min = Size::new(min_width, line_h);

                let preferred = if options.wrap
                    && available.width > 0.0
                    && available.width + 1e-3 < natural.width
                {
                    let lines = layout_text(measurer, text, *font_size, available.width, *options);
                    Size::new(available.width, lines.len() as f32 * line_h)
                } else {
                    let mut height = natural.height;
                    if let Some(max_lines) = options.max_lines {
                        height = height.min(max_lines as f32 * line_h);
                    }
                    Size::new(natural.width, height)
                };
                ContentSize::new(min, preferred.max(min))
            }
            Self::Button(button) => {
                let line_h = measurer.line_height(button.font_size);
                let natural = measure_with(measurer, &button.text, button.font_size);
                let mut text_min = layout::longest_unit_width_with(
                    measurer,
                    &button.text,
                    button.font_size,
                    button.options.word_break,
                );
                // Same cap as the label: a wrapping button can hard-break an
                // overlong token, so it must not report a minimum wider than
                // the space its parent offered.
                if button.options.wrap && available.width > 0.0 {
                    text_min = text_min.min((available.width - 32.0).max(0.0));
                }
                let min = Size::new(text_min + 32.0, line_h.max(36.0) + 12.0);

                let preferred = if button.options.wrap
                    && available.width > 0.0
                    && available.width + 1e-3 < natural.width + 32.0
                {
                    let inner = (available.width - 32.0).max(0.0);
                    let lines = layout_text(
                        measurer,
                        &button.text,
                        button.font_size,
                        inner,
                        button.options,
                    );
                    Size::new(available.width, lines.len() as f32 * line_h + 12.0)
                } else {
                    let mut height = natural.height;
                    if let Some(max_lines) = button.options.max_lines {
                        height = height.min(max_lines as f32 * line_h);
                    }
                    Size::new(natural.width + 32.0, height.max(36.0) + 12.0)
                };
                ContentSize::new(min, preferred.max(min))
            }
        }
    }

    /// Minimum intrinsic size (content can never shrink below this).
    pub fn content_min_size(&self) -> Size {
        self.measure(Size::ZERO).min
    }

    pub fn text(&self) -> Option<&str> {
        match self {
            Self::Label { text, .. } => Some(text),
            Self::Button(button) => Some(&button.text),
            _ => None,
        }
    }

    /// Replaces the text, returning whether it actually changed.
    pub fn set_text(&mut self, new_text: impl Into<String>) -> bool {
        match self {
            Self::Label { text, .. } => {
                let new_text = new_text.into();
                if *text == new_text {
                    false
                } else {
                    *text = new_text;
                    true
                }
            }
            Self::Button(button) => {
                let new_text = new_text.into();
                if button.text == new_text {
                    false
                } else {
                    button.text = new_text;
                    true
                }
            }
            _ => false,
        }
    }
}

/// Rough text size estimate (no font shaping in MVP).
///
/// Width assumes an average glyph advance; wide (CJK) characters count double.
pub fn estimate_text_size(text: &str, font_size: f32) -> Size {
    layout::measure(text, font_size)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn label(text: &str, font_size: f32) -> Widget {
        Widget::Label {
            text: text.into(),
            font_size,
            color: Color::WHITE,
            options: TextOptions::default(),
        }
    }

    #[test]
    fn label_and_button_min_sizes() {
        let button = Widget::Button(ButtonData::new("Click"));
        let size = label("Hello", 20.0).content_min_size();
        assert!(size.width > 0.0 && size.height > 0.0);

        let button_size = button.content_min_size();
        assert!(button_size.width > size.width * 0.0);
        assert!(button_size.height >= 36.0);
    }

    #[test]
    fn wrapped_label_reports_taller_preferred_size() {
        let label = label("hello world hello world", 20.0);
        let full = label.measure(Size::ZERO).preferred;
        let wrapped = label.measure(Size::new(80.0, 1000.0)).preferred;
        assert!(wrapped.width <= 80.0 + 1e-3);
        assert!(wrapped.height > full.height);
    }

    #[test]
    fn max_lines_caps_preferred_height() {
        let mut label = label("hello world hello world", 20.0);
        if let Widget::Label { options, .. } = &mut label {
            *options = TextOptions::default().max_lines(1);
        }
        let capped = label.measure(Size::new(80.0, 1000.0)).preferred;
        let full = label.measure(Size::ZERO).preferred;
        assert!((capped.height - full.height).abs() < 1e-3);
    }

    /// One line of JSON (or any giant unbreakable ASCII token) must not report
    /// a minimum wider than the space offered: the paint pass hard-breaks such
    /// a word, so the layout may shrink the label to fit and the flex chain
    /// must not stretch to the token's width.
    #[test]
    fn an_unbreakable_token_does_not_widen_the_minimum() {
        let token = "x".repeat(2_000);
        let widget = label(&token, 14.0);
        let available = Size::new(268.0, 600.0);
        let measured = widget.measure(available);
        assert!(
            measured.min.width <= available.width + 1e-3,
            "min {} exceeds the offered width",
            measured.min.width
        );
        assert!(
            measured.preferred.width <= available.width + 1e-3,
            "preferred {} exceeds the offered width",
            measured.preferred.width
        );

        // Without wrap the natural width is still the truth: nothing can break
        // the token, so the min keeps reporting it (the caller clips).
        let mut no_wrap = label(&token, 14.0);
        if let Widget::Label { options, .. } = &mut no_wrap {
            *options = TextOptions::no_wrap();
        }
        assert!(no_wrap.measure(available).min.width > available.width);
    }

    #[test]
    fn a_wrapping_button_with_a_long_token_stays_within_the_offer() {
        let mut button = ButtonData::new("x".repeat(2_000));
        button.options = TextOptions::default();
        let available = Size::new(268.0, 600.0);
        let measured = Widget::Button(button).measure(available);
        assert!(measured.min.width <= available.width + 1e-3);
        assert!(measured.preferred.width <= available.width + 1e-3);
    }

    #[test]
    fn set_text_reports_change() {
        let mut label = label("a", 10.0);
        assert!(!label.set_text("a"));
        assert!(label.set_text("b"));
    }

    #[test]
    fn button_fill_follows_state() {
        let mut button = ButtonData::new("x");
        assert_eq!(button.fill(), button.color);
        button.state.hovered = true;
        assert_eq!(button.fill(), button.hover_color);
        button.state.pressed = true;
        assert_eq!(button.fill(), button.pressed_color);
    }
}
