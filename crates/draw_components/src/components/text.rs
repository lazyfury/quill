//! Text components.

use draw_core::Color;
use draw_theme::{TextSize, Theme, Tone};
use draw_ui::{TextOptions, Widget};
use draw_widgets::{Component, Spec};

/// A single block of text with a semantic size and color.
///
/// The theme is a value passed to the constructor; the component resolves its
/// tone with it and never reads a theme from the tree.
///
/// ```ignore
/// tree.add_child(root, Text::heading("Settings", theme));
/// tree.add_child(root, Text::body("Saved automatically.", theme).tone(Tone::Muted));
/// ```
pub struct Text {
    spec: Spec,
    text: String,
    size: TextSize,
    tone: Tone,
    color: Option<Color>,
    options: TextOptions,
    theme: Theme,
}

impl Text {
    /// Body text in the default foreground.
    pub fn new(text: impl Into<String>, theme: Theme) -> Self {
        Self {
            spec: Spec::leaf(),
            text: text.into(),
            size: TextSize::Body,
            tone: Tone::Default,
            color: None,
            options: TextOptions::default(),
            theme,
        }
    }

    /// 48–64px hero text.
    pub fn display(text: impl Into<String>, theme: Theme) -> Self {
        Self::new(text, theme).size(TextSize::Display)
    }

    /// 28–40px page title.
    pub fn title(text: impl Into<String>, theme: Theme) -> Self {
        Self::new(text, theme).size(TextSize::Title)
    }

    /// 20–24px section heading.
    pub fn heading(text: impl Into<String>, theme: Theme) -> Self {
        Self::new(text, theme).size(TextSize::Heading)
    }

    /// 16–18px subsection heading.
    pub fn subheading(text: impl Into<String>, theme: Theme) -> Self {
        Self::new(text, theme).size(TextSize::Subheading)
    }

    /// 12–14px secondary text.
    pub fn small(text: impl Into<String>, theme: Theme) -> Self {
        Self::new(text, theme).size(TextSize::Small)
    }

    /// 11–12px metadata.
    pub fn caption(text: impl Into<String>, theme: Theme) -> Self {
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

    pub fn size_px(&self) -> f32 {
        self.size.px()
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
        Widget::Label {
            text: self.text.clone(),
            font_size: self.size.px(),
            color: self.color.unwrap_or_else(|| self.tone.color(&self.theme)),
            options: self.options,
        }
    }
}

draw_widgets::impl_scene_child!(Text);
