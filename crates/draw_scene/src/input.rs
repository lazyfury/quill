//! Unified node lifecycle and input routing.
//!
//! Godot's `Viewport::push_input` order is confirmed from source as
//! `_input` (capture) -> GUI (`_gui_input`) -> `_unhandled_input`. The world /
//! `Node2D` pick (`CanvasItem::_input_event`, delivered through physics object
//! picking) sits with the capture/world pass, before GUI. [`SceneTree`] owns the
//! whole order and the per-frame `process` lifecycle; the GUI middle stage is
//! supplied by the host through the [`GuiInput`] trait, so `draw_scene` stays
//! UI-agnostic and a UI-less game uses [`SceneTree::route_input`] directly.

use draw_core::{EventResult, InputEvent, NodeId, Vec2};

use crate::node::Visual;
use crate::tree::SceneTree;

/// A GUI/`Control` input stage plugged into scene input routing.
///
/// `draw_scene` owns the Godot `Viewport::push_input` order but is
/// backend-neutral and UI-agnostic: the GUI middle stage is supplied by the
/// host. `draw_ui::route_input` uses this trait via an internal GUI stage; a UI-less game simply uses
/// [`SceneTree::route_input`].
pub trait GuiInput {
    /// Handles a GUI-stage event (Godot `Control::_gui_input`). Return
    /// [`EventResult::Handled`] to stop routing.
    fn gui_input(&mut self, tree: &mut SceneTree, event: &InputEvent) -> EventResult;
}

impl SceneTree {
    // -- lifecycle ---------------------------------------------------------

    /// Installs (or replaces) a per-frame `process(dt)` callback on `id`.
    pub fn set_process(&mut self, id: NodeId, callback: impl FnMut(f32) + 'static) -> bool {
        match self.get_mut(id) {
            Some(node) => {
                node.process = Some(Box::new(callback));
                true
            }
            None => false,
        }
    }

    /// Removes the `process(dt)` callback.
    pub fn clear_process(&mut self, id: NodeId) -> bool {
        match self.get_mut(id) {
            Some(node) => {
                let had = node.process.take().is_some();
                had
            }
            None => false,
        }
    }

    /// Dispatches `process(dt)` to every node that has a callback, in tree
    /// order. Call once per frame.
    pub fn process(&mut self, dt: f32) {
        for id in self.iter().collect::<Vec<_>>() {
            let mut callback = self.get_mut(id).and_then(|node| node.process.take());
            if let Some(callback) = callback.as_mut() {
                callback(dt);
            }
            if let Some(callback) = callback {
                if let Some(node) = self.get_mut(id) {
                    node.process = Some(callback);
                }
            }
        }
    }

    // -- input callbacks ---------------------------------------------------

    /// Installs a capture-phase callback (Godot `Node::_input`).
    pub fn set_input(
        &mut self,
        id: NodeId,
        callback: impl FnMut(&InputEvent) -> EventResult + 'static,
    ) -> bool {
        match self.get_mut(id) {
            Some(node) => {
                node.input = Some(Box::new(callback));
                true
            }
            None => false,
        }
    }

    /// Installs a world-pick callback (Godot `CanvasItem::_input_event`).
    pub fn set_input_event(
        &mut self,
        id: NodeId,
        callback: impl FnMut(&InputEvent) -> EventResult + 'static,
    ) -> bool {
        match self.get_mut(id) {
            Some(node) => {
                node.input_event = Some(Box::new(callback));
                true
            }
            None => false,
        }
    }

    /// Installs an unhandled-input callback (Godot `Node::_unhandled_input`).
    pub fn set_unhandled_input(
        &mut self,
        id: NodeId,
        callback: impl FnMut(&InputEvent) -> EventResult + 'static,
    ) -> bool {
        match self.get_mut(id) {
            Some(node) => {
                node.unhandled_input = Some(Box::new(callback));
                true
            }
            None => false,
        }
    }

    /// Routes an event through the capture phase, then the world pick.
    ///
    /// Returns [`EventResult::Handled`] as soon as a callback consumes it.
    /// GUI/`Control` routing is the host's next step (`Ui::route_input`).
    pub fn handle_input(&mut self, event: &InputEvent) -> EventResult {
        // 1) Capture (`_input`), tree order.
        for id in self.iter().collect::<Vec<_>>() {
            if let Some(result) = self.call_capture(id, event) {
                if result.is_handled() {
                    return EventResult::Handled;
                }
            }
        }
        // 2) World pick (`_input_event`), topmost visible canvas first.
        if let Some(point) = event.position() {
            if let Some(target) = self.pick_world(point) {
                if let Some(result) = self.call_input_event(target, event) {
                    if result.is_handled() {
                        return EventResult::Handled;
                    }
                }
            }
        }
        EventResult::Ignored
    }

    /// Topmost canvas item whose `Visual` contains `screen_point`, considering
    /// canvas layers and reverse draw order. Only nodes with an `_input_event`
    /// callback are considered.
    pub fn pick_world(&self, screen_point: Vec2) -> Option<NodeId> {
        for group in self.paint_groups().into_iter().rev() {
            for id in group.items.iter().rev().copied() {
                let Some(node) = self.get(id) else {
                    continue;
                };
                if !node.has_input_event() {
                    continue;
                }
                let Some(canvas) = node.canvas() else {
                    continue;
                };
                let full = group.transform * canvas.world_transform();
                let Some(inverse) = full.try_inverse() else {
                    continue;
                };
                let local = inverse.transform_point(screen_point);
                if hit_visual(canvas.visual(), local) {
                    return Some(id);
                }
            }
        }
        None
    }

    /// Full routing without a GUI stage (UI-less games):
    /// `_input` -> world pick -> `_unhandled_input`.
    pub fn route_input(&mut self, event: &InputEvent) -> EventResult {
        if self.handle_input(event).is_handled() {
            return EventResult::Handled;
        }
        self.dispatch_unhandled_input(event)
    }

    /// Full Godot routing with a GUI stage:
    /// `_input` -> world pick -> GUI -> `_unhandled_input`.
    pub fn route_input_with(&mut self, gui: &mut dyn GuiInput, event: &InputEvent) -> EventResult {
        if self.handle_input(event).is_handled() {
            return EventResult::Handled;
        }
        if gui.gui_input(self, event).is_handled() {
            return EventResult::Handled;
        }
        self.dispatch_unhandled_input(event)
    }

    /// Runs the `_unhandled_input` stage: callbacks on nodes that want input
    /// the GUI did not consume. Call after GUI routing.
    pub fn dispatch_unhandled_input(&mut self, event: &InputEvent) -> EventResult {
        for id in self.iter().collect::<Vec<_>>() {
            let mut callback = self
                .get_mut(id)
                .and_then(|node| node.unhandled_input.take());
            let result = callback.as_mut().map(|callback| callback(event));
            if let Some(callback) = callback {
                if let Some(node) = self.get_mut(id) {
                    node.unhandled_input = Some(callback);
                }
            }
            if let Some(result) = result {
                if result.is_handled() {
                    return EventResult::Handled;
                }
            }
        }
        EventResult::Ignored
    }

    fn call_capture(&mut self, id: NodeId, event: &InputEvent) -> Option<EventResult> {
        let mut callback = self.get_mut(id).and_then(|node| node.input.take());
        let result = callback.as_mut().map(|callback| callback(event));
        if let Some(callback) = callback {
            if let Some(node) = self.get_mut(id) {
                node.input = Some(callback);
            }
        }
        result
    }

    fn call_input_event(&mut self, id: NodeId, event: &InputEvent) -> Option<EventResult> {
        let mut callback = self.get_mut(id).and_then(|node| node.input_event.take());
        let result = callback.as_mut().map(|callback| callback(event));
        if let Some(callback) = callback {
            if let Some(node) = self.get_mut(id) {
                node.input_event = Some(callback);
            }
        }
        result
    }
}

/// Point-in-visual test in the item's local space (the same space `paint`
/// draws in: `Rect` spans `(0,0)..size`, `Circle` is centered at the origin).
fn hit_visual(visual: Visual, point: Vec2) -> bool {
    match visual {
        Visual::None => false,
        Visual::Rect { size, .. } => {
            point.x >= 0.0 && point.y >= 0.0 && point.x <= size.width && point.y <= size.height
        }
        Visual::Circle { radius, .. } => point.length_squared() <= radius * radius,
        // An image is a rectangle from the origin; hit-test it like `Rect`.
        Visual::Image { size, .. } => {
            point.x >= 0.0 && point.y >= 0.0 && point.x <= size.width && point.y <= size.height
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    use crate::node::NodeKind;
    use draw_core::{Color, InputEvent, PointerButton, Size, Transform2D};

    fn rect() -> Visual {
        Visual::Rect {
            size: Size::splat(10.0),
            color: Color::RED,
        }
    }

    #[test]
    fn process_dispatches_in_tree_order() {
        let mut tree = SceneTree::new();
        let root = tree.root();
        let a = tree.add_node(root, "A");
        let b = tree.add_node2d(a, "B");
        let b2 = tree.add_node(root, "B2");

        let log = Rc::new(RefCell::new(Vec::new()));
        for (id, name) in [(a, "A"), (b, "B"), (b2, "B2")] {
            let log = log.clone();
            tree.set_process(id, move |dt| log.borrow_mut().push((name, dt)));
        }
        tree.process(0.25);
        assert_eq!(&*log.borrow(), &[("A", 0.25), ("B", 0.25), ("B2", 0.25)]);
    }

    #[test]
    fn capture_consumes_before_world_pick() {
        let mut tree = SceneTree::new();
        tree.set_viewport_size(Size::splat(100.0));
        let root = tree.root();
        let node = tree.add_node2d(root, "N");
        tree.set_position(node, Vec2::new(50.0, 50.0));
        tree.set_visual(node, rect());
        tree.update();

        let captured = Rc::new(Cell::new(false));
        let picked = Rc::new(Cell::new(false));
        let c = captured.clone();
        let p = picked.clone();
        tree.set_input(root, move |_| {
            c.set(true);
            EventResult::Handled
        });
        tree.set_input_event(node, move |_| {
            p.set(true);
            EventResult::Handled
        });

        let event = InputEvent::PointerDown {
            position: Vec2::new(50.0, 50.0),
            button: PointerButton::Left,
        };
        assert!(tree.handle_input(&event).is_handled());
        assert!(captured.get());
        assert!(!picked.get(), "capture consumed the event");
    }

    #[test]
    fn world_pick_hits_through_the_camera() {
        let mut tree = SceneTree::new();
        tree.set_viewport_size(Size::splat(100.0));
        let root = tree.root();
        let camera = tree.add_camera_2d(root, "Camera");
        tree.set_camera_current(camera, true);
        tree.set_position(camera, Vec2::new(50.0, 50.0));
        let node = tree.add_node2d(root, "N");
        tree.set_position(node, Vec2::new(50.0, 50.0));
        tree.set_visual(node, rect());
        tree.update();

        let hits = Rc::new(Cell::new(0));
        let h = hits.clone();
        tree.set_input_event(node, move |_| {
            h.set(h.get() + 1);
            EventResult::Handled
        });

        let hit = InputEvent::PointerDown {
            position: Vec2::new(50.0, 50.0),
            button: PointerButton::Left,
        };
        assert!(tree.handle_input(&hit).is_handled());
        assert_eq!(hits.get(), 1);

        // Screen (10,10) maps to world (10,10), far from the node.
        let miss = InputEvent::PointerDown {
            position: Vec2::new(10.0, 10.0),
            button: PointerButton::Left,
        };
        assert!(!tree.handle_input(&miss).is_handled());
        assert_eq!(hits.get(), 1);
    }

    #[test]
    fn world_pick_uses_canvas_layer_transform() {
        let mut tree = SceneTree::new();
        tree.set_viewport_size(Size::splat(100.0));
        let root = tree.root();
        let layer = tree.add_canvas_layer(root, "Layer");
        tree.set_canvas_layer_transform(layer, Transform2D::from_translation(Vec2::new(20.0, 0.0)));
        let node = tree.add_node2d(layer, "N");
        tree.set_visual(node, rect());
        tree.update();

        let hits = Rc::new(Cell::new(0));
        let h = hits.clone();
        tree.set_input_event(node, move |_| {
            h.set(h.get() + 1);
            EventResult::Handled
        });

        // The layer shifts the node +20 in x, so world (0,0) is at screen (20,0).
        let hit = InputEvent::PointerDown {
            position: Vec2::new(25.0, 5.0),
            button: PointerButton::Left,
        };
        assert!(tree.handle_input(&hit).is_handled());
        assert_eq!(hits.get(), 1);
    }

    #[test]
    fn world_pick_skips_nodes_without_a_handler() {
        let mut tree = SceneTree::new();
        tree.set_viewport_size(Size::splat(100.0));
        let root = tree.root();
        let node = tree.add_node2d(root, "N");
        tree.set_visual(node, rect());
        tree.update();

        let event = InputEvent::PointerDown {
            position: Vec2::new(5.0, 5.0),
            button: PointerButton::Left,
        };
        assert!(!tree.handle_input(&event).is_handled());
    }

    #[test]
    fn ui_less_route_input_runs_input_world_unhandled() {
        let mut tree = SceneTree::new();
        tree.set_viewport_size(Size::splat(100.0));
        let root = tree.root();
        let node = tree.add_node2d(root, "N");
        tree.set_visual(node, rect());
        tree.update();

        let log = Rc::new(RefCell::new(Vec::new()));
        let l1 = log.clone();
        tree.set_input(root, move |_| {
            l1.borrow_mut().push("input");
            EventResult::Ignored
        });
        let l2 = log.clone();
        tree.set_input_event(node, move |_| {
            l2.borrow_mut().push("world");
            EventResult::Ignored
        });
        let l3 = log.clone();
        tree.set_unhandled_input(node, move |_| {
            l3.borrow_mut().push("unhandled");
            EventResult::Ignored
        });

        let event = InputEvent::PointerDown {
            position: Vec2::new(5.0, 5.0),
            button: PointerButton::Left,
        };
        assert!(!tree.route_input(&event).is_handled());
        assert_eq!(&*log.borrow(), &["input", "world", "unhandled"]);
    }

    #[test]
    fn process_callback_can_mutate_external_state_each_frame() {
        let mut tree = SceneTree::new();
        let node = tree.add_node(tree.root(), "N");
        let ticks = Rc::new(Cell::new(0));
        let t = ticks.clone();
        tree.set_process(node, move |_| t.set(t.get() + 1));
        tree.process(1.0);
        tree.process(1.0);
        assert_eq!(ticks.get(), 2);
        assert_eq!(tree.node(node).kind(), NodeKind::Node);
    }
}
