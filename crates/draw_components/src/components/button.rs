//! Themed buttons.

use crate::base::{Component, Label, Spec};
use draw_core::{Color, Edges, FontWeight};
use draw_theme::{radius, ControlSize, TextSize, Theme};
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
    theme: &'static dyn Theme,
    text: String,
    variant: ButtonVariant,
    size: ControlSize,
    font_size: f32,
    weight: FontWeight,
    on_click: Option<Box<dyn FnMut()>>,
}

impl Button {
    pub fn new(text: impl Into<String>, theme: &'static dyn Theme) -> Self {
        Self {
            spec: Spec::leaf(),
            theme,
            text: text.into(),
            variant: ButtonVariant::Secondary,
            size: theme.default_control(),
            font_size: theme.font_size(TextSize::Small),
            weight: FontWeight::NORMAL,
            on_click: None,
        }
    }

    pub fn primary(text: impl Into<String>, theme: &'static dyn Theme) -> Self {
        Self::new(text, theme).variant(ButtonVariant::Primary)
    }

    pub fn secondary(text: impl Into<String>, theme: &'static dyn Theme) -> Self {
        Self::new(text, theme).variant(ButtonVariant::Secondary)
    }

    pub fn ghost(text: impl Into<String>, theme: &'static dyn Theme) -> Self {
        Self::new(text, theme).variant(ButtonVariant::Ghost)
    }

    pub fn destructive(text: impl Into<String>, theme: &'static dyn Theme) -> Self {
        Self::new(text, theme).variant(ButtonVariant::Destructive)
    }

    pub fn variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self
    }

    /// A compact (mini) button; the height comes from the theme density.
    pub fn mini(mut self) -> Self {
        self.size = ControlSize::Mini;
        self
    }

    /// Forces the regular control height (overriding the theme default).
    pub fn regular(mut self) -> Self {
        self.size = ControlSize::Regular;
        self
    }

    /// Overrides the control size.
    pub fn size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }

    pub fn font_size(mut self, font_size: f32) -> Self {
        self.font_size = font_size;
        self
    }

    /// Sets the label weight (regular or bold).
    pub fn weight(mut self, weight: FontWeight) -> Self {
        self.weight = weight;
        self
    }

    /// Shorthand for [`weight`](Self::weight)`(`[`FontWeight::BOLD`]`)`.
    pub fn bold(self) -> Self {
        self.weight(FontWeight::BOLD)
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
                .padding(Edges::symmetric(self.theme.control_padding_x(), 0.0)),
        )
    }

    fn prepare(&mut self) {
        let theme = self.theme;
        let variant = self.variant;

        // The theme provides the default height; an explicit `min_size` from the
        // caller (e.g. the toolbar's 32x28 icon buttons) wins.
        if self.spec.data.min_size.height <= 0.0 {
            self.spec.data.min_size.height = theme.control_height(self.size);
        }
        // The variant provides the default surface, but a caller's explicit
        // `background` / `dynamic_background` (e.g. the toolbar's active-tool
        // highlight) wins.
        if self.spec.background.is_none() {
            self.spec.background = Some(Box::new(move |st| {
                let palette = theme.palette();
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
        }

        let color = match self.variant {
            ButtonVariant::Primary | ButtonVariant::Destructive => theme.palette().on_accent,
            _ => theme.palette().foreground,
        };
        let text = self.text.clone();
        let font_size = self.font_size;
        self.spec.child(
            Label::new(text)
                .font_size(font_size)
                .color(color)
                .weight(self.weight)
                .text_options(TextOptions::no_wrap()),
        );
        self.spec.on_click = self.on_click.take();
    }
}

crate::impl_scene_child!(Button);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::base::Flex;
    use draw_core::{Size, ViewportSize};
    use draw_render::{DrawCommand, PaintContext};
    use draw_scene::SceneTree;
    use draw_theme::{compact_theme, default_theme, DefaultTheme, Mode, Palette};
    use draw_ui::{control, Control, MouseFilter};

    fn column(tree: &mut SceneTree, button: Button) -> draw_core::NodeId {
        let root = tree.root();
        let page = tree.add_child(
            root,
            Flex::column()
                .gap(0.0)
                .padding(Edges::ZERO)
                .mouse_filter(MouseFilter::Ignore)
                .child(button),
        );
        tree.children(page).unwrap()[0]
    }

    #[test]
    fn a_compact_theme_makes_the_default_button_mini() {
        let mut tree = SceneTree::new();
        let comfortable = column(&mut tree, Button::new("A", default_theme(Mode::Dark)));
        let compact = column(&mut tree, Button::new("B", compact_theme(Mode::Dark)));
        draw_ui::layout(&mut tree, ViewportSize::new(Size::new(400.0, 300.0)));

        let tall = control(&tree, comfortable).unwrap().rect.size.height;
        let short = control(&tree, compact).unwrap().rect.size.height;
        assert!(short < tall, "{short} should be shorter than {tall}");
    }

    #[test]
    fn an_explicit_background_wins_over_the_variant() {
        // The toolbar highlights the active tool with `.dynamic_background(..)`;
        // `Button::prepare` must not overwrite it with the Ghost default.
        let mut tree = SceneTree::new();
        let page = tree.add_child(
            tree.root(),
            Flex::column()
                .gap(0.0)
                .padding(Edges::ZERO)
                .mouse_filter(MouseFilter::Ignore)
                .child(Button::ghost("A", default_theme(Mode::Dark)).background(Color::RED)),
        );
        let _ = page;
        draw_ui::layout(&mut tree, ViewportSize::new(Size::new(400.0, 300.0)));
        let mut ctx = PaintContext::new();
        draw_ui::paint(&tree, &mut ctx);
        let red = ctx
            .into_draw_list()
            .commands()
            .iter()
            .any(|command| {
                matches!(command, DrawCommand::FillRoundedRect { paint, .. } if paint.color == Color::RED)
            });
        assert!(red, "the caller background should be painted");
    }

    #[test]
    fn an_explicit_min_size_wins_over_the_theme_height() {
        // The toolbar's icon buttons pass `min_size(32, 28)`; the compact theme's
        // mini height must not clobber it.
        let mut tree = SceneTree::new();
        let id = column(
            &mut tree,
            Button::new("A", compact_theme(Mode::Dark)).min_size(32.0, 28.0),
        );
        draw_ui::layout(&mut tree, ViewportSize::new(Size::new(400.0, 300.0)));
        let rect = control(&tree, id).unwrap().rect;
        assert!(rect.size.height >= 28.0, "height was {}", rect.size.height);
    }

    #[test]
    fn regular_overrides_the_compact_default() {
        let mut tree = SceneTree::new();
        let compact = compact_theme(Mode::Dark);
        let forced = column(&mut tree, Button::new("A", compact).regular());
        draw_ui::layout(&mut tree, ViewportSize::new(Size::new(400.0, 300.0)));
        let height = control(&tree, forced).unwrap().rect.size.height;
        assert!(height >= compact.control_height(ControlSize::Regular));
        assert!(height > compact.control_height(ControlSize::Mini));
    }

    #[test]
    fn a_theme_can_scale_the_button_label() {
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
        let mut tree = SceneTree::new();
        let id = column(&mut tree, Button::new("A", theme));
        let label = tree.children(id).unwrap().into_iter().find_map(|child| {
            match tree.data::<Control>(*child).map(|data| &data.widget) {
                Some(Widget::Label { font_size, .. }) => Some(*font_size),
                _ => None,
            }
        });
        assert_eq!(label, Some(TextSize::Small.px() * 2.0));
    }
}
