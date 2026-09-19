//! `draw_ui` — controls, layout, containers and input.
//!
//! Built on the `draw_scene` tree: a [`Ui`] owns a [`SceneTree`] of `Control`
//! nodes plus per-control layout ([`ControlData`]) and behavior ([`Widget`]).
//!
//! - **Layout** resolves absolute rectangles from anchors/offsets; flex and
//!   grid containers size and arrange their children (see [`layout`]).
//! - **Painting** emits a backend-neutral [`draw_render::DrawList`].
//! - **Input** hit-tests topmost `Control`s and routes pointer/keyboard events
//!   (target dispatch in MVP; capture/bubble is a future extension).
//!
//! This crate never touches browser APIs, so all of the above is testable with
//! native `cargo test`.

/// Crate name, kept for lightweight smoke checks.
pub const CRATE: &str = "draw_ui";

mod component;
mod control;
mod debug;
mod decor;
pub mod layout;
mod paint;
mod tone;
mod ui;
mod widget;

pub use component::{Button, Component, ControlRef, Flex, Grid, HBox, Label, Panel, VBox};
pub use control::{ControlData, MouseFilter};
pub use debug::DebugDrawOptions;
pub use decor::{
    dynamic_surface_decor, foreground_decor, surface_decor, DecorRef, InteractState, NodeDecor,
};
pub use layout::{
    Align, AlignContent, ApproxTextMeasurer, ContentSize, FixedWidthTextMeasurer, FlexDirection,
    FlexStyle, GridPlacement, GridStyle, Justify, LayoutStyle, SizeBasis, TextMeasurer,
    TextOptions, Track,
};
pub use paint::{fill_rounded_rect, fill_rounded_rect_corners, inset, surface, SurfaceStyle};
pub use tone::{SurfaceTone, Tone};
pub use ui::{ClickCallback, Ui};
pub use widget::{estimate_text_size, BoxLayout, ButtonData, ButtonState, Widget};

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::rc::Rc;

    use draw_core::{InputEvent, Key, PointerButton, Rect, Size, Viewport};

    fn build() -> (Ui, draw_core::NodeId, draw_core::NodeId, draw_core::NodeId) {
        let mut ui = Ui::new();
        let panel = ui.add_panel(ui.root());
        let vbox = ui.add_vbox(panel);
        let label = ui.add_label(vbox, "Hello");
        let button = ui.add_button(vbox, "Click me");
        ui.layout(Viewport::new(Size::new(800.0, 600.0)));
        (ui, panel, label, button)
    }

    fn rect(ui: &Ui, id: draw_core::NodeId) -> Rect {
        ui.control(id).unwrap().rect
    }

    fn click(ui: &mut Ui, position: draw_core::Vec2) {
        ui.handle_input(&InputEvent::PointerDown {
            position,
            button: PointerButton::Left,
        });
        ui.handle_input(&InputEvent::PointerUp {
            position,
            button: PointerButton::Left,
        });
    }

    #[test]
    fn panel_label_button_layout() {
        let (ui, panel, label, button) = build();
        assert_eq!(
            rect(&ui, panel),
            Rect::from_min_size(draw_core::Vec2::ZERO, Size::new(800.0, 600.0))
        );

        let label_rect = rect(&ui, label);
        let button_rect = rect(&ui, button);
        assert!(label_rect.top() < button_rect.top());
        assert!(panel_contains(rect(&ui, panel), button_rect));
        assert!(button_rect.size.height >= 36.0);
        // VBox stacks with the default separation
        assert!((button_rect.top() - label_rect.bottom() - 8.0).abs() < 1e-4);
    }

    fn panel_contains(panel: Rect, child: Rect) -> bool {
        child.left() >= panel.left()
            && child.top() >= panel.top()
            && child.right() <= panel.right()
            && child.bottom() <= panel.bottom()
    }

    #[test]
    fn control_count_tracks_inserted_controls() {
        let (ui, ..) = build();
        // root + panel + vbox + label + button
        assert_eq!(ui.control_count(), 5);
    }

    #[test]
    fn layout_responds_to_resize() {
        let (mut ui, panel, _label, button) = build();
        assert_eq!(rect(&ui, panel).size, Size::new(800.0, 600.0));

        ui.layout(Viewport::new(Size::new(400.0, 300.0)));
        assert_eq!(rect(&ui, panel).size, Size::new(400.0, 300.0));
        assert!(panel_contains(rect(&ui, panel), rect(&ui, button)));
    }

    #[test]
    fn hit_test_finds_topmost_button() {
        let (ui, panel, _label, button) = build();
        let button_center = rect(&ui, button).center();
        assert_eq!(ui.hit_test(button_center), Some(button));

        // A point low in the panel misses the button but still hits a control.
        let elsewhere = draw_core::Vec2::new(400.0, 590.0);
        assert_ne!(ui.hit_test(elsewhere), Some(button));
        assert!(ui.hit_test(elsewhere).is_some());
        let _ = panel;
    }

    #[test]
    fn hovered_is_button_only_for_buttons() {
        let (mut ui, _panel, label, button) = build();

        ui.handle_input(&InputEvent::PointerMove {
            position: rect(&ui, label).center(),
        });
        assert!(!ui.hovered_is_button());

        ui.handle_input(&InputEvent::PointerMove {
            position: rect(&ui, button).center(),
        });
        assert!(ui.hovered_is_button());

        ui.handle_input(&InputEvent::PointerLeave);
        assert!(!ui.hovered_is_button());
    }

    #[test]
    fn decor_paints_around_each_node_in_tree_order() {
        use std::cell::RefCell;
        use std::rc::Rc;

        use draw_core::Rect;

        struct Marker {
            name: &'static str,
            log: Rc<RefCell<Vec<&'static str>>>,
        }
        impl NodeDecor for Marker {
            fn paint_behind(
                &self,
                _ctx: &mut draw_render::PaintContext,
                _rect: Rect,
                _state: InteractState,
            ) {
                self.log.borrow_mut().push(self.name);
            }
            fn paint_front(
                &self,
                _ctx: &mut draw_render::PaintContext,
                _rect: Rect,
                _state: InteractState,
            ) {
                self.log.borrow_mut().push(self.name);
            }
        }

        let mut ui = Ui::new();
        let a = ui.add_label(ui.root(), "A");
        let b = ui.add_label(ui.root(), "B");
        let log = Rc::new(RefCell::new(Vec::new()));
        ui.add_decor(
            a,
            Rc::new(Marker {
                name: "a",
                log: log.clone(),
            }),
        );
        ui.add_decor(
            b,
            Rc::new(Marker {
                name: "b",
                log: log.clone(),
            }),
        );
        ui.layout(Viewport::new(Size::new(200.0, 200.0)));

        let mut ctx = draw_render::PaintContext::new();
        ui.paint(&mut ctx);
        // Each node's decor wraps its own content; there are no separate
        // global surface/foreground passes.
        assert_eq!(&*log.borrow(), &["a", "a", "b", "b"]);
    }

    #[test]
    fn state_and_clicks_inherit_from_ancestors() {
        use std::cell::Cell;
        use std::rc::Rc;

        use draw_core::Vec2;

        let mut ui = Ui::new();
        let row = ui.add_vbox(ui.root());
        let child = ui.add_label(row, "child");
        let clicks = Rc::new(Cell::new(0));
        let counter = clicks.clone();
        ui.set_on_click(row, move || counter.set(counter.get() + 1));
        ui.layout(Viewport::new(Size::new(200.0, 200.0)));

        let center: Vec2 = rect(&ui, child).center();
        ui.handle_input(&InputEvent::PointerMove { position: center });
        assert!(ui.state_for(row).hovered);
        assert!(ui.is_interactive(child));

        ui.handle_input(&InputEvent::PointerDown {
            position: center,
            button: PointerButton::Left,
        });
        assert!(ui.state_for(row).pressed);
        ui.handle_input(&InputEvent::PointerUp {
            position: center,
            button: PointerButton::Left,
        });
        assert_eq!(clicks.get(), 1);
    }

    #[test]
    fn mouse_filter_ignore_is_transparent() {
        let (mut ui, _panel, label, _button) = build();
        let center = rect(&ui, label).center();
        ui.set_mouse_filter(label, MouseFilter::Ignore);
        assert_ne!(ui.hit_test(center), Some(label));
    }

    #[test]
    fn click_increments_state_and_fires_callback() {
        let (mut ui, _panel, _label, button) = build();
        let count = Rc::new(Cell::new(0));
        let captured = count.clone();
        assert!(ui.set_on_click(button, move || captured.set(captured.get() + 1)));

        let center = rect(&ui, button).center();
        click(&mut ui, center);

        assert_eq!(ui.click_count(button), 1);
        assert_eq!(count.get(), 1);
        assert!(!ui.button_state(button).unwrap().pressed);
    }

    #[test]
    fn click_outside_does_not_activate() {
        let (mut ui, _panel, _label, button) = build();
        click(&mut ui, draw_core::Vec2::new(400.0, 590.0));
        assert_eq!(ui.click_count(button), 0);
    }

    #[test]
    fn keyboard_activates_focused_button() {
        let (mut ui, _panel, _label, button) = build();
        let center = rect(&ui, button).center();
        click(&mut ui, center);
        assert_eq!(ui.focused(), Some(button));

        ui.handle_input(&InputEvent::KeyDown { key: Key::Enter });
        ui.handle_input(&InputEvent::KeyDown { key: Key::Space });
        assert_eq!(ui.click_count(button), 3);
    }

    #[test]
    fn hover_state_tracks_pointer() {
        let (mut ui, _panel, _label, button) = build();
        let center = rect(&ui, button).center();
        ui.handle_input(&InputEvent::PointerMove { position: center });
        assert!(ui.button_state(button).unwrap().hovered);

        ui.handle_input(&InputEvent::PointerMove {
            position: draw_core::Vec2::new(400.0, 590.0),
        });
        assert!(!ui.button_state(button).unwrap().hovered);
    }

    #[test]
    fn paint_debug_draws_yellow_bounds_and_name_id_labels() {
        let (ui, _panel, _label, _button) = build();
        let mut ctx = draw_render::PaintContext::new();
        ui.paint_debug(&mut ctx, &DebugDrawOptions::default());
        let list = ctx.into_draw_list();

        let strokes: Vec<draw_core::Color> = list
            .commands()
            .iter()
            .filter_map(|command| match command {
                draw_render::DrawCommand::StrokeRect { paint, .. } => Some(paint.color),
                _ => None,
            })
            .collect();
        assert_eq!(strokes.len(), 5);
        assert!(strokes
            .iter()
            .all(|color| *color == draw_core::Color::YELLOW));

        let labels: Vec<&str> = list
            .commands()
            .iter()
            .filter_map(|command| match command {
                draw_render::DrawCommand::DrawText { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(labels.len(), 5);
        assert!(labels.iter().any(|text| text.starts_with("Button #")));
        assert!(labels.iter().any(|text| text.starts_with("Panel #")));
    }

    #[test]
    fn wrapped_label_paints_one_command_per_line() {
        let mut ui = Ui::new();
        let column = ui.add_flex(ui.root(), FlexStyle::column().gap(0.0));
        let _label = ui.add_label(column, "hello world hello world");
        ui.layout(Viewport::new(Size::new(120.0, 400.0)));

        let mut ctx = draw_render::PaintContext::new();
        ui.paint(&mut ctx);
        let list = ctx.into_draw_list();
        let lines = list
            .commands()
            .iter()
            .filter(|command| matches!(command, draw_render::DrawCommand::DrawText { .. }))
            .count();
        assert!(lines > 1, "expected wrapped label to emit multiple lines");
    }

    #[test]
    fn ellipsis_clips_label_to_one_line() {
        let mut ui = Ui::new();
        let column = ui.add_flex(ui.root(), FlexStyle::column().gap(0.0));
        let _label = ui.add(
            column,
            Label::new("hello world hello world")
                .max_lines(1)
                .ellipsis(true),
        );
        ui.layout(Viewport::new(Size::new(100.0, 400.0)));

        let mut ctx = draw_render::PaintContext::new();
        ui.paint(&mut ctx);
        let list = ctx.into_draw_list();
        let texts: Vec<&str> = list
            .commands()
            .iter()
            .filter_map(|command| match command {
                draw_render::DrawCommand::DrawText { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(texts.len(), 1);
        assert!(texts[0].ends_with('\u{2026}'));
    }

    fn painted_texts(ui: &Ui) -> Vec<String> {
        let mut ctx = draw_render::PaintContext::new();
        ui.paint(&mut ctx);
        let list = ctx.into_draw_list();
        list.commands()
            .iter()
            .filter_map(|command| match command {
                draw_render::DrawCommand::DrawText { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect()
    }

    fn painted_text_baselines(ui: &Ui) -> Vec<(f32, f32)> {
        let mut ctx = draw_render::PaintContext::new();
        ui.paint(&mut ctx);
        let list = ctx.into_draw_list();
        list.commands()
            .iter()
            .filter_map(|command| match command {
                draw_render::DrawCommand::DrawText {
                    position,
                    font_size,
                    ..
                } => Some((position.y, *font_size)),
                _ => None,
            })
            .collect()
    }

    /// Measurer with realistic (non-`0.8em`) ascent, as a real backend font has.
    struct RealMetrics;

    impl TextMeasurer for RealMetrics {
        fn advance(&self, _ch: char, font_size: f32) -> f32 {
            font_size * 0.5
        }

        fn line_height(&self, font_size: f32) -> f32 {
            font_size * 1.2
        }

        fn ascent(&self, font_size: f32) -> f32 {
            font_size * 0.9
        }
    }

    #[test]
    fn label_baseline_follows_measurer_ascent() {
        // A host that injects real font metrics must get baselines computed from
        // them; a hard-coded ascent leaves text vertically off-centre.
        let mut ui = Ui::new();
        ui.set_text_measurer(Rc::new(RealMetrics));
        let row = ui.add_flex(
            ui.root(),
            FlexStyle::row()
                .align(Align::Center)
                .gap(0.0)
                .padding(draw_core::Edges::ZERO),
        );
        ui.set_min_size(row, Size::new(0.0, 30.0));
        let label = ui.add_label(row, "Ag");
        ui.layout(Viewport::new(Size::new(200.0, 30.0)));

        let (baseline, font_size) = painted_text_baselines(&ui)[0];
        let ascent = RealMetrics.ascent(font_size);
        let descent = RealMetrics.line_height(font_size) - ascent;

        assert!((baseline - (rect(&ui, label).top() + ascent)).abs() < 1e-3);
        // The glyph box centre (ascent..descent around the baseline) coincides
        // with the label box centre, so a centered flex row stays centered.
        let visual_center = baseline - (ascent - descent) / 2.0;
        assert!((visual_center - rect(&ui, label).center().y).abs() < 1e-3);
    }

    #[test]
    fn swapping_measurer_recomputes_text_layout() {
        let mut ui = Ui::new();
        let column = ui.add_flex(
            ui.root(),
            FlexStyle::column().gap(0.0).padding(draw_core::Edges::ZERO),
        );
        ui.add(column, Label::new("hello world hello world"));
        let vp = Viewport::new(Size::new(120.0, 400.0));

        ui.layout(vp);
        let approx_lines = painted_texts(&ui).len();

        ui.set_text_measurer(Rc::new(FixedWidthTextMeasurer::default()));
        ui.layout(vp);
        let fixed_lines = painted_texts(&ui).len();

        assert!(fixed_lines > approx_lines);
    }

    #[test]
    fn wrapped_button_paints_multiple_lines() {
        let mut ui = Ui::new();
        let column = ui.add_flex(ui.root(), FlexStyle::column().gap(0.0));
        ui.add(column, Button::new("hello world hello world").wrap(true));
        ui.layout(Viewport::new(Size::new(100.0, 400.0)));
        assert!(painted_texts(&ui).len() > 1);
    }
}
