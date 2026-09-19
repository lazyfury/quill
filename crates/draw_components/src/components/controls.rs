//! Interactive controls: checkbox and switch.

use std::cell::Cell;
use std::rc::Rc;

use draw_app::{Flex, Label};
use draw_core::{Edges, NodeId, Size, Vec2};
use draw_scene::SceneTree;
use draw_theme::{control, radius, space, TextSize};
use draw_ui::{Align, TextOptions};

use crate::{Component, ControlRef};
use draw_ui::foreground_decor;
use draw_ui::{fill_rounded_rect, inset, surface, SurfaceStyle};

/// A compact checkbox with a label.
///
/// State lives in an `Rc<Cell<bool>>` so the check mark repaints without
/// remounting. Pass an external handle with [`Checkbox::state`] to read it.
pub struct Checkbox {
    label: String,
    initial: bool,
    state: Option<Rc<Cell<bool>>>,
    on_change: Option<Box<dyn FnMut(bool)>>,
}

impl Checkbox {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            initial: false,
            state: None,
            on_change: None,
        }
    }

    pub fn checked(mut self, checked: bool) -> Self {
        self.initial = checked;
        self
    }

    /// Shares state with the caller (e.g. to read the value after a click).
    pub fn state(mut self, state: Rc<Cell<bool>>) -> Self {
        self.state = Some(state);
        self
    }

    pub fn on_change(mut self, callback: impl FnMut(bool) + 'static) -> Self {
        self.on_change = Some(Box::new(callback));
        self
    }
}

impl Component for Checkbox {
    fn mount(self, tree: &mut SceneTree, parent: NodeId) -> ControlRef {
        let state = self
            .state
            .unwrap_or_else(|| Rc::new(Cell::new(self.initial)));
        let theme = draw_ui::theme(tree);

        let row = draw_app::add(
            tree,
            parent,
            Flex::row().align(Align::Center).gap(space::SM),
        );
        crate::detach(tree, row.id());
        draw_app::update_control(tree, row.id(), |d| {
            d.min_size = Size::new(0.0, control::ROW_SM)
        });

        let box_node = draw_app::add(tree, row.id(), Flex::new().padding(Edges::ZERO));
        draw_app::update_control(tree, box_node.id(), |d| d.min_size = Size::new(16.0, 16.0));

        draw_app::add(
            tree,
            row.id(),
            Label::new(self.label)
                .font_size(TextSize::Body.px())
                .color(theme.palette.foreground)
                .text_options(TextOptions::no_wrap()),
        );

        let paint_state = state.clone();
        draw_ui::add_decor(
            tree,
            box_node.id(),
            foreground_decor(theme, move |ctx, rect, theme, st| {
                let checked = paint_state.get();
                let fill = if checked {
                    theme.palette.accent
                } else {
                    theme.palette.background
                };
                let border = if checked || st.hovered {
                    theme.palette.accent
                } else {
                    theme.palette.border
                };
                surface(
                    ctx,
                    rect,
                    &SurfaceStyle::new(fill).border(border).radius(radius::SM),
                );
                if checked {
                    fill_rounded_rect(ctx, inset(rect, 4.0), 1.5, theme.palette.on_accent);
                }
            }),
        );

        let click_state = state;
        let mut on_change = self.on_change;
        draw_app::set_on_click(tree, row.id(), move || {
            let next = !click_state.get();
            click_state.set(next);
            if let Some(callback) = on_change.as_mut() {
                callback(next);
            }
        });
        row
    }
}

/// A compact on/off switch.
pub struct Switch {
    label: Option<String>,
    initial: bool,
    state: Option<Rc<Cell<bool>>>,
    on_change: Option<Box<dyn FnMut(bool)>>,
}

impl Switch {
    pub fn new() -> Self {
        Self {
            label: None,
            initial: false,
            state: None,
            on_change: None,
        }
    }

    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn on(mut self, on: bool) -> Self {
        self.initial = on;
        self
    }

    pub fn state(mut self, state: Rc<Cell<bool>>) -> Self {
        self.state = Some(state);
        self
    }

    pub fn on_change(mut self, callback: impl FnMut(bool) + 'static) -> Self {
        self.on_change = Some(Box::new(callback));
        self
    }
}

impl Default for Switch {
    fn default() -> Self {
        Self::new()
    }
}

impl Component for Switch {
    fn mount(self, tree: &mut SceneTree, parent: NodeId) -> ControlRef {
        let state = self
            .state
            .unwrap_or_else(|| Rc::new(Cell::new(self.initial)));
        let theme = draw_ui::theme(tree);

        let row = draw_app::add(
            tree,
            parent,
            Flex::row().align(Align::Center).gap(space::SM),
        );
        crate::detach(tree, row.id());
        draw_app::update_control(tree, row.id(), |d| {
            d.min_size = Size::new(0.0, control::ROW_SM)
        });

        let track = draw_app::add(tree, row.id(), Flex::new().padding(Edges::ZERO));
        draw_app::update_control(tree, track.id(), |d| d.min_size = Size::new(34.0, 18.0));

        if let Some(label) = self.label {
            draw_app::add(
                tree,
                row.id(),
                Label::new(label)
                    .font_size(TextSize::Body.px())
                    .color(theme.palette.foreground)
                    .text_options(TextOptions::no_wrap()),
            );
        }

        let paint_state = state.clone();
        draw_ui::add_decor(
            tree,
            track.id(),
            foreground_decor(theme, move |ctx, rect, theme, st| {
                let on = paint_state.get();
                let track_color = if on {
                    theme.palette.accent
                } else if st.hovered {
                    theme.palette.surface_hover
                } else {
                    theme.palette.surface_raised
                };
                let border = if on {
                    theme.palette.accent
                } else {
                    theme.palette.border
                };
                surface(
                    ctx,
                    rect,
                    &SurfaceStyle::new(track_color)
                        .border(border)
                        .radius(radius::FULL),
                );

                let r = 6.5;
                let inset = 1.5;
                let cx = if on {
                    rect.right() - r - inset
                } else {
                    rect.left() + r + inset
                };
                ctx.fill_circle(
                    Vec2::new(cx, rect.center().y),
                    r,
                    if on {
                        theme.palette.on_accent
                    } else {
                        theme.palette.muted
                    },
                );
            }),
        );

        let click_state = state;
        let mut on_change = self.on_change;
        draw_app::set_on_click(tree, row.id(), move || {
            let next = !click_state.get();
            click_state.set(next);
            if let Some(callback) = on_change.as_mut() {
                callback(next);
            }
        });
        row
    }
}
