//! Interactive controls: checkbox and switch.

use std::cell::Cell;
use std::rc::Rc;

use crate::base::{Component, Flex, Label, Spec};
use crate::glyph::{paint_glyph, Glyph};
use draw_core::{Edges, Size, Vec2};
use draw_theme::{radius, Space, TextSize, Theme};
use draw_ui::{Align, SurfaceStyle, TextOptions, Widget};

/// A compact checkbox with a label.
///
/// State lives in an `Rc<Cell<bool>>` so the check mark repaints without
/// remounting. Pass an external handle with [`Checkbox::state`] to read it.
pub struct Checkbox {
    spec: Spec,
    theme: &'static dyn Theme,
    label: String,
    initial: bool,
    state: Option<Rc<Cell<bool>>>,
    on_change: Option<Box<dyn FnMut(bool)>>,
}

impl Checkbox {
    pub fn new(label: impl Into<String>, theme: &'static dyn Theme) -> Self {
        Self {
            spec: Spec::leaf(),
            theme,
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
    fn spec(&mut self) -> &mut Spec {
        &mut self.spec
    }

    fn name(&self) -> &'static str {
        "Checkbox"
    }

    fn widget(&self) -> Widget {
        Widget::Flex(
            draw_ui::FlexStyle::row()
                .align(Align::Center)
                .padding(Edges::ZERO)
                .gap(self.theme.spacing(Space::SM)),
        )
    }

    fn prepare(&mut self) {
        let theme = self.theme;
        let state = self
            .state
            .clone()
            .unwrap_or_else(|| Rc::new(Cell::new(self.initial)));
        // Fill the row by default, but let a caller pin a smaller height (a
        // virtualized list row is shorter than the theme's row height).
        if self.spec.data.min_size.height <= 0.0 {
            self.spec.data.min_size.height = theme.row_height();
        }

        let paint_state = state.clone();
        self.spec.child(
            Flex::new()
                .padding(Edges::ZERO)
                .anchors(Edges::ZERO)
                .offsets(Edges::ZERO)
                .min_size(16.0, 16.0)
                .foreground(move |ctx, rect, st| {
                    let checked = paint_state.get();
                    let fill = if checked {
                        theme.palette().accent
                    } else {
                        theme.palette().background
                    };
                    let border = if checked || st.hovered {
                        theme.palette().accent
                    } else {
                        theme.palette().border
                    };
                    draw_ui::surface(
                        ctx,
                        rect,
                        &SurfaceStyle::new(fill).border(border).radius(radius::SM),
                    );
                    if checked {
                        paint_glyph(
                            Glyph::Check,
                            ctx,
                            draw_ui::inset(rect, 2.0),
                            theme.palette().on_accent,
                            1.8,
                        );
                    }
                }),
        );

        let label = self.label.clone();
        let body = TextSize::Body.px();
        let foreground = theme.palette().foreground;
        self.spec.child(
            Label::new(label)
                .font_size(body)
                .color(foreground)
                .text_options(TextOptions::no_wrap()),
        );

        let click_state = state;
        let mut on_change = self.on_change.take();
        self.spec.on_click = Some(Box::new(move || {
            let next = !click_state.get();
            click_state.set(next);
            if let Some(callback) = on_change.as_mut() {
                callback(next);
            }
        }));
    }
}

/// A compact on/off switch.
pub struct Switch {
    spec: Spec,
    theme: &'static dyn Theme,
    label: Option<String>,
    initial: bool,
    state: Option<Rc<Cell<bool>>>,
    on_change: Option<Box<dyn FnMut(bool)>>,
}

impl Switch {
    pub fn new(theme: &'static dyn Theme) -> Self {
        Self {
            spec: Spec::leaf(),
            theme,
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

impl Component for Switch {
    fn spec(&mut self) -> &mut Spec {
        &mut self.spec
    }

    fn name(&self) -> &'static str {
        "Switch"
    }

    fn widget(&self) -> Widget {
        Widget::Flex(
            draw_ui::FlexStyle::row()
                .align(Align::Center)
                .gap(self.theme.spacing(Space::SM)),
        )
    }

    fn prepare(&mut self) {
        let theme = self.theme;
        let state = self
            .state
            .clone()
            .unwrap_or_else(|| Rc::new(Cell::new(self.initial)));
        self.spec.data.min_size = Size::new(0.0, theme.row_height());

        let paint_state = state.clone();
        self.spec.child(
            Flex::new()
                .padding(Edges::ZERO)
                .anchors(Edges::ZERO)
                .offsets(Edges::ZERO)
                .min_size(34.0, 18.0)
                .foreground(move |ctx, rect, st| {
                    let on = paint_state.get();
                    let track_color = if on {
                        theme.palette().accent
                    } else if st.hovered {
                        theme.palette().surface_hover
                    } else {
                        theme.palette().surface_raised
                    };
                    let border = if on {
                        theme.palette().accent
                    } else {
                        theme.palette().border
                    };
                    draw_ui::surface(
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
                            theme.palette().on_accent
                        } else {
                            theme.palette().muted
                        },
                    );
                }),
        );

        if let Some(label) = self.label.clone() {
            let body = TextSize::Body.px();
            let foreground = theme.palette().foreground;
            self.spec.child(
                Label::new(label)
                    .font_size(body)
                    .color(foreground)
                    .text_options(TextOptions::no_wrap()),
            );
        }

        let click_state = state;
        let mut on_change = self.on_change.take();
        self.spec.on_click = Some(Box::new(move || {
            let next = !click_state.get();
            click_state.set(next);
            if let Some(callback) = on_change.as_mut() {
                callback(next);
            }
        }));
    }
}

crate::impl_scene_child!(Checkbox, Switch);

#[cfg(test)]
mod tests {
    use super::*;
    use draw_core::ViewportSize;
    use draw_render::{DrawCommand, PaintContext};
    use draw_scene::SceneTree;
    use draw_theme::{default_theme, Mode};

    /// Paints one mounted control and returns the emitted commands.
    fn painted(component: impl Component + 'static) -> Vec<DrawCommand> {
        let mut tree = SceneTree::new();
        tree.add_child(
            tree.root(),
            Flex::column()
                .gap(0.0)
                .padding(Edges::ZERO)
                .child(component),
        );
        draw_ui::layout(&mut tree, ViewportSize::new(Size::new(240.0, 48.0)));
        let mut ctx = PaintContext::new();
        draw_ui::paint(&tree, &mut ctx);
        ctx.into_draw_list().commands().to_vec()
    }

    #[test]
    fn a_checked_box_draws_the_check_mark() {
        let theme = default_theme(Mode::Dark);
        let commands = painted(Checkbox::new("Done", theme).checked(true));
        assert!(
            commands
                .iter()
                .any(|command| matches!(command, DrawCommand::Line { .. })),
            "the checked box should stroke a check mark"
        );
    }

    #[test]
    fn an_unchecked_box_draws_no_check_mark() {
        let theme = default_theme(Mode::Dark);
        let commands = painted(Checkbox::new("Todo", theme));
        assert!(
            !commands
                .iter()
                .any(|command| matches!(command, DrawCommand::Line { .. })),
            "the empty box should not stroke a check mark"
        );
    }
}
