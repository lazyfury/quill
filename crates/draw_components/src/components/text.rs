//! Text components.

use draw_app::Label;
use draw_core::{Color, NodeId};
use draw_scene::SceneTree;
use draw_theme::{TextSize, Tone};
use draw_ui::TextOptions;

use crate::{Component, ControlRef};

/// A single block of text with a semantic size and color.
///
/// ```ignore
/// draw_app::add(parent, Text::heading("Settings"));
/// draw_app::add(parent, Text::body("Changes are saved automatically.").tone(Tone::Muted));
/// ```
#[derive(Debug, Clone)]
pub struct Text {
    text: String,
    size: TextSize,
    tone: Tone,
    color: Option<Color>,
    options: TextOptions,
}

impl Text {
    /// Body text in the default foreground.
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            size: TextSize::Body,
            tone: Tone::Default,
            color: None,
            options: TextOptions::default(),
        }
    }

    /// 48–64px hero text.
    pub fn display(text: impl Into<String>) -> Self {
        Self::new(text).size(TextSize::Display)
    }

    /// 28–40px page title.
    pub fn title(text: impl Into<String>) -> Self {
        Self::new(text).size(TextSize::Title)
    }

    /// 20–24px section heading.
    pub fn heading(text: impl Into<String>) -> Self {
        Self::new(text).size(TextSize::Heading)
    }

    /// 16–18px subsection heading.
    pub fn subheading(text: impl Into<String>) -> Self {
        Self::new(text).size(TextSize::Subheading)
    }

    /// 12–14px secondary text.
    pub fn small(text: impl Into<String>) -> Self {
        Self::new(text).size(TextSize::Small)
    }

    /// 11–12px metadata.
    pub fn caption(text: impl Into<String>) -> Self {
        Self::new(text).size(TextSize::Caption)
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
    fn mount(self, tree: &mut SceneTree, parent: NodeId) -> ControlRef {
        let theme = draw_ui::theme(tree);
        let color = self.color.unwrap_or_else(|| self.tone.color(&theme));
        draw_app::add(
            tree,
            parent,
            Label::new(self.text)
                .font_size(self.size.px())
                .color(color)
                .text_options(self.options),
        )
    }
}
