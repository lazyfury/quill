//! `draw_app` — the application layer on top of `draw_ui`.
//!
//! `draw_ui` owns layout and paint; the [`SceneTree`] owns composition. This
//! crate supplies the pieces an application needs around them:
//!
//! - **Components** — the [`Component`] trait plus [`Flex`], [`Panel`],
//!   [`Label`], [`Button`] and [`Grid`]. Components compose with `.child()`
//!   and attach to the scene with
//!   [`SceneTree::add_child`](draw_scene::SceneTree::add_child).
//! - **Input** — [`hit_test`] / [`handle_input`] / [`route_input`] run the GUI
//!   hit-test and the `_input -> world -> GUI -> _unhandled_input` order.
//! - **Runtime** — [`App`] binds a tree and submits a frame to a
//!   [`RenderBackend`](draw_render::RenderBackend); layout/paint live in
//!   `draw_ui`.
//!
//! The theme is a plain value: components receive concrete colors, and nothing
//! reads a theme from the tree.
//!
//! ```ignore
//! use draw_app::{Flex, Label};
//! use draw_scene::SceneTree;
//!
//! let mut tree = SceneTree::new();
//! let root = tree.root();
//! tree.add_child(root, Flex::column().child(Label::new("Hi")));
//! ```

/// Crate name, kept for lightweight smoke checks.
pub const CRATE: &str = "draw_app";

mod app;
mod component;
mod input;

pub use app::App;
pub use component::{
    apply_spec, control_mut, set_on_click, set_on_drag, set_text, update_control, Button, ChildFn,
    Column, Component, Flex, Grid, HBox, Label, Panel, Row, Spec, VBox,
};
pub use input::{
    focused, handle_input, hit_test, hovered, hovered_cursor, hovered_is_button, is_interactive,
    route_input,
};

use draw_core::NodeId;
use draw_scene::SceneTree;
use draw_ui::{ButtonState, Control, Widget};

/// Runtime state of a button control.
pub fn button_state(tree: &SceneTree, id: NodeId) -> Option<ButtonState> {
    match tree.data::<Control>(id).map(|control| &control.widget) {
        Some(Widget::Button(button)) => Some(button.state),
        _ => None,
    }
}

/// Number of times a button control has been activated.
pub fn click_count(tree: &SceneTree, id: NodeId) -> u32 {
    button_state(tree, id).map_or(0, |state| state.click_count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::rc::Rc;

    use draw_core::{
        Edges, EventResult, InputEvent, PointerButton, Rect, Size, Vec2, ViewportSize,
    };
    use draw_scene::Visual;
    use draw_ui::{MouseFilter, SizeBasis};

    fn viewport(w: f32, h: f32) -> ViewportSize {
        ViewportSize::new(Size::new(w, h))
    }

    fn host() -> (SceneTree, NodeId) {
        let mut tree = SceneTree::new();
        let root = tree.root();
        let root = tree.add_child(root, Flex::column().mouse_filter(MouseFilter::Ignore));
        (tree, root)
    }

    fn click(tree: &mut SceneTree, position: Vec2) {
        handle_input(
            tree,
            &InputEvent::PointerDown {
                position,
                button: PointerButton::Left,
            },
        );
        handle_input(
            tree,
            &InputEvent::PointerUp {
                position,
                button: PointerButton::Left,
            },
        );
    }

    #[test]
    fn build_layout_and_paint() {
        let (mut tree, root) = host();
        let panel = tree.add_child(
            root,
            Panel::new().child(
                VBox::new()
                    .child(Label::new("Hello"))
                    .child(Button::new("Click me")),
            ),
        );
        let vbox = tree.children(panel).unwrap()[0];
        let label = tree.children(vbox).unwrap()[0];
        let button = tree.children(vbox).unwrap()[1];
        draw_ui::layout(&mut tree, viewport(800.0, 600.0));
        tree.update();

        assert_eq!(
            draw_ui::control(&tree, panel).unwrap().rect.size,
            Size::new(800.0, 600.0)
        );
        assert!(
            draw_ui::control(&tree, label).unwrap().rect.top()
                < draw_ui::control(&tree, button).unwrap().rect.top()
        );

        let mut ctx = draw_render::PaintContext::new();
        draw_ui::paint(&tree, &mut ctx);
        let list = ctx.into_draw_list();
        assert!(list
            .commands()
            .iter()
            .any(|c| matches!(c, draw_render::DrawCommand::DrawText { .. })));
    }

    #[test]
    fn flex_grow_distributes_leftover() {
        let (mut tree, root) = host();
        let row = tree.add_child(root, Flex::row().gap(0.0).padding(draw_core::Edges::ZERO));
        let a = tree.add_child(row, Panel::new().basis(SizeBasis::Px(100.0)).shrink(0.0));
        let b = tree.add_child(row, Panel::new().basis(SizeBasis::Px(100.0)).grow(1.0));
        draw_ui::layout(&mut tree, viewport(300.0, 100.0));
        assert_eq!(draw_ui::control(&tree, a).unwrap().rect.size.width, 100.0);
        assert_eq!(draw_ui::control(&tree, b).unwrap().rect.size.width, 200.0);
    }

    #[test]
    fn click_fires_callback_and_focus() {
        let (mut tree, root) = host();
        let clicks = Rc::new(Cell::new(0));
        let counter = clicks.clone();
        let button = tree.add_child(
            root,
            Button::new("Click me").on_click(move || counter.set(counter.get() + 1)),
        );
        draw_ui::layout(&mut tree, viewport(400.0, 200.0));
        tree.update();

        let center = draw_ui::control(&tree, button).unwrap().rect.center();
        click(&mut tree, center);
        assert_eq!(clicks.get(), 1);
        assert_eq!(click_count(&tree, button), 1);
        assert_eq!(focused(&tree), Some(button));
    }

    #[test]
    fn drag_callback_accumulates_deltas_and_captures_the_pointer() {
        let (mut tree, root) = host();
        let handle = tree.add_child(
            root,
            Panel::new()
                .anchors(Edges::ZERO)
                .offsets(Edges::new(0.0, 0.0, 20.0, 20.0)),
        );
        let total = Rc::new(Cell::new(0.0f32));
        let acc = total.clone();
        set_on_drag(&mut tree, handle, move |_tree, delta| {
            acc.set(acc.get() + delta.x);
        });
        draw_ui::layout(&mut tree, viewport(200.0, 200.0));
        tree.update();

        handle_input(
            &mut tree,
            &InputEvent::PointerDown {
                position: Vec2::new(10.0, 10.0),
                button: PointerButton::Left,
            },
        );
        // A move inside the handle, then one far outside it: pointer capture
        // must keep routing to the handle.
        handle_input(
            &mut tree,
            &InputEvent::PointerMove {
                position: Vec2::new(20.0, 10.0),
            },
        );
        handle_input(
            &mut tree,
            &InputEvent::PointerMove {
                position: Vec2::new(60.0, 180.0),
            },
        );
        handle_input(
            &mut tree,
            &InputEvent::PointerUp {
                position: Vec2::new(60.0, 180.0),
                button: PointerButton::Left,
            },
        );

        assert!((total.get() - 50.0).abs() < 1e-3, "total = {}", total.get());
    }

    #[test]
    fn route_input_prefers_the_world_pick_over_the_gui() {
        let (mut tree, root) = host();
        let clicks = Rc::new(Cell::new(0));
        let counter = clicks.clone();
        let button = tree.add_child(
            root,
            Button::new("Hit").on_click(move || counter.set(counter.get() + 1)),
        );
        draw_ui::layout(&mut tree, viewport(200.0, 200.0));
        tree.update();

        let world = tree.add_node2d(tree.root(), "World");
        tree.set_visual(
            world,
            Visual::Rect {
                size: Size::splat(200.0),
                color: draw_core::Color::RED,
            },
        );
        let hits = Rc::new(Cell::new(0));
        let h = hits.clone();
        tree.set_input_event(world, move |_| {
            h.set(h.get() + 1);
            EventResult::Handled
        });
        tree.update();

        let center = draw_ui::control(&tree, button).unwrap().rect.center();
        let down = InputEvent::PointerDown {
            position: center,
            button: PointerButton::Left,
        };
        let up = InputEvent::PointerUp {
            position: center,
            button: PointerButton::Left,
        };
        assert!(route_input(&mut tree, &down).is_handled());
        assert!(route_input(&mut tree, &up).is_handled());
        assert_eq!(hits.get(), 2);
        assert_eq!(clicks.get(), 0, "world consumed both events");
    }

    #[test]
    fn layout_cache_lives_on_the_tree() {
        let (mut tree, _root) = host();
        draw_ui::layout(&mut tree, viewport(100.0, 100.0));
        assert_eq!(draw_ui::layout_count(&tree), 1);
        draw_ui::layout(&mut tree, viewport(100.0, 100.0));
        assert_eq!(draw_ui::layout_count(&tree), 1, "cache hit");
    }

    #[test]
    fn decor_paints_around_nodes_in_tree_order() {
        use draw_ui::{InteractState, NodeDecor};
        use std::cell::RefCell;

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

        let (mut tree, root) = host();
        let a = tree.add_child(root, Label::new("A"));
        let b = tree.add_child(root, Label::new("B"));
        let log = Rc::new(RefCell::new(Vec::new()));
        draw_ui::add_decor(
            &mut tree,
            a,
            Rc::new(Marker {
                name: "a",
                log: log.clone(),
            }),
        );
        draw_ui::add_decor(
            &mut tree,
            b,
            Rc::new(Marker {
                name: "b",
                log: log.clone(),
            }),
        );
        draw_ui::layout(&mut tree, viewport(200.0, 200.0));
        tree.update();
        let mut ctx = draw_render::PaintContext::new();
        draw_ui::paint(&tree, &mut ctx);
        assert_eq!(&*log.borrow(), &["a", "a", "b", "b"]);
    }
}
