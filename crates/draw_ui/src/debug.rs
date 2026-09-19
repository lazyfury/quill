//! Debug drawing of UI controls (yellow bounds + `name#id` labels).
//!
//! [`DebugDrawOptions`] is the style used by
//! [`Ui::paint_debug`](crate::Ui::paint_debug): a yellow rectangle around every
//! visible control plus a `Name #id` label pinned to its top-left corner. It
//! emits ordinary backend-neutral [`DrawCommand`](draw_render::DrawCommand)s, so
//! every backend can render it.

use draw_core::{Color, NodeId, Vec2};

/// Style for [`Ui::paint_debug`](crate::Ui::paint_debug).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DebugDrawOptions {
    /// Border color (default: yellow).
    pub border_color: Color,
    /// Label color (default: yellow).
    pub text_color: Color,
    /// Border thickness in logical pixels.
    pub width: f32,
    /// Label font size.
    pub font_size: f32,
    /// Offset of the label from the control's top-left corner, in logical
    /// pixels (`y` is measured to the text baseline).
    pub label_offset: Vec2,
    /// Include the control name in the label.
    pub show_names: bool,
    /// Include the control id in the label.
    pub show_ids: bool,
}

impl Default for DebugDrawOptions {
    fn default() -> Self {
        Self {
            border_color: Color::YELLOW,
            text_color: Color::YELLOW,
            width: 1.0,
            font_size: 11.0,
            label_offset: Vec2::new(2.0, 0.0),
            show_names: true,
            show_ids: true,
        }
    }
}

impl DebugDrawOptions {
    /// Builds the label for one control, honoring [`show_names`](Self::show_names)
    /// and [`show_ids`](Self::show_ids). Empty when both are disabled.
    pub fn label(&self, name: &str, id: NodeId) -> String {
        match (self.show_names, self.show_ids) {
            (true, true) => format!("{name} #{}", id.index()),
            (true, false) => name.to_string(),
            (false, true) => format!("#{}", id.index()),
            (false, false) => String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_yellow_names_and_ids() {
        let options = DebugDrawOptions::default();
        assert_eq!(options.border_color, Color::YELLOW);
        assert_eq!(options.text_color, Color::YELLOW);
        assert!(options.show_names && options.show_ids);
    }

    #[test]
    fn label_combinations() {
        let id = NodeId::new(7, 0);
        let both = DebugDrawOptions::default();
        assert_eq!(both.label("Button", id), "Button #7");

        let names_only = DebugDrawOptions {
            show_ids: false,
            ..DebugDrawOptions::default()
        };
        assert_eq!(names_only.label("Button", id), "Button");

        let ids_only = DebugDrawOptions {
            show_names: false,
            ..DebugDrawOptions::default()
        };
        assert_eq!(ids_only.label("Button", id), "#7");

        let none = DebugDrawOptions {
            show_names: false,
            show_ids: false,
            ..DebugDrawOptions::default()
        };
        assert_eq!(none.label("Button", id), "");
    }
}
