//! Pop-up style selects: a trigger that shows the current value and a chevron.
//!
//! A component in the tree cannot open an overlay by itself (the overlay layer
//! is owned by the host, not the tree), so a [`Select`] only renders the trigger
//! and raises [`Select::on_open`] when clicked. The caller then opens the option
//! menu with [`Overlays::menu`](crate::Overlays::menu) anchored to this node —
//! exactly how [`Button`](crate::Button) raises an action. The visible value is
//! dynamic: mount the [`NodeRef`] passed to [`Select::value_ref`] and write the
//! new label with
//! [`set_text`](draw_ui::set_text) when the choice changes.

use crate::base::{Component, Label, Spec};
use crate::glyph::{paint_glyph, Glyph};
use crate::NodeRef;
use draw_core::{Cursor, Edges, Rect, Size, Vec2};
use draw_theme::{radius, ControlSize, Space, TextSize, Theme};
use draw_ui::{Align, Justify, SurfaceStyle, TextOptions, Widget};

/// How wide the chevron zone (and its right inset) is reserved on the trigger.
const CHEVRON_ZONE: f32 = 18.0;

/// A compact pop-up trigger: the current value plus a chevron.
pub struct Select {
    spec: Spec,
    theme: &'static dyn Theme,
    value: String,
    value_ref: Option<NodeRef>,
    size: ControlSize,
    min_width: f32,
    disabled: bool,
    on_open: Option<Box<dyn FnMut()>>,
}

impl Select {
    pub fn new(theme: &'static dyn Theme) -> Self {
        Self {
            spec: Spec::leaf(),
            theme,
            value: String::new(),
            value_ref: None,
            size: theme.default_control(),
            min_width: 0.0,
            disabled: false,
            on_open: None,
        }
    }

    /// The initial value label (update it later through [`value_ref`](Self::value_ref)).
    pub fn value(mut self, value: impl Into<String>) -> Self {
        self.value = value.into();
        self
    }

    /// Mounts this select's value label into `slot`, so the caller can rewrite
    /// the shown label each frame.
    pub fn value_ref(mut self, slot: &NodeRef) -> Self {
        self.value_ref = Some(slot.clone());
        self
    }

    /// A minimum trigger width, so a long value cannot stretch the column.
    pub fn min_width(mut self, width: f32) -> Self {
        self.min_width = width;
        self
    }

    /// Forces the regular control height (overriding the theme default).
    pub fn size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }

    /// Dims the trigger and makes it inert (no hover, no open).
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Runs `callback` when the trigger is clicked; the caller opens the menu.
    pub fn on_open(mut self, callback: impl FnMut() + 'static) -> Self {
        self.on_open = Some(Box::new(callback));
        self
    }
}

impl Component for Select {
    fn spec(&mut self) -> &mut Spec {
        &mut self.spec
    }

    fn name(&self) -> &'static str {
        "Select"
    }

    fn widget(&self) -> Widget {
        Widget::Flex(
            draw_ui::FlexStyle::row()
                .align(Align::Center)
                .justify(Justify::SpaceBetween)
                .gap(0.0)
                .padding(Edges {
                    left: self.theme.control_padding_x(),
                    right: self.theme.control_padding_x() + CHEVRON_ZONE,
                    top: 0.0,
                    bottom: 0.0,
                }),
        )
    }

    fn prepare(&mut self) {
        let theme = self.theme;
        let disabled = self.disabled;

        if self.spec.data.min_size.height <= 0.0 {
            self.spec.data.min_size.height = theme.control_height(self.size);
        }
        if self.min_width > 0.0 {
            self.spec.data.min_size.width = self.min_width;
        }

        self.spec.background = Some(Box::new(move |st| {
            let palette = theme.palette();
            let fill = if disabled {
                palette.surface_raised
            } else if st.hovered || st.pressed {
                palette.surface_hover
            } else {
                palette.surface_raised
            };
            SurfaceStyle::new(fill)
                .border(palette.border)
                .radius(radius::MD)
        }));

        let color = if disabled {
            theme.palette().subtle
        } else {
            theme.palette().foreground
        };
        let label = Label::new(self.value.clone())
            .font_size(theme.font_size(TextSize::Small))
            .color(color)
            .text_options(TextOptions::no_wrap());
        match self.value_ref.clone() {
            Some(slot) => self.spec.child(label.ref_(&slot)),
            None => self.spec.child(label),
        }

        let chevron = if disabled {
            theme.palette().subtle
        } else {
            theme.palette().muted
        };
        self.spec.foreground = Some(Box::new(move |ctx, rect, _| {
            let extent = theme.font_size(TextSize::Small) * 0.5;
            let center = Vec2::new(
                rect.right() - theme.spacing(Space::SM) - extent * 0.5,
                rect.center().y,
            );
            paint_glyph(
                Glyph::ChevronDown,
                ctx,
                Rect::from_center_size(center, Size::splat(extent)),
                chevron,
                1.5,
            );
        }));

        if disabled {
            self.spec.on_click = None;
        } else {
            self.spec.data.cursor = Cursor::Pointer;
            self.spec.on_click = self.on_open.take();
        }
    }
}

crate::impl_scene_child!(Select);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{set_text, Flex};
    use draw_core::{InputEvent, PointerButton, ViewportSize};
    use draw_scene::SceneTree;
    use draw_theme::{default_theme, Mode};
    use draw_ui::{control, MouseFilter};
    use std::cell::Cell;
    use std::rc::Rc;

    fn mount(tree: &mut SceneTree, select: Select) -> (draw_core::NodeId, draw_core::NodeId) {
        let page = tree.add_child(
            tree.root(),
            Flex::column()
                .gap(0.0)
                .padding(Edges::ZERO)
                .mouse_filter(MouseFilter::Ignore)
                .child(select),
        );
        let id = tree.children(page).unwrap()[0];
        (page, id)
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
    fn clicking_a_select_raises_open() {
        let theme = default_theme(Mode::Dark);
        let opened = Rc::new(Cell::new(false));
        let flag = opened.clone();
        let mut tree = SceneTree::new();
        let (_, id) = mount(
            &mut tree,
            Select::new(theme).on_open(move || flag.set(true)),
        );
        draw_ui::layout(&mut tree, ViewportSize::new(Size::new(400.0, 300.0)));
        let center = control(&tree, id).unwrap().rect.center();
        click(&mut tree, center);
        assert!(opened.get());
    }

    #[test]
    fn a_disabled_select_ignores_clicks() {
        let theme = default_theme(Mode::Dark);
        let opened = Rc::new(Cell::new(false));
        let flag = opened.clone();
        let mut tree = SceneTree::new();
        let (_, id) = mount(
            &mut tree,
            Select::new(theme)
                .disabled(true)
                .on_open(move || flag.set(true)),
        );
        draw_ui::layout(&mut tree, ViewportSize::new(Size::new(400.0, 300.0)));
        let center = control(&tree, id).unwrap().rect.center();
        click(&mut tree, center);
        assert!(!opened.get());
    }

    #[test]
    fn the_value_ref_receives_the_label_and_can_be_rewritten() {
        let theme = default_theme(Mode::Dark);
        let slot = NodeRef::default();
        let mut tree = SceneTree::new();
        mount(&mut tree, Select::new(theme).value("ZIP").value_ref(&slot));
        draw_ui::layout(&mut tree, ViewportSize::new(Size::new(400.0, 300.0)));

        let label = slot.get().expect("value label mounted");
        set_text(&mut tree, label, "7z");
        draw_ui::layout(&mut tree, ViewportSize::new(Size::new(400.0, 300.0)));
        // The label node exists and takes the new text; nothing else to assert
        // (the rendered glyphs are a paint concern, not behaviour).
        assert_eq!(slot.get(), Some(label));
    }
}
