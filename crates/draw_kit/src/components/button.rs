//! Themed buttons.

use draw_core::{Color, Edges, NodeId, Size};
use draw_theme::{control, radius, TextSize};
use draw_ui::{estimate_text_size, Label, Panel, TextOptions};

use crate::paint::SurfaceStyle;
use crate::{detach, Component, ControlRef, Kit, Ui};

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
    fn mount(self, kit: &mut Kit, ui: &mut Ui, parent: NodeId) -> ControlRef {
        let theme = *kit.theme();
        let font = self.font_size;
        let pad = control::PADDING_X;
        let text_size = estimate_text_size(&self.text, font);
        let size = Size::new(
            text_size.width + pad * 2.0,
            text_size.height.max(control::HEIGHT),
        );

        let node = ui.add(parent, Panel::new().color(Color::TRANSPARENT).flat());
        detach(ui, node.id());
        ui.set_min_size(node.id(), size);

        let variant = self.variant;
        kit.dynamic_surface(node.id(), move |theme, st| {
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
            }
        });

        let color = match variant {
            ButtonVariant::Primary => theme.palette.on_accent,
            _ => theme.palette.foreground,
        };
        let label = ui.add(
            node.id(),
            Label::new(self.text)
                .font_size(font)
                .color(color)
                .text_options(TextOptions::no_wrap()),
        );
        ui.set_anchors(label.id(), Edges::new(0.0, 0.5, 1.0, 0.5));
        ui.set_offsets(label.id(), Edges::new(pad, -font * 0.72, -pad, font * 0.72));

        if let Some(callback) = self.on_click {
            kit.on_click(node.id(), callback);
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
        let mut kit = Kit::new(Theme::dark());
        let clicks = Rc::new(Cell::new(0));
        let counter = clicks.clone();
        let root = ui.root();
        let button = kit.add(
            &mut ui,
            root,
            Button::primary("Save").on_click(move || counter.set(counter.get() + 1)),
        );
        ui.layout(Viewport::new(CoreSize::new(400.0, 200.0)));
        let center = ui.control(button.id()).unwrap().rect.center();
        kit.handle_input(
            &ui,
            &InputEvent::PointerDown {
                position: center,
                button: PointerButton::Left,
            },
        );
        kit.handle_input(
            &ui,
            &InputEvent::PointerUp {
                position: center,
                button: PointerButton::Left,
            },
        );
        assert_eq!(clicks.get(), 1);
        let rect = ui.control(button.id()).unwrap().rect;
        assert!(rect.size.height >= control::HEIGHT);
    }
}
