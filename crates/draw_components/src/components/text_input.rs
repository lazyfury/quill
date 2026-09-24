//! Themed text fields.
//!
//! The stack has no global focus / text-routing yet, so a [`TextInput`] renders
//! the field and shows a value the caller owns: mount the [`NodeRef`] passed to
//! [`TextInput::value_ref`] and write the buffer with
//! [`set_text`](draw_ui::set_text) each frame. Keyboard capture stays with the
//! caller — the same model the host's password prompt already uses. With
//! [`masked`](TextInput::masked) the field shows one dot per character, and an
//! empty value falls back to the placeholder.

use crate::base::{Component, Label, Spec};
use crate::NodeRef;
use draw_core::Edges;
use draw_theme::{radius, ControlSize, SurfaceLevel, TextSize, Theme, Tone};
use draw_ui::{Align, Justify, SurfaceStyle, TextOptions, Widget};

/// A compact single-line text field.
pub struct TextInput {
    spec: Spec,
    theme: &'static dyn Theme,
    value: String,
    placeholder: String,
    masked: bool,
    size: ControlSize,
    min_width: f32,
    value_ref: Option<NodeRef>,
}

impl TextInput {
    pub fn new(theme: &'static dyn Theme) -> Self {
        Self {
            spec: Spec::leaf(),
            theme,
            value: String::new(),
            placeholder: String::new(),
            masked: false,
            size: theme.default_control(),
            min_width: 0.0,
            value_ref: None,
        }
    }

    /// The initial value (update it later through [`value_ref`](Self::value_ref)).
    pub fn value(mut self, value: impl Into<String>) -> Self {
        self.value = value.into();
        self
    }

    /// Mounts the field's text node into `slot`, so the caller can rewrite the
    /// shown value each frame.
    pub fn value_ref(mut self, slot: &NodeRef) -> Self {
        self.value_ref = Some(slot.clone());
        self
    }

    /// Text shown, muted, while the value is empty.
    pub fn placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    /// Renders the value as one dot per character (passwords).
    pub fn masked(mut self, masked: bool) -> Self {
        self.masked = masked;
        self
    }

    /// A minimum field width.
    pub fn min_width(mut self, width: f32) -> Self {
        self.min_width = width;
        self
    }

    /// Forces the regular control height (overriding the theme default).
    pub fn size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }
}

impl Component for TextInput {
    fn spec(&mut self) -> &mut Spec {
        &mut self.spec
    }

    fn name(&self) -> &'static str {
        "TextInput"
    }

    fn widget(&self) -> Widget {
        Widget::Flex(
            draw_ui::FlexStyle::row()
                .align(Align::Center)
                .justify(Justify::Start)
                .gap(0.0)
                .padding(Edges::symmetric(self.theme.control_padding_x(), 0.0)),
        )
    }

    fn prepare(&mut self) {
        let theme = self.theme;
        if self.spec.data.min_size.height <= 0.0 {
            self.spec.data.min_size.height = theme.control_height(self.size);
        }
        if self.min_width > 0.0 {
            self.spec.data.min_size.width = self.min_width;
        }

        self.spec.background = Some(Box::new(move |_| {
            let palette = theme.palette();
            SurfaceStyle::new(theme.surface(SurfaceLevel::Base))
                .border(palette.border)
                .radius(radius::SM)
        }));

        let empty = self.value.is_empty();
        let (text, tone) = if empty && !self.placeholder.is_empty() {
            (self.placeholder.clone(), Tone::Muted)
        } else if self.masked {
            ("•".repeat(self.value.chars().count()), Tone::Default)
        } else {
            (self.value.clone(), Tone::Default)
        };
        let label = Label::new(text)
            .font_size(theme.font_size(TextSize::Small))
            .color(tone.color(theme))
            .text_options(TextOptions::no_wrap());
        match self.value_ref.clone() {
            Some(slot) => self.spec.child(label.ref_(&slot)),
            None => self.spec.child(label),
        }
    }
}

crate::impl_scene_child!(TextInput);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{set_text, Flex};
    use draw_core::{Size, ViewportSize};
    use draw_scene::SceneTree;
    use draw_theme::{default_theme, Mode};
    use draw_ui::{Control, MouseFilter, Widget};

    fn mount(tree: &mut SceneTree, input: TextInput) -> draw_core::NodeId {
        let page = tree.add_child(
            tree.root(),
            Flex::column()
                .gap(0.0)
                .padding(Edges::ZERO)
                .mouse_filter(MouseFilter::Ignore)
                .child(input),
        );
        tree.children(page).unwrap()[0]
    }

    /// The text of the field's label child.
    fn label_text(tree: &SceneTree, input: draw_core::NodeId) -> String {
        let child = tree.children(input).unwrap()[0];
        node_text(tree, child)
    }

    fn node_text(tree: &SceneTree, id: draw_core::NodeId) -> String {
        match tree.data::<Control>(id).map(|c| &c.widget) {
            Some(Widget::Label { text, .. }) => text.clone(),
            _ => panic!("not a label"),
        }
    }

    #[test]
    fn a_masked_field_hides_the_characters() {
        let theme = default_theme(Mode::Dark);
        let mut tree = SceneTree::new();
        let id = mount(
            &mut tree,
            TextInput::new(theme).value("hunter2").masked(true),
        );
        draw_ui::layout(&mut tree, ViewportSize::new(Size::new(400.0, 300.0)));
        assert_eq!(label_text(&tree, id), "•••••••");
    }

    #[test]
    fn an_empty_field_shows_its_placeholder() {
        let theme = default_theme(Mode::Dark);
        let mut tree = SceneTree::new();
        let id = mount(&mut tree, TextInput::new(theme).placeholder("密码"));
        draw_ui::layout(&mut tree, ViewportSize::new(Size::new(400.0, 300.0)));
        assert_eq!(label_text(&tree, id), "密码");
    }

    #[test]
    fn the_value_ref_receives_the_text_node() {
        let theme = default_theme(Mode::Dark);
        let slot = NodeRef::default();
        let mut tree = SceneTree::new();
        mount(&mut tree, TextInput::new(theme).value_ref(&slot));
        draw_ui::layout(&mut tree, ViewportSize::new(Size::new(400.0, 300.0)));
        let label = slot.get().expect("value node mounted");
        set_text(&mut tree, label, "typed");
        assert_eq!(node_text(&tree, label), "typed");
    }
}
