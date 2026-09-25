//! `Area` triggers: callbacks fired when registered nodes overlap.
//!
//! A host owns one [`Areas`] and calls [`Areas::update`] each frame (in the same
//! step as its other runners). Each registered [`Area`] carries a
//! [`CollisionShape`] in its node's local space; [`Areas`] resolves them to world
//! space, diffs the overlap pairs against the previous frame, and fires
//! `on_enter` / `on_exit` on both sides. This is the Godot `Area2D` shape of the
//! problem — no rigid-body solver.

use draw_core::NodeId;
use draw_scene::SceneTree;

use crate::shape::CollisionShape;

/// A registered collision area.
pub struct Area {
    node: NodeId,
    shape: CollisionShape,
    enabled: bool,
    on_enter: Option<Box<dyn FnMut(NodeId)>>,
    on_exit: Option<Box<dyn FnMut(NodeId)>>,
}

impl Area {
    /// An enabled area on `node` with `shape`.
    pub fn new(node: NodeId, shape: CollisionShape) -> Self {
        Self {
            node,
            shape,
            enabled: true,
            on_enter: None,
            on_exit: None,
        }
    }

    /// Enables or disables the area (a disabled area never overlaps).
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Called with the other node's id when an overlap begins.
    pub fn on_enter(mut self, callback: impl FnMut(NodeId) + 'static) -> Self {
        self.on_enter = Some(Box::new(callback));
        self
    }

    /// Called with the other node's id when an overlap ends.
    pub fn on_exit(mut self, callback: impl FnMut(NodeId) + 'static) -> Self {
        self.on_exit = Some(Box::new(callback));
        self
    }
}

/// Tracks registered areas and their current overlap pairs.
#[derive(Default)]
pub struct Areas {
    areas: Vec<Area>,
    overlaps: Vec<(NodeId, NodeId)>,
}

impl Areas {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers `area`, replacing any existing area on the same node.
    pub fn add(&mut self, area: Area) {
        self.areas.retain(|existing| existing.node != area.node);
        self.areas.push(area);
    }

    /// Removes the area on `node` (its overlaps end on the next update).
    pub fn remove(&mut self, node: NodeId) -> bool {
        let before = self.areas.len();
        self.areas.retain(|area| area.node != node);
        self.areas.len() != before
    }

    /// Removes every area.
    pub fn clear(&mut self) {
        self.areas.clear();
        self.overlaps.clear();
    }

    pub fn len(&self) -> usize {
        self.areas.len()
    }

    pub fn is_empty(&self) -> bool {
        self.areas.is_empty()
    }

    /// Whether `a` and `b` overlapped at the last [`Areas::update`].
    pub fn is_overlapping(&self, a: NodeId, b: NodeId) -> bool {
        let pair = ordered(a, b);
        self.overlaps.contains(&pair)
    }

    /// Every node currently overlapping `node`.
    pub fn overlapping_with(&self, node: NodeId) -> Vec<NodeId> {
        self.overlaps
            .iter()
            .filter_map(|&(a, b)| {
                if a == node {
                    Some(b)
                } else if b == node {
                    Some(a)
                } else {
                    None
                }
            })
            .collect()
    }

    /// Recomputes overlap pairs and fires enter/exit callbacks.
    ///
    /// Areas whose node is no longer in `tree` are dropped (any overlap they
    /// had is reported as an exit). O(pairs), which is fine for an MVP; a broad
    /// phase can come later.
    pub fn update(&mut self, tree: &SceneTree) {
        let mut current: Vec<(NodeId, NodeId)> = Vec::new();
        for i in 0..self.areas.len() {
            for j in (i + 1)..self.areas.len() {
                let a = &self.areas[i];
                let b = &self.areas[j];
                if !a.enabled || !b.enabled {
                    continue;
                }
                let (Some(ta), Some(tb)) =
                    (tree.world_transform(a.node), tree.world_transform(b.node))
                else {
                    continue;
                };
                if a.shape.world(ta).overlaps(b.shape.world(tb)) {
                    current.push(ordered(a.node, b.node));
                }
            }
        }

        let mut entered = Vec::new();
        let mut exited = Vec::new();
        for pair in &current {
            if !self.overlaps.contains(pair) {
                entered.push(*pair);
            }
        }
        for pair in &self.overlaps {
            if !current.contains(pair) {
                exited.push(*pair);
            }
        }
        self.overlaps = current;

        for (a, b) in entered {
            self.fire(a, b, true);
            self.fire(b, a, true);
        }
        for (a, b) in exited {
            self.fire(a, b, false);
            self.fire(b, a, false);
        }

        self.areas.retain(|area| tree.contains(area.node));
    }

    fn fire(&mut self, owner: NodeId, other: NodeId, enter: bool) {
        let Some(area) = self.areas.iter_mut().find(|area| area.node == owner) else {
            return;
        };
        let callback = if enter {
            area.on_enter.as_mut()
        } else {
            area.on_exit.as_mut()
        };
        if let Some(callback) = callback {
            callback(other);
        }
    }
}

fn ordered(a: NodeId, b: NodeId) -> (NodeId, NodeId) {
    if a <= b {
        (a, b)
    } else {
        (b, a)
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use draw_core::{Size, Vec2};

    use super::*;
    use crate::Sprite;
    use draw_render::TextureId;

    /// Logs `(label, other)` callbacks so tests can assert order/content.
    type Log = Rc<RefCell<Vec<(char, NodeId)>>>;

    fn setup() -> (SceneTree, NodeId, NodeId, Areas, Log) {
        let mut tree = SceneTree::new();
        let a = tree.add_child(
            tree.root(),
            Sprite::new(TextureId::new(1), Size::splat(10.0)),
        );
        // Start apart so no overlap is reported on the first update.
        let b = tree.add_child(
            tree.root(),
            Sprite::new(TextureId::new(1), Size::splat(10.0)).position(Vec2::new(100.0, 0.0)),
        );
        tree.update();

        let log: Log = Rc::new(RefCell::new(Vec::new()));
        let (la, lb) = (log.clone(), log.clone());
        let mut areas = Areas::new();
        areas.add(
            Area::new(a, CollisionShape::aabb(Size::splat(10.0)))
                .on_enter(move |other| la.borrow_mut().push(('>', other)))
                .on_exit(move |other| lb.borrow_mut().push(('<', other))),
        );
        areas.add(Area::new(b, CollisionShape::aabb(Size::splat(10.0))));
        (tree, a, b, areas, log)
    }

    #[test]
    fn enter_fires_once_then_exit_when_they_part() {
        let (mut tree, a, b, mut areas, log) = setup();
        areas.update(&tree);
        assert!(!areas.is_overlapping(a, b));

        tree.set_position(b, Vec2::new(5.0, 0.0));
        tree.update();
        areas.update(&tree);
        assert!(areas.is_overlapping(a, b));
        assert_eq!(&*log.borrow(), &[('>', b)], "one enter");

        // Staying overlapped does not re-fire.
        areas.update(&tree);
        assert_eq!(log.borrow().len(), 1);

        tree.set_position(b, Vec2::new(100.0, 0.0));
        tree.update();
        areas.update(&tree);
        assert!(!areas.is_overlapping(a, b));
        assert_eq!(&*log.borrow(), &[('>', b), ('<', b)], "then one exit");
    }

    #[test]
    fn a_removed_node_reports_an_exit() {
        let (mut tree, a, b, mut areas, log) = setup();
        tree.set_position(b, Vec2::new(5.0, 0.0));
        tree.update();
        areas.update(&tree);
        assert!(areas.is_overlapping(a, b));

        assert!(tree.remove(b));
        areas.update(&tree);
        assert!(!areas.is_overlapping(a, b));
        assert_eq!(log.borrow().last(), Some(&('<', b)));
        assert_eq!(areas.len(), 1, "the removed area is dropped");
    }

    #[test]
    fn a_disabled_area_never_overlaps() {
        let (mut tree, a, b, mut areas, _log) = setup();
        tree.set_position(b, Vec2::new(5.0, 0.0));
        tree.update();
        areas.remove(b);
        areas.add(Area::new(b, CollisionShape::aabb(Size::splat(10.0))).enabled(false));

        areas.update(&tree);
        assert!(!areas.is_overlapping(a, b));
    }
}
