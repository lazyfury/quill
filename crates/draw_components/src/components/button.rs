//! Themed buttons.

use crate::base::{set_text, Component, Label, Spec};
use draw_core::{Color, Edges, FontWeight, NodeId};
use draw_scene::SceneTree;
use draw_theme::{radius, ControlSize, TextSize, Theme};
use draw_ui::{Align, Control, Justify, SurfaceStyle, TextOptions, Widget};

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
    /// Overrides the variant's label color when set.
    text_color: Option<Color>,
    /// Dimmed and inert: no hover, no click, muted label.
    disabled: bool,
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
            text_color: None,
            disabled: false,
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

    /// Overrides the label color (default: the variant's label color).
    pub fn text_color(mut self, color: Color) -> Self {
        self.text_color = Some(color);
        self
    }

    /// Dims the button and makes it inert: the label uses the muted tone, hover
    /// and press are ignored, and the click callback never runs. Available state
    /// still drives the cursor.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
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
                .padding(Edges::symmetric(self.theme.control_padding_x(), 0.0)),
        )
    }

    fn prepare(&mut self) {
        let theme = self.theme;
        let variant = self.variant;
        let disabled = self.disabled;
        self.spec.data.disabled = disabled;

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
                // A disabled button keeps a neutral resting surface and never
                // reacts to hover / press (runtime state, not just build time).
                if st.disabled || disabled {
                    let fill = match variant {
                        ButtonVariant::Ghost => Color::TRANSPARENT,
                        _ => palette.surface_raised,
                    };
                    return SurfaceStyle::new(fill)
                        .border(palette.border)
                        .radius(radius::MD);
                }
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

        let color = if disabled {
            theme.palette().subtle
        } else {
            self.text_color.unwrap_or(match self.variant {
                ButtonVariant::Primary | ButtonVariant::Destructive => theme.palette().on_accent,
                _ => theme.palette().foreground,
            })
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
        // A disabled button drops its callback, so the click can never run.
        self.spec.on_click = if disabled { None } else { self.on_click.take() };
    }
}

crate::impl_scene_child!(Button);

/// Enables / disables a themed [`Button`] at runtime.
///
/// Sets the control's disabled flag — so hover is ignored, the cursor stays
/// default and a click never fires — and recolors its label child between
/// `enabled_color` and `disabled_color`. (The build-time
/// [`Button::disabled`] covers the static case; this covers a state that
/// changes while the button stays mounted.)
pub fn set_disabled(
    tree: &mut SceneTree,
    id: NodeId,
    disabled: bool,
    enabled_color: Color,
    disabled_color: Color,
) {
    let changed = match tree.data_mut::<Control>(id) {
        Some(control) => {
            let changed = control.data.disabled != disabled;
            control.data.disabled = disabled;
            changed
        }
        None => return,
    };
    if !changed {
        return;
    }
    if let Some(children) = tree.children(id).map(|children| children.to_vec()) {
        for child in children {
            if let Some(control) = tree.data_mut::<Control>(child) {
                if let Widget::Label { color, .. } = &mut control.widget {
                    *color = if disabled {
                        disabled_color
                    } else {
                        enabled_color
                    };
                }
            }
        }
    }
    draw_ui::mark_dirty(tree, id);
}

/// Replaces a themed [`Button`]'s label text at runtime.
///
/// A button builds its label as a child node in `prepare`, so there is no
/// `NodeRef` for it. This finds that label child and rewrites it, marking the
/// tree dirty. Returns whether a label child was found (and rewritten).
pub fn set_button_text(tree: &mut SceneTree, id: NodeId, text: impl Into<String>) -> bool {
    let Some(children) = tree.children(id).map(|children| children.to_vec()) else {
        return false;
    };
    let text = text.into();
    for child in children {
        if matches!(
            tree.data::<Control>(child).map(|control| &control.widget),
            Some(Widget::Label { .. })
        ) {
            set_text(tree, child, text);
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::base::Flex;
    use draw_core::{InputEvent, PointerButton, Size, Vec2, ViewportSize};
    use draw_render::{DrawCommand, PaintContext};
    use draw_scene::SceneTree;
    use draw_theme::{compact_theme, default_theme, DefaultTheme, Mode, Palette};
    use draw_ui::{control, Control, MouseFilter};
    use std::cell::Cell;
    use std::rc::Rc;

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

    fn click(tree: &mut SceneTree, position: Vec2) {
        for event in [
            InputEvent::PointerDown {
                position,
                button: PointerButton::Left,
            },
            InputEvent::PointerUp {
                position,
                button: PointerButton::Left,
            },
        ] {
            draw_ui::handle_input(tree, &event);
        }
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

    #[test]
    fn a_disabled_button_ignores_clicks() {
        let theme = default_theme(Mode::Dark);
        let clicked = Rc::new(Cell::new(false));
        let flag = clicked.clone();
        let mut tree = SceneTree::new();
        let id = column(
            &mut tree,
            Button::new("A", theme)
                .disabled(true)
                .on_click(move || flag.set(true)),
        );
        draw_ui::layout(&mut tree, ViewportSize::new(Size::new(400.0, 300.0)));
        let center = control(&tree, id).unwrap().rect.center();
        click(&mut tree, center);
        assert!(!clicked.get(), "a disabled button must not fire");
    }

    #[test]
    fn an_enabled_button_still_fires() {
        let theme = default_theme(Mode::Dark);
        let clicked = Rc::new(Cell::new(false));
        let flag = clicked.clone();
        let mut tree = SceneTree::new();
        let id = column(
            &mut tree,
            Button::new("A", theme).on_click(move || flag.set(true)),
        );
        draw_ui::layout(&mut tree, ViewportSize::new(Size::new(400.0, 300.0)));
        let center = control(&tree, id).unwrap().rect.center();
        click(&mut tree, center);
        assert!(clicked.get(), "the control case must still fire");
    }

    #[test]
    fn runtime_disabling_blocks_clicks_and_restores() {
        let theme = default_theme(Mode::Dark);
        let clicks = Rc::new(Cell::new(0));
        let counter = clicks.clone();
        let mut tree = SceneTree::new();
        let id = column(
            &mut tree,
            Button::new("A", theme).on_click(move || counter.set(counter.get() + 1)),
        );
        let viewport = ViewportSize::new(Size::new(400.0, 300.0));
        draw_ui::layout(&mut tree, viewport);
        let center = control(&tree, id).unwrap().rect.center();
        click(&mut tree, center);
        assert_eq!(clicks.get(), 1);

        let muted = Color::new(0.5, 0.5, 0.5, 1.0);
        set_disabled(&mut tree, id, true, Color::WHITE, muted);
        draw_ui::layout(&mut tree, viewport);
        click(&mut tree, center);
        assert_eq!(clicks.get(), 1, "a disabled button must not fire");

        set_disabled(&mut tree, id, false, Color::WHITE, muted);
        draw_ui::layout(&mut tree, viewport);
        click(&mut tree, center);
        assert_eq!(clicks.get(), 2, "re-enabling restores the click");
    }

    #[test]
    fn runtime_relabelling_rewrites_the_label_child() {
        let theme = default_theme(Mode::Dark);
        let mut tree = SceneTree::new();
        let id = column(&mut tree, Button::new("Start", theme).child(Flex::row()));
        draw_ui::layout(&mut tree, ViewportSize::new(Size::new(400.0, 300.0)));

        assert!(set_button_text(&mut tree, id, "Stop"));

        let label = tree.children(id).unwrap().into_iter().find_map(|child| {
            match tree.data::<Control>(*child).map(|data| &data.widget) {
                Some(Widget::Label { text, .. }) => Some(text.clone()),
                _ => None,
            }
        });
        assert_eq!(label.as_deref(), Some("Stop"));
    }
}
