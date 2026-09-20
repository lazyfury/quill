//! Themed buttons.

use crate::base::{Component, Label, Spec};
use draw_core::{Color, Edges, Size};
use draw_theme::{control, radius, TextSize, Theme};
use draw_ui::{Align, Justify, SurfaceStyle, TextOptions, Widget};

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
    spec: Spec,
    theme: Theme,
    text: String,
    variant: ButtonVariant,
    font_size: f32,
    on_click: Option<Box<dyn FnMut()>>,
}

impl Button {
    pub fn new(text: impl Into<String>, theme: Theme) -> Self {
        Self {
            spec: Spec::leaf(),
            theme,
            text: text.into(),
            variant: ButtonVariant::Secondary,
            font_size: TextSize::Small.px(),
            on_click: None,
        }
    }

    pub fn primary(text: impl Into<String>, theme: Theme) -> Self {
        Self::new(text, theme).variant(ButtonVariant::Primary)
    }

    pub fn secondary(text: impl Into<String>, theme: Theme) -> Self {
        Self::new(text, theme).variant(ButtonVariant::Secondary)
    }

    pub fn ghost(text: impl Into<String>, theme: Theme) -> Self {
        Self::new(text, theme).variant(ButtonVariant::Ghost)
    }

    pub fn destructive(text: impl Into<String>, theme: Theme) -> Self {
        Self::new(text, theme).variant(ButtonVariant::Destructive)
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
    fn spec(&mut self) -> &mut Spec {
        &mut self.spec
    }

    fn name(&self) -> &'static str {
        "Button"
    }

    fn widget(&self) -> Widget {
        Widget::Flex(
            draw_ui::FlexStyle::row()
                .align(Align::Center)
                .justify(Justify::Center)
                .gap(0.0)
                .padding(Edges::symmetric(control::PADDING_X, 0.0)),
        )
    }

    fn prepare(&mut self) {
        let theme = self.theme;
        let variant = self.variant;

        self.spec.data.min_size = Size::new(0.0, control::HEIGHT);
        self.spec.background = Some(Box::new(move |st| {
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
        }));

        let color = match self.variant {
            ButtonVariant::Primary | ButtonVariant::Destructive => theme.palette.on_accent,
            _ => theme.palette.foreground,
        };
        let text = self.text.clone();
        let font_size = self.font_size;
        self.spec.child(
            Label::new(text)
                .font_size(font_size)
                .color(color)
                .text_options(TextOptions::no_wrap()),
        );
        self.spec.on_click = self.on_click.take();
    }
}

crate::impl_scene_child!(Button);
