//! Text components.

use draw_core::{Color, NodeId};
use draw_theme::TextSize;
use draw_ui::{Label, TextOptions};

use crate::{Component, ControlRef, Ui};
use draw_ui::Tone;

/// A single block of text with a semantic size and color.
///
/// ```ignore
/// ui.add(parent, Text::heading("Settings"));
/// ui.add(parent, Text::body("Changes are saved automatically.").tone(Tone::Muted));
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
    fn mount(self, ui: &mut Ui, parent: NodeId) -> ControlRef {
        let theme = ui.theme();
        let color = self.color.unwrap_or_else(|| self.tone.color(&theme));
        ui.add(
            parent,
            Label::new(self.text)
                .font_size(self.size.px())
                .color(color)
                .text_options(self.options),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use draw_core::{Size, Viewport};
    use draw_theme::Theme;

    #[test]
    fn text_mounts_a_label_with_resolved_tone() {
        let mut ui = Ui::new();
        ui.set_theme(Theme::dark());
        let root = ui.root();
        let control = ui.add(root, Text::heading("Hi").tone(Tone::Error));
        ui.layout(Viewport::new(Size::new(400.0, 200.0)));
        assert!(ui.control(control.id()).is_some());
        match ui.widget(control.id()) {
            Some(draw_ui::Widget::Label {
                text,
                font_size,
                color,
                ..
            }) => {
                assert_eq!(text, "Hi");
                assert_eq!(*font_size, TextSize::Heading.px());
                assert_eq!(*color, Theme::dark().palette.error);
            }
            other => panic!("expected a label, got {other:?}"),
        }
    }

    #[test]
    fn explicit_color_wins() {
        let mut ui = Ui::new();
        ui.set_theme(Theme::light());
        let root = ui.root();
        let control = ui.add(root, Text::caption("v1.2.0").color(Color::WHITE));
        assert_eq!(
            ui.widget(control.id()).and_then(|w| w.text()),
            Some("v1.2.0")
        );
    }
}
