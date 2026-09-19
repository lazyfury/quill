//! `draw_ui` — controls, layout, containers and input.
//!
//! Built on the `draw_scene` tree: a [`Ui`] owns a [`SceneTree`] of `Control`
//! nodes plus per-control layout ([`ControlData`]) and behavior ([`Widget`]).
//!
//! - **Layout** resolves absolute rectangles from anchors/offsets; containers
//!   (VBox/HBox) position their children.
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
mod ui;
mod widget;

pub use component::{Button, Component, ControlRef, HBox, Label, Panel, VBox};
pub use control::{ControlData, MouseFilter};
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
}
