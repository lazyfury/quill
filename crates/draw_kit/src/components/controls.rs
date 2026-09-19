//! Interactive controls: checkbox and switch.

use std::cell::Cell;
use std::rc::Rc;

use draw_core::{Edges, NodeId, Size, Vec2};
use draw_theme::{control, radius, space, TextSize};
use draw_ui::{Align, Flex, Label, TextOptions};

use crate::paint::{self, SurfaceStyle};
use crate::{Component, ControlRef, Kit, Ui};

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
    fn mount(self, kit: &mut Kit, ui: &mut Ui, parent: NodeId) -> ControlRef {
        let state = self
            .state
            .unwrap_or_else(|| Rc::new(Cell::new(self.initial)));
        let theme = *kit.theme();

        let row = ui.add(parent, Flex::row().align(Align::Center).gap(space::SM));
        crate::detach(ui, row.id());
        ui.set_min_size(row.id(), Size::new(0.0, control::ROW_SM));

        let box_node = ui.add(row.id(), Flex::new().padding(Edges::ZERO));
        ui.set_min_size(box_node.id(), Size::new(16.0, 16.0));

        ui.add(
            row.id(),
            Label::new(self.label)
                .font_size(TextSize::Body.px())
                .color(theme.palette.foreground)
                .text_options(TextOptions::no_wrap()),
        );

        let paint_state = state.clone();
        kit.foreground(box_node.id(), move |ctx, rect, theme, st| {
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
            paint::surface(
                ctx,
                rect,
                &SurfaceStyle::new(fill).border(border).radius(radius::SM),
            );
            if checked {
                paint::fill_rounded_rect(
                    ctx,
                    paint::inset(rect, 4.0),
                    1.5,
                    theme.palette.on_accent,
                );
            }
        });

        let click_state = state;
        let mut on_change = self.on_change;
        kit.on_click(row.id(), move || {
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
    fn mount(self, kit: &mut Kit, ui: &mut Ui, parent: NodeId) -> ControlRef {
        let state = self
            .state
            .unwrap_or_else(|| Rc::new(Cell::new(self.initial)));
        let theme = *kit.theme();

        let row = ui.add(parent, Flex::row().align(Align::Center).gap(space::SM));
        crate::detach(ui, row.id());
        ui.set_min_size(row.id(), Size::new(0.0, control::ROW_SM));

        let track = ui.add(row.id(), Flex::new().padding(Edges::ZERO));
        ui.set_min_size(track.id(), Size::new(34.0, 18.0));

        if let Some(label) = self.label {
            ui.add(
                row.id(),
                Label::new(label)
                    .font_size(TextSize::Body.px())
                    .color(theme.palette.foreground)
                    .text_options(TextOptions::no_wrap()),
            );
        }

        let paint_state = state.clone();
        kit.foreground(track.id(), move |ctx, rect, theme, st| {
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
            paint::surface(
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
        });

        let click_state = state;
        let mut on_change = self.on_change;
        kit.on_click(row.id(), move || {
            let next = !click_state.get();
            click_state.set(next);
            if let Some(callback) = on_change.as_mut() {
                callback(next);
            }
        });
        row
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use draw_core::{InputEvent, PointerButton, Size as CoreSize, Viewport};
    use draw_render::DrawCommand;
    use draw_theme::Theme;

    fn click_at(kit: &mut Kit, ui: &Ui, position: Vec2) {
        kit.handle_input(
            ui,
            &InputEvent::PointerDown {
                position,
                button: PointerButton::Left,
            },
        );
        kit.handle_input(
            ui,
            &InputEvent::PointerUp {
                position,
                button: PointerButton::Left,
            },
        );
    }

    #[test]
    fn checkbox_toggles_shared_state_and_fires_callback() {
        let mut ui = Ui::new();
        let mut kit = Kit::new(Theme::dark());
        let state = Rc::new(Cell::new(false));
        let changes = Rc::new(Cell::new(0));
        let counter = changes.clone();
        let root = ui.root();
        let checkbox = kit.add(
            &mut ui,
            root,
            Checkbox::new("Verbose")
                .state(state.clone())
                .on_change(move |_| counter.set(counter.get() + 1)),
        );
        ui.layout(Viewport::new(CoreSize::new(400.0, 200.0)));

        let center = ui.control(checkbox.id()).unwrap().rect.center();
        click_at(&mut kit, &ui, center);
        assert!(state.get());
        assert_eq!(changes.get(), 1);

        click_at(&mut kit, &ui, center);
        assert!(!state.get());
        assert_eq!(changes.get(), 2);
    }

    #[test]
    fn checked_checkbox_paints_a_mark() {
        let mut ui = Ui::new();
        let mut kit = Kit::new(Theme::dark());
        let root = ui.root();
        let checkbox = kit.add(&mut ui, root, Checkbox::new("x").checked(true));
        ui.layout(Viewport::new(CoreSize::new(400.0, 200.0)));
        assert!(ui.control(checkbox.id()).is_some());

        let mut ctx = draw_render::PaintContext::new();
        kit.paint_foreground(&ui, &mut ctx);
        let list = ctx.into_draw_list();
        let rounded = list
            .commands()
            .iter()
            .filter(|c| matches!(c, DrawCommand::FillRoundedRect { .. }))
            .count();
        // The box fill plus the inner check mark.
        assert_eq!(rounded, 2, "box fill and check mark are rounded rects");
        let strokes = list
            .commands()
            .iter()
            .filter(|c| matches!(c, DrawCommand::StrokeRoundedRect { .. }))
            .count();
        assert_eq!(strokes, 1, "box border is a rounded stroke");
    }

    #[test]
    fn switch_toggles_state() {
        let mut ui = Ui::new();
        let mut kit = Kit::new(Theme::light());
        let state = Rc::new(Cell::new(false));
        let root = ui.root();
        let switch = kit.add(
            &mut ui,
            root,
            Switch::new().label("Enabled").state(state.clone()),
        );
        ui.layout(Viewport::new(CoreSize::new(400.0, 200.0)));
        let center = ui.control(switch.id()).unwrap().rect.center();
        click_at(&mut kit, &ui, center);
        assert!(state.get());
    }

    #[test]
    fn switch_paints_a_knob() {
        let mut ui = Ui::new();
        let mut kit = Kit::new(Theme::light());
        let root = ui.root();
        let switch = kit.add(&mut ui, root, Switch::new().on(true));
        ui.layout(Viewport::new(CoreSize::new(400.0, 200.0)));
        assert!(ui.control(switch.id()).is_some());
        let mut ctx = draw_render::PaintContext::new();
        kit.paint_foreground(&ui, &mut ctx);
        let list = ctx.into_draw_list();
        let circles = list
            .commands()
            .iter()
            .filter(|c| matches!(c, DrawCommand::FillCircle { .. }))
            .count();
        // Only the knob is a circle now; the track is a rounded surface.
        assert_eq!(circles, 1);
        assert!(list
            .commands()
            .iter()
            .any(|c| matches!(c, DrawCommand::FillRoundedRect { .. })));
    }
}
