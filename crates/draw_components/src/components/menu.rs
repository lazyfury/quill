//! Themed menus: a floating [`Menu`] surface and its [`MenuItem`] rows.
//!
//! A menu is built like any other component and, in practice, placed in the
//! overlay layer ([`Overlays::menu`](crate::Overlays::menu)) so it paints on top
//! of the UI and closes on Escape / a click outside:
//!
//! ```ignore
//! use draw_components::{Menu, MenuItem};
//!
//! overlays.menu(button, move |tree, node| {
//!     tree.add_child(node, Menu::new(theme)
//!         .item(MenuItem::new("Undo", theme).shortcut("Ctrl+Z").on_click(undo))
//!         .separator()
//!         .item(MenuItem::new("About", theme).on_click(about)));
//! });
//! ```
//!
//! The surface owns its width (`Menu::min_width`, default 200px) and stretches
//! each row, so items line up regardless of label length.

use crate::base::{Component, Label, Spec};
use draw_core::{Cursor, Edges, Size};
use draw_theme::{radius, Space, SurfaceLevel, TextSize, Theme, Tone};
use draw_ui::{Align, Justify, SurfaceStyle, TextOptions, Widget};

/// Default minimum width of a menu surface (logical pixels).
pub const MENU_MIN_WIDTH: f32 = 200.0;

/// One clickable row in a [`Menu`].
pub struct MenuItem {
    spec: Spec,
    theme: Theme,
    label: String,
    shortcut: Option<String>,
    tone: Tone,
    disabled: bool,
    on_click: Option<Box<dyn FnMut()>>,
}

impl MenuItem {
    /// A row with the default (foreground) tone.
    pub fn new(label: impl Into<String>, theme: Theme) -> Self {
        Self {
            spec: Spec::leaf(),
            theme,
            label: label.into(),
            shortcut: None,
            tone: Tone::Default,
            disabled: false,
            on_click: None,
        }
    }

    /// A row with a right-aligned shortcut hint (e.g. `"Ctrl+Z"`).
    pub fn action(label: impl Into<String>, shortcut: impl Into<String>, theme: Theme) -> Self {
        Self::new(label, theme).shortcut(shortcut)
    }

    /// Sets the right-aligned shortcut hint.
    pub fn shortcut(mut self, shortcut: impl Into<String>) -> Self {
        self.shortcut = Some(shortcut.into());
        self
    }

    /// Overrides the label tone.
    pub fn tone(mut self, tone: Tone) -> Self {
        self.tone = tone;
        self
    }

    /// Renders the label in the error tone (destructive actions).
    pub fn destructive(mut self) -> Self {
        self.tone = Tone::Error;
        self
    }

    /// Disables the row: dimmed, no hover, no click.
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Runs `callback` when the row is clicked.
    pub fn on_click(mut self, callback: impl FnMut() + 'static) -> Self {
        self.on_click = Some(Box::new(callback));
        self
    }
}

impl Component for MenuItem {
    fn spec(&mut self) -> &mut Spec {
        &mut self.spec
    }

    fn name(&self) -> &'static str {
        "MenuItem"
    }

    fn widget(&self) -> Widget {
        Widget::Flex(
            draw_ui::FlexStyle::row()
                .align(Align::Center)
                .justify(Justify::SpaceBetween)
                .gap(self.theme.spacing(Space::LG))
                .padding(Edges::symmetric(self.theme.control_padding_x(), 0.0)),
        )
    }

    fn prepare(&mut self) {
        let theme = self.theme;
        let disabled = self.disabled;

        self.spec.data.min_size = Size::new(0.0, theme.row_height());
        // Dimmed rows never highlight; enabled ones light up on hover / press.
        self.spec.background = Some(Box::new(move |st| {
            let fill = if disabled || !(st.hovered || st.pressed) {
                draw_core::Color::TRANSPARENT
            } else {
                theme.palette.surface_hover
            };
            SurfaceStyle::new(fill).radius(radius::SM)
        }));
        if !disabled {
            self.spec.data.cursor = Cursor::Pointer;
        }

        let color = if disabled {
            theme.palette.subtle
        } else {
            self.tone.color(&theme)
        };
        self.spec.child(
            Label::new(self.label.clone())
                .font_size(TextSize::Small.px())
                .color(color)
                .text_options(TextOptions::no_wrap()),
        );
        if let Some(shortcut) = self.shortcut.clone() {
            let shortcut_color = if disabled {
                theme.palette.subtle
            } else {
                theme.palette.muted
            };
            self.spec.child(
                Label::new(shortcut)
                    .font_size(TextSize::Caption.px())
                    .color(shortcut_color)
                    .text_options(TextOptions::no_wrap()),
            );
        }

        if !disabled {
            self.spec.on_click = self.on_click.take();
        }
    }
}

/// A row in a [`Menu`]: an item or a hairline separator.
enum Row {
    Item(MenuItem),
    Separator,
}

/// A floating menu surface: a vertical stack of [`MenuItem`]s with a themed
/// background, border and radius.
///
/// Use it standalone or as the content of [`Overlays::menu`](crate::Overlays::menu).
pub struct Menu {
    spec: Spec,
    theme: Theme,
    rows: Vec<Row>,
    min_width: f32,
}

impl Menu {
    pub fn new(theme: Theme) -> Self {
        Self {
            spec: Spec::leaf(),
            theme,
            rows: Vec::new(),
            min_width: MENU_MIN_WIDTH,
        }
    }

    /// Appends one clickable row.
    pub fn item(mut self, item: MenuItem) -> Self {
        self.rows.push(Row::Item(item));
        self
    }

    /// Appends several rows.
    pub fn items<I: IntoIterator<Item = MenuItem>>(mut self, items: I) -> Self {
        for item in items {
            self.rows.push(Row::Item(item));
        }
        self
    }

    /// Appends a hairline separator.
    pub fn separator(mut self) -> Self {
        self.rows.push(Row::Separator);
        self
    }

    /// Overrides the minimum surface width (default [`MENU_MIN_WIDTH`]).
    pub fn min_width(mut self, min_width: f32) -> Self {
        self.min_width = min_width;
        self
    }
}

impl Component for Menu {
    fn spec(&mut self) -> &mut Spec {
        &mut self.spec
    }

    fn name(&self) -> &'static str {
        "Menu"
    }

    fn widget(&self) -> Widget {
        Widget::Flex(
            draw_ui::FlexStyle::column()
                .gap(self.theme.spacing(Space::XXXS))
                .padding(Edges::all(self.theme.spacing(Space::XS))),
        )
    }

    fn prepare(&mut self) {
        let theme = self.theme;
        let style = SurfaceStyle::new(theme.surface(SurfaceLevel::Floating))
            .border(theme.palette.border)
            .radius(radius::LG);
        self.spec.data.min_size = Size::new(self.min_width, 0.0);
        self.spec.background = Some(Box::new(move |_| style));

        for row in std::mem::take(&mut self.rows) {
            match row {
                Row::Item(item) => self.spec.child(item),
                Row::Separator => self.spec.child(crate::Divider::horizontal(theme)),
            }
        }
    }
}

crate::impl_scene_child!(Menu, MenuItem);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::base::Flex;
    use draw_core::{InputEvent, PointerButton, Vec2, ViewportSize};
    use draw_scene::SceneTree;
    use draw_ui::{control, MouseFilter};
    use std::cell::Cell;
    use std::rc::Rc;

    fn viewport() -> ViewportSize {
        ViewportSize::new(Size::new(400.0, 300.0))
    }

    fn click(tree: &mut SceneTree, position: Vec2) {
        draw_ui::handle_input(
            tree,
            &InputEvent::PointerDown {
                position,
                button: PointerButton::Left,
            },
        );
        draw_ui::handle_input(
            tree,
            &InputEvent::PointerUp {
                position,
                button: PointerButton::Left,
            },
        );
    }

    /// Mounts `menu` in a filling column and returns the menu's node.
    fn mount_menu(tree: &mut SceneTree, menu: Menu) -> draw_core::NodeId {
        let root = tree.root();
        let page = tree.add_child(
            root,
            Flex::column()
                .gap(0.0)
                .padding(Edges::ZERO)
                .mouse_filter(MouseFilter::Ignore)
                .child(menu),
        );
        tree.children(page).unwrap()[0]
    }

    #[test]
    fn menu_items_stack_and_the_surface_is_at_least_min_width() {
        let theme = Theme::dark();
        let mut tree = SceneTree::new();
        let menu = mount_menu(
            &mut tree,
            Menu::new(theme)
                .item(MenuItem::new("Undo", theme))
                .separator()
                .item(MenuItem::new("Redo", theme)),
        );
        draw_ui::layout(&mut tree, viewport());

        let menu_rect = control(&tree, menu).unwrap().rect;
        assert!(menu_rect.size.width >= MENU_MIN_WIDTH - 1e-3);
        // item + separator + item
        assert_eq!(tree.children(menu).unwrap().len(), 3);
    }

    #[test]
    fn clicking_a_row_runs_its_callback() {
        let theme = Theme::dark();
        let clicked = Rc::new(Cell::new(false));
        let flag = clicked.clone();
        let mut tree = SceneTree::new();
        let root = tree.root();
        let id = MenuItem::new("Undo", theme)
            .on_click(move || flag.set(true))
            .build(&mut tree, root);
        draw_ui::layout(&mut tree, viewport());
        let center = control(&tree, id).unwrap().rect.center();
        click(&mut tree, center);
        assert!(clicked.get());
    }

    #[test]
    fn a_disabled_row_ignores_clicks_and_keeps_the_default_cursor() {
        let theme = Theme::dark();
        let clicked = Rc::new(Cell::new(false));
        let flag = clicked.clone();
        let mut tree = SceneTree::new();
        let root = tree.root();
        let id = MenuItem::new("Undo", theme)
            .disabled(true)
            .on_click(move || flag.set(true))
            .build(&mut tree, root);
        draw_ui::layout(&mut tree, viewport());
        assert_eq!(control(&tree, id).unwrap().cursor, Cursor::Default);
        let center = control(&tree, id).unwrap().rect.center();
        click(&mut tree, center);
        assert!(!clicked.get());
    }

    #[test]
    fn an_enabled_row_shows_a_pointer_cursor() {
        let theme = Theme::dark();
        let mut tree = SceneTree::new();
        let root = tree.root();
        let id = MenuItem::new("Undo", theme).build(&mut tree, root);
        draw_ui::layout(&mut tree, viewport());
        assert_eq!(control(&tree, id).unwrap().cursor, Cursor::Pointer);
    }
}
