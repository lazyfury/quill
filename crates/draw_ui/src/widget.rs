use draw_core::{Color, Edges, Size};

/// Runtime state of a button.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ButtonState {
    pub hovered: bool,
    pub pressed: bool,
    pub click_count: u32,
}

/// Layout parameters shared by vertical and horizontal box containers.
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
    VBox(BoxLayout),
    HBox(BoxLayout),
    Label {
        text: String,
        font_size: f32,
        color: Color,
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
        matches!(self, Self::VBox(_) | Self::HBox(_))
    }

    pub fn is_button(&self) -> bool {
        matches!(self, Self::Button(_))
    }

    /// Intrinsic minimum size derived from content (text or padding).
    pub fn content_min_size(&self) -> Size {
        match self {
            Self::Panel { .. } | Self::VBox(_) | Self::HBox(_) => Size::ZERO,
            Self::Label {
                text, font_size, ..
            } => estimate_text_size(text, *font_size),
            Self::Button(button) => {
                let text = estimate_text_size(&button.text, button.font_size);
                Size::new(text.width + 32.0, text.height.max(36.0) + 12.0)
            }
        }
    }

    pub fn text(&self) -> Option<&str> {
        match self {
            Self::Label { text, .. } => Some(text),
            Self::Button(button) => Some(&button.text),
            _ => None,
        }
    }

    pub fn set_text(&mut self, new_text: impl Into<String>) {
        match self {
            Self::Label { text, .. } => *text = new_text.into(),
            Self::Button(button) => button.text = new_text.into(),
            _ => {}
        }
    }
}

/// Rough text size estimate (no font shaping in MVP).
///
/// Width assumes an average glyph advance of `0.6 * font_size`.
pub fn estimate_text_size(text: &str, font_size: f32) -> Size {
    let glyphs = text.chars().count() as f32;
    Size::new(glyphs * font_size * 0.6, font_size * 1.2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn label_and_button_min_sizes() {
        let label = Widget::Label {
            text: "Hello".into(),
            font_size: 20.0,
            color: Color::WHITE,
        };
        let size = label.content_min_size();
        assert!(size.width > 0.0 && size.height > 0.0);

        let button = Widget::Button(ButtonData::new("Click"));
        let button_size = button.content_min_size();
        assert!(button_size.width > size.width * 0.0);
        assert!(button_size.height >= 36.0);
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
