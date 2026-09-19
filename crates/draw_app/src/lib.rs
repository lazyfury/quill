//! `draw_app` — the application layer on top of `draw_ui`.
//!
//! `draw_ui` owns layout and paint. This crate owns everything an application
//! needs around them:
//!
//! - **Construction** — [`Component`], [`View`], [`BuildContext`], [`ViewExt`]
//!   and the `add_*` helpers build controls into a [`SceneTree`].
//! - **Input** — [`hit_test`] / [`handle_input`] / [`route_input`] run the GUI
//!   hit-test and the `_input -> world -> GUI -> _unhandled_input` order.
//! - **Runtime** — [`App`] owns a tree and submits a frame to a
//!   [`RenderBackend`](draw_render::RenderBackend); layout/paint themselves live
//!   in `draw_ui` (`draw_ui::layout`, `draw_ui::paint`).
//!
//! ```ignore
//! use draw_app as app;
//! use draw_ui as ui;
//!
//! let mut app = app::App::new();
//! app::set_theme(app.tree_mut(), Theme::dark());
//! let root = app::add_flex(app.tree_mut(), app.tree().root(), FlexStyle::column());
//! app::mount(app.tree_mut(), root, Column::new().child(Label::new("Hi")));
//! app.event(&event);
//! app.render(viewport, &mut backend)?;
//! ```

/// Crate name, kept for lightweight smoke checks.
pub const CRATE: &str = "draw_app";

mod app;
mod build;
mod component;
mod input;
mod view;

pub use app::App;
pub use build::{
    add_button, add_flex, add_grid, add_hbox, add_label, add_panel, add_vbox, control_mut, insert,
    set_on_click, set_text, update_control, ClickCallback,
};
pub use component::{
    Button, Column, Component, ControlRef, Flex, Grid, HBox, Label, Panel, Row, VBox,
};
pub use input::{
    focused, handle_input, hit_test, hovered, hovered_is_button, is_interactive, route_input,
};
pub use view::{child, widget as insert_widget, BuildContext, Child, Modify, View, ViewExt};

use draw_core::NodeId;
use draw_scene::SceneTree;
use draw_ui::{ButtonState, Control, Widget};

/// Mounts a [`Component`] under `parent`.
pub fn add<C: Component>(tree: &mut SceneTree, parent: NodeId, component: C) -> ControlRef {
    component.mount(tree, parent)
}

/// Mounts a declarative [`View`] under `parent`.
pub fn mount<V: View>(tree: &mut SceneTree, parent: NodeId, view: V) -> NodeId {
    view.build(&mut BuildContext::new(tree, parent))
}

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

    use draw_core::{EventResult, InputEvent, PointerButton, Rect, Size, Vec2, ViewportSize};
    use draw_scene::Visual;
    use draw_ui::{FlexStyle, MouseFilter, SizeBasis};

    fn viewport(w: f32, h: f32) -> ViewportSize {
        ViewportSize::new(Size::new(w, h))
    }

    fn host() -> (SceneTree, NodeId) {
        let mut tree = SceneTree::new();
        let tree_root = tree.root();
        let root = add_flex(&mut tree, tree_root, FlexStyle::column());
        update_control(&mut tree, root, |data| {
            data.mouse_filter = MouseFilter::Ignore
        });
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
        let panel = add_panel(&mut tree, root);
        let vbox = add_vbox(&mut tree, panel);
        let label = add_label(&mut tree, vbox, "Hello");
        let button = add_button(&mut tree, vbox, "Click me");
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
        let row = add_flex(
            &mut tree,
            root,
            FlexStyle::row().gap(0.0).padding(draw_core::Edges::ZERO),
        );
        let a = add_panel(&mut tree, row);
        let b = add_panel(&mut tree, row);
        update_control(&mut tree, a, |data| {
            data.layout = draw_ui::layout::LayoutStyle::new()
                .basis(SizeBasis::Px(100.0))
                .shrink(0.0);
        });
        update_control(&mut tree, b, |data| {
            data.layout = draw_ui::layout::LayoutStyle::new()
                .basis(SizeBasis::Px(100.0))
                .grow(1.0);
        });
        draw_ui::layout(&mut tree, viewport(300.0, 100.0));
        assert_eq!(draw_ui::control(&tree, a).unwrap().rect.size.width, 100.0);
        assert_eq!(draw_ui::control(&tree, b).unwrap().rect.size.width, 200.0);
    }

    #[test]
    fn click_fires_callback_and_focus() {
        let (mut tree, root) = host();
        let clicks = Rc::new(Cell::new(0));
        let counter = clicks.clone();
        let button = add_button(&mut tree, root, "Click me");
        set_on_click(&mut tree, button, move || counter.set(counter.get() + 1));
        draw_ui::layout(&mut tree, viewport(400.0, 200.0));
        tree.update();

        let center = draw_ui::control(&tree, button).unwrap().rect.center();
        click(&mut tree, center);
        assert_eq!(clicks.get(), 1);
        assert_eq!(click_count(&tree, button), 1);
        assert_eq!(focused(&tree), Some(button));
    }

    #[test]
    fn route_input_prefers_the_world_pick_over_the_gui() {
        let (mut tree, root) = host();
        let clicks = Rc::new(Cell::new(0));
        let counter = clicks.clone();
        let button = add_button(&mut tree, root, "Hit");
        set_on_click(&mut tree, button, move || counter.set(counter.get() + 1));
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
    fn theme_and_layout_cache_live_on_the_tree() {
        let (mut tree, _root) = host();
        draw_ui::set_theme(&mut tree, draw_theme::Theme::light());
        draw_ui::layout(&mut tree, viewport(100.0, 100.0));
        assert_eq!(draw_ui::layout_count(&tree), 1);

        assert_eq!(draw_ui::theme(&tree), draw_theme::Theme::light());
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
        let a = add_label(&mut tree, root, "A");
        let b = add_label(&mut tree, root, "B");
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
