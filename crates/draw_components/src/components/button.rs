//! Themed buttons.

use draw_core::{Color, Edges, NodeId, Size};
use draw_theme::{control, radius, TextSize};
use draw_ui::{Align, Flex, Justify, Label, TextOptions};

use crate::{detach, Component, ControlRef, Ui};
use draw_ui::dynamic_surface_decor;
use draw_ui::SurfaceStyle;

/// Visual weight of a button.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ButtonVariant {
    /// Solid accent background; the single primary action.
    Primary,
    /// Surface background with a hairline border (default).
    #[default]
    Secondary,
    /// Transparent until hovered.
    Ghost,
    /// Solid error background for destructive actions.
    Destructive,
}

/// A compact, themed button.
pub struct Button {
    text: String,
    variant: ButtonVariant,
    font_size: f32,
    on_click: Option<Box<dyn FnMut()>>,
}

impl Button {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            variant: ButtonVariant::Secondary,
            font_size: TextSize::Small.px(),
            on_click: None,
        }
    }

    pub fn primary(text: impl Into<String>) -> Self {
        Self::new(text).variant(ButtonVariant::Primary)
    }

    pub fn secondary(text: impl Into<String>) -> Self {
        Self::new(text).variant(ButtonVariant::Secondary)
    }

    pub fn ghost(text: impl Into<String>) -> Self {
        Self::new(text).variant(ButtonVariant::Ghost)
    }

    pub fn destructive(text: impl Into<String>) -> Self {
        Self::new(text).variant(ButtonVariant::Destructive)
    }

    pub fn variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self
    }

    pub fn font_size(mut self, font_size: f32) -> Self {
        self.font_size = font_size;
        self
    }

    pub fn on_click(mut self, callback: impl FnMut() + 'static) -> Self {
        self.on_click = Some(Box::new(callback));
        self
    }
}

impl Component for Button {
    fn mount(self, ui: &mut Ui, parent: NodeId) -> ControlRef {
        let theme = ui.theme();
        let font = self.font_size;
        let pad = control::PADDING_X;

        // A centered row: the label is intrinsically sized by the active text
        // measurer and centred both ways, so the button re-measures when the
        // host swaps the measurer (e.g. for a real font).
        let node = ui.add(
            parent,
            Flex::row()
                .align(Align::Center)
                .justify(Justify::Center)
                .gap(0.0)
                .padding(Edges::symmetric(pad, 0.0)),
        );
        detach(ui, node.id());
        ui.set_min_size(node.id(), Size::new(0.0, control::HEIGHT));

        let variant = self.variant;
        ui.add_decor(
            node.id(),
            dynamic_surface_decor(theme, move |theme, st| {
                let palette = &theme.palette;
                match variant {
                    ButtonVariant::Primary => {
                        let fill = if st.pressed {
                            palette.accent.lerp(Color::BLACK, 0.12)
                        } else if st.hovered {
                            palette.accent.lerp(palette.foreground, 0.10)
                        } else {
                            palette.accent
                        };
                        SurfaceStyle::new(fill).radius(radius::MD)
                    }
                    ButtonVariant::Secondary => {
                        let fill = if st.hovered || st.pressed {
                            palette.surface_hover
                        } else {
                            palette.surface_raised
                        };
                        SurfaceStyle::new(fill)
                            .border(palette.border)
                            .radius(radius::MD)
                    }
                    ButtonVariant::Ghost => {
                        let fill = if st.hovered || st.pressed {
                            palette.surface_hover
                        } else {
                            Color::TRANSPARENT
                        };
                        SurfaceStyle::new(fill).radius(radius::MD)
                    }
                    ButtonVariant::Destructive => {
                        let fill = if st.pressed {
                            palette.error.lerp(Color::BLACK, 0.12)
                        } else if st.hovered {
                            palette.error.lerp(palette.foreground, 0.10)
                        } else {
                            palette.error
                        };
                        SurfaceStyle::new(fill).radius(radius::MD)
                    }
                }
            }),
        );
        let color = match variant {
            ButtonVariant::Primary | ButtonVariant::Destructive => theme.palette.on_accent,
            _ => theme.palette.foreground,
        };
        ui.add(
            node.id(),
            Label::new(self.text)
                .font_size(font)
                .color(color)
                .text_options(TextOptions::no_wrap()),
        );

        if let Some(callback) = self.on_click {
            ui.set_on_click(node.id(), callback);
        }
        node
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::rc::Rc;

    use draw_core::{InputEvent, PointerButton, Size as CoreSize, Viewport};
    use draw_theme::Theme;

    #[test]
    fn primary_button_is_clickable_and_fires_callback() {
        let mut ui = Ui::new();
        ui.set_theme(Theme::dark());
        let clicks = Rc::new(Cell::new(0));
        let counter = clicks.clone();
        let root = ui.root();
        let button = ui.add(
            root,
            Button::primary("Save").on_click(move || counter.set(counter.get() + 1)),
        );
        ui.layout(Viewport::new(CoreSize::new(400.0, 200.0)));
        let center = ui.control(button.id()).unwrap().rect.center();
        ui.handle_input(&InputEvent::PointerDown {
            position: center,
            button: PointerButton::Left,
        });
        ui.handle_input(&InputEvent::PointerUp {
            position: center,
            button: PointerButton::Left,
        });
        assert_eq!(clicks.get(), 1);
        let rect = ui.control(button.id()).unwrap().rect;
        assert!(rect.size.height >= control::HEIGHT);
    }
}
