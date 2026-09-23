//! Text components.

use crate::base::{Component, Spec};
use draw_core::{Color, FontWeight};
use draw_theme::{TextSize, Theme, Tone};
use draw_ui::{TextOptions, Widget, WordBreak};

/// A single block of text with a semantic size and color.
///
/// The theme is a value passed to the constructor; the component resolves its
/// tone with it and never reads a theme from the tree.
///
/// ```ignore
/// tree.add_child(root, Text::heading("Settings", theme));
/// tree.add_child(root, Text::body("Saved automatically.", theme).tone(Tone::Muted));
/// tree.add_child(root, Text::body("Important", theme).bold());
/// ```
pub struct Text {
    spec: Spec,
    text: String,
    size: TextSize,
    tone: Tone,
    color: Option<Color>,
    options: TextOptions,
    /// Explicit weight; `None` falls back to the theme's per-size token.
    weight: Option<FontWeight>,
    theme: &'static dyn Theme,
}

impl Text {
    /// Body text in the default foreground.
    pub fn new(text: impl Into<String>, theme: &'static dyn Theme) -> Self {
        Self {
            spec: Spec::leaf(),
            text: text.into(),
            size: TextSize::Body,
            tone: Tone::Default,
            color: None,
            options: TextOptions::default(),
            weight: None,
            theme,
        }
    }

    /// 48–64px hero text.
    pub fn display(text: impl Into<String>, theme: &'static dyn Theme) -> Self {
        Self::new(text, theme).size(TextSize::Display)
    }

    /// 28–40px page title.
    pub fn title(text: impl Into<String>, theme: &'static dyn Theme) -> Self {
        Self::new(text, theme).size(TextSize::Title)
    }

    /// 20–24px section heading.
    pub fn heading(text: impl Into<String>, theme: &'static dyn Theme) -> Self {
        Self::new(text, theme).size(TextSize::Heading)
    }

    /// 16–18px subsection heading.
    pub fn subheading(text: impl Into<String>, theme: &'static dyn Theme) -> Self {
        Self::new(text, theme).size(TextSize::Subheading)
    }

    /// 12–14px secondary text.
    pub fn small(text: impl Into<String>, theme: &'static dyn Theme) -> Self {
        Self::new(text, theme).size(TextSize::Small)
    }

    /// 11–12px metadata.
    pub fn caption(text: impl Into<String>, theme: &'static dyn Theme) -> Self {
        Self::new(text, theme).size(TextSize::Caption)
    }

    pub fn size(mut self, size: TextSize) -> Self {
        self.size = size;
        self
    }

    pub fn tone(mut self, tone: Tone) -> Self {
        self.tone = tone;
        self
    }

    /// Overrides the resolved tone with an explicit color.
    pub fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self
    }

    /// Enables or disables soft wrapping.
    pub fn wrap(mut self, wrap: bool) -> Self {
        self.options.wrap = wrap;
        self
    }

    /// Sets how the text breaks across lines (word / character / keep-all).
    pub fn word_break(mut self, word_break: WordBreak) -> Self {
        self.options = self.options.word_break(word_break);
        self
    }

    pub fn max_lines(mut self, max_lines: usize) -> Self {
        self.options = self.options.max_lines(max_lines);
        self
    }

    pub fn ellipsis(mut self, ellipsis: bool) -> Self {
        self.options = self.options.ellipsis(ellipsis);
        self
    }

    pub fn text_options(mut self, options: TextOptions) -> Self {
        self.options = options;
        self
    }

    /// Sets the text weight, overriding the theme's per-size token.
    pub fn weight(mut self, weight: FontWeight) -> Self {
        self.weight = Some(weight);
        self
    }

    /// Shorthand for [`weight`](Self::weight)`(`[`FontWeight::BOLD`]`)`.
    pub fn bold(self) -> Self {
        self.weight(FontWeight::BOLD)
    }
}

impl Component for Text {
    fn spec(&mut self) -> &mut Spec {
        &mut self.spec
    }

    fn name(&self) -> &'static str {
        "Text"
    }

    fn widget(&self) -> Widget {
        let weight = self
            .weight
            .unwrap_or_else(|| self.theme.font_weight(self.size));
        Widget::Label {
            text: self.text.clone(),
            font_size: self.theme.font_size(self.size),
            color: self.color.unwrap_or_else(|| self.tone.color(self.theme)),
            options: self.options.weight(weight),
        }
    }
}

crate::impl_scene_child!(Text);

#[cfg(test)]
mod tests {
    use super::*;
    use draw_theme::{default_theme, DefaultTheme, Mode, Palette, Theme};

    fn weight_of(text: Text) -> FontWeight {
        match text.widget() {
            Widget::Label { options, .. } => options.weight,
            _ => panic!("Text must build a Label"),
        }
    }

    fn size_of(text: Text) -> f32 {
        match text.widget() {
            Widget::Label { font_size, .. } => font_size,
            _ => panic!("Text must build a Label"),
        }
    }

    #[test]
    fn default_weight_follows_the_theme() {
        let theme = default_theme(Mode::Dark);
        assert_eq!(weight_of(Text::new("x", theme)), FontWeight::NORMAL);
    }

    #[test]
    fn bold_overrides_the_theme_token() {
        let theme = default_theme(Mode::Dark);
        assert_eq!(weight_of(Text::new("x", theme).bold()), FontWeight::BOLD);
    }

    #[test]
    fn a_theme_can_make_headings_bold() {
        struct BoldHeadings(DefaultTheme);
        impl Theme for BoldHeadings {
            fn palette(&self) -> &Palette {
                self.0.palette()
            }
            fn mode(&self) -> Mode {
                self.0.mode()
            }
            fn font_weight(&self, size: TextSize) -> FontWeight {
                if matches!(size, TextSize::Heading) {
                    FontWeight::BOLD
                } else {
                    FontWeight::NORMAL
                }
            }
        }
        let theme: &'static dyn Theme = Box::leak(Box::new(BoldHeadings(DefaultTheme::dark())));
        assert_eq!(weight_of(Text::heading("x", theme)), FontWeight::BOLD);
        assert_eq!(weight_of(Text::new("x", theme)), FontWeight::NORMAL);
    }

    #[test]
    fn a_theme_can_scale_the_type() {
        struct BiggerType(DefaultTheme);
        impl Theme for BiggerType {
            fn palette(&self) -> &Palette {
                self.0.palette()
            }
            fn mode(&self) -> Mode {
                self.0.mode()
            }
            fn font_size(&self, size: TextSize) -> f32 {
                self.0.font_size(size) * 2.0
            }
        }
        let theme: &'static dyn Theme = Box::leak(Box::new(BiggerType(DefaultTheme::dark())));
        assert_eq!(
            size_of(Text::heading("x", theme)),
            TextSize::Heading.px() * 2.0
        );
        assert_eq!(size_of(Text::new("x", theme)), TextSize::Body.px() * 2.0);
    }
}
