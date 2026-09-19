use draw_core::{NodeId, NodeIdAllocator, Transform2D, Vec2};

use crate::node::{DirtyFlags, Node, NodeKind};

/// The scene tree: an arena of [`Node`]s plus parent/child links.
///
/// Nodes are addressed by [`NodeId`]. Derived state (`world_transform`,
/// `world_visible`) is recomputed by [`SceneTree::update`], which uses per-node
/// [`DirtyFlags`] to skip work when nothing changed.
///
/// Child lists are kept sorted by `(z_index, creation order)`, so both
/// iteration and drawing order are deterministic.
#[derive(Debug, Clone)]
pub struct SceneTree {
    slots: Vec<Option<Node>>,
    allocator: NodeIdAllocator,
    root: NodeId,
    order_counter: u64,
}

impl Default for SceneTree {
    fn default() -> Self {
        Self::new()
    }
}

impl SceneTree {
    /// Creates a tree with a single plain `Node` root.
    pub fn new() -> Self {
        let mut allocator = NodeIdAllocator::new();
        let root = allocator.alloc();
        let root_node = Node::new(root, "root", NodeKind::Node, 0);
        Self {
            slots: vec![Some(root_node)],
            allocator,
            root,
            order_counter: 1,
        }
    }

    pub const fn root(&self) -> NodeId {
        self.root
    }

    /// Number of live nodes, including the root.
    pub fn node_count(&self) -> usize {
        self.allocator.live_count()
    }

    pub fn contains(&self, id: NodeId) -> bool {
        self.get(id).is_some()
    }

    pub fn get(&self, id: NodeId) -> Option<&Node> {
        if !self.allocator.is_alive(id) {
            return None;
        }
        self.slots.get(id.index() as usize).and_then(Option::as_ref)
    }

    pub fn get_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        if !self.allocator.is_alive(id) {
            return None;
        }
        self.slots
            .get_mut(id.index() as usize)
            .and_then(Option::as_mut)
    }

    /// Like [`SceneTree::get`], but panics on a stale or unknown id.
    pub fn node(&self, id: NodeId) -> &Node {
        self.get(id)
            .unwrap_or_else(|| panic!("node {id} is not live"))
    }

    /// Like [`SceneTree::get_mut`], but panics on a stale or unknown id.
    pub fn node_mut(&mut self, id: NodeId) -> &mut Node {
        self.get_mut(id)
            .unwrap_or_else(|| panic!("node {id} is not live"))
    }

    pub fn parent(&self, id: NodeId) -> Option<NodeId> {
        self.get(id).and_then(Node::parent)
    }

    pub fn children(&self, id: NodeId) -> Option<&[NodeId]> {
        self.get(id).map(Node::children)
    }

    // -- construction ------------------------------------------------------

    /// Adds a plain grouping node under `parent`.
    ///
    /// # Panics
    /// Panics if `parent` is not a live node.
    pub fn add_node(&mut self, parent: NodeId, name: impl Into<String>) -> NodeId {
        self.insert(parent, name, NodeKind::Node)
    }

    /// Adds a `Node2D` canvas item under `parent`.
    ///
    /// # Panics
    /// Panics if `parent` is not a live node.
    pub fn add_node2d(&mut self, parent: NodeId, name: impl Into<String>) -> NodeId {
        self.insert(parent, name, NodeKind::Node2D)
    }

    fn insert(&mut self, parent: NodeId, name: impl Into<String>, kind: NodeKind) -> NodeId {
        assert!(self.contains(parent), "add: parent {parent} is not live");
        let id = self.allocator.alloc();
        let order = self.order_counter;
        self.order_counter += 1;

        let node = Node::new(id, name, kind, order);
        let index = id.index() as usize;
        if index == self.slots.len() {
            self.slots.push(Some(node));
        } else {
            self.slots[index] = Some(node);
        }

        self.node_mut(parent).children.push(id);
        self.node_mut(id).parent = Some(parent);
        self.sort_children_of(parent);
        id
    }

    /// Removes a node and its whole subtree.
    ///
    /// Returns `false` for the root or an unknown id.
    pub fn remove(&mut self, id: NodeId) -> bool {
        if id == self.root || !self.contains(id) {
            return false;
        }
        let parent = self.get(id).and_then(Node::parent);
        let subtree = self.collect_subtree(id);

        if let Some(parent) = parent {
            if let Some(node) = self.get_mut(parent) {
                node.children.retain(|c| *c != id);
            }
        }
        for victim in &subtree {
            self.allocator.free(*victim);
            self.slots[victim.index() as usize] = None;
        }
        true
    }

    /// Moves `id` under `new_parent`, keeping the rest of the tree intact.
    ///
    /// Rejects the root, unknown ids, and moves that would create a cycle.
    /// The moved subtree is marked dirty so it is recomputed on the next update.
    pub fn reparent(&mut self, id: NodeId, new_parent: NodeId) -> bool {
        if id == self.root
            || !self.contains(id)
            || !self.contains(new_parent)
            || id == new_parent
            || self.is_ancestor(id, new_parent)
        {
            return false;
        }
        if let Some(old_parent) = self.get(id).and_then(Node::parent) {
            if let Some(node) = self.get_mut(old_parent) {
                node.children.retain(|c| *c != id);
            }
        }
        self.node_mut(new_parent).children.push(id);
        self.node_mut(id).parent = Some(new_parent);
        self.sort_children_of(new_parent);

        for node_id in self.collect_subtree(id) {
            if let Some(canvas) = self
                .slots
                .get_mut(node_id.index() as usize)
                .and_then(Option::as_mut)
                .and_then(|n| n.canvas.as_mut())
            {
                canvas.dirty = DirtyFlags::DIRTY;
            }
        }
        true
    }

    // -- canvas item state -------------------------------------------------

    /// Sets the local transform of a canvas item. Returns `false` for non-canvas
    /// nodes or unknown ids.
    pub fn set_transform(&mut self, id: NodeId, transform: Transform2D) -> bool {
        self.update_local_transform(id, |t| *t = transform)
    }

    /// Sets the local position (transform origin), preserving rotation/scale.
    pub fn set_position(&mut self, id: NodeId, position: Vec2) -> bool {
        self.update_local_transform(id, |t| t.origin = position)
    }

    /// Sets the local rotation (radians), preserving scale and position.
    pub fn set_rotation(&mut self, id: NodeId, angle_radians: f32) -> bool {
        let Some(t) = self.local_transform(id) else {
            return false;
        };
        let scale = Vec2::new(t.x_axis.length(), t.y_axis.length());
        self.set_transform(
            id,
            Transform2D::from_scale_rotation_origin(scale, angle_radians, t.origin),
        )
    }

    /// Sets the local scale, preserving rotation and position.
    pub fn set_scale(&mut self, id: NodeId, scale: Vec2) -> bool {
        let Some(t) = self.local_transform(id) else {
            return false;
        };
        let rotation = t.x_axis.y.atan2(t.x_axis.x);
        self.set_transform(
            id,
            Transform2D::from_scale_rotation_origin(scale, rotation, t.origin),
        )
    }

    pub fn local_transform(&self, id: NodeId) -> Option<Transform2D> {
        self.get(id).and_then(Node::local_transform)
    }

    /// Transform relative to the root, valid after [`SceneTree::update`].
    pub fn world_transform(&self, id: NodeId) -> Option<Transform2D> {
        self.get(id).and_then(Node::world_transform)
    }

    pub fn position(&self, id: NodeId) -> Option<Vec2> {
        self.local_transform(id).map(|t| t.origin)
    }

    pub fn rotation(&self, id: NodeId) -> Option<f32> {
        self.local_transform(id)
            .map(|t| t.x_axis.y.atan2(t.x_axis.x))
    }

    pub fn scale(&self, id: NodeId) -> Option<Vec2> {
        self.local_transform(id)
            .map(|t| Vec2::new(t.x_axis.length(), t.y_axis.length()))
    }

    /// Sets local visibility. Returns `false` for non-canvas nodes.
    pub fn set_visible(&mut self, id: NodeId, visible: bool) -> bool {
        if !self.allocator.is_alive(id) {
            return false;
        }
        let Some(canvas) = self
            .slots
            .get_mut(id.index() as usize)
            .and_then(Option::as_mut)
            .and_then(|n| n.canvas.as_mut())
        else {
            return false;
        };
        canvas.visible = visible;
        canvas.dirty.visibility = true;
        true
    }

    pub fn is_visible(&self, id: NodeId) -> Option<bool> {
        self.get(id).map(Node::is_visible)
    }

    /// Effective visibility including ancestors (valid after update).
    pub fn is_visible_in_tree(&self, id: NodeId) -> Option<bool> {
        self.get(id).map(Node::world_visible)
    }

    /// Sets the z-index and re-sorts the parent's child list. Returns `false`
    /// for non-canvas nodes.
    pub fn set_z_index(&mut self, id: NodeId, z_index: i32) -> bool {
        if !self.allocator.is_alive(id) {
            return false;
        }
        let Some(canvas) = self
            .slots
            .get_mut(id.index() as usize)
            .and_then(Option::as_mut)
            .and_then(|n| n.canvas.as_mut())
        else {
            return false;
        };
        canvas.z_index = z_index;
        let parent = self.get(id).and_then(Node::parent);
        if let Some(parent) = parent {
            self.sort_children_of(parent);
        }
        true
    }

    pub fn z_index(&self, id: NodeId) -> Option<i32> {
        self.get(id).map(Node::z_index)
    }

    fn update_local_transform(&mut self, id: NodeId, f: impl FnOnce(&mut Transform2D)) -> bool {
        if !self.allocator.is_alive(id) {
            return false;
        }
        let Some(canvas) = self
            .slots
            .get_mut(id.index() as usize)
            .and_then(Option::as_mut)
            .and_then(|n| n.canvas.as_mut())
        else {
            return false;
        };
        f(&mut canvas.transform);
        canvas.dirty.transform = true;
        true
    }

    // -- update ------------------------------------------------------------

    /// Recomputes world transforms and effective visibility.
    ///
    /// Returns the number of canvas items whose world transform was
    /// recomputed. A second call with no intervening changes returns `0`.
    pub fn update(&mut self) -> usize {
        self.update_subtree(self.root, Transform2D::IDENTITY, false, true, false)
    }

    fn update_subtree(
        &mut self,
        id: NodeId,
        parent_world: Transform2D,
        parent_world_changed: bool,
        parent_visible: bool,
        parent_visible_changed: bool,
    ) -> usize {
        let mut recomputed = 0;
        let (children, child_world, world_changed, child_visible, visible_changed);

        {
            let Some(node) = self
                .slots
                .get_mut(id.index() as usize)
                .and_then(Option::as_mut)
            else {
                return 0;
            };

            match node.canvas.as_mut() {
                Some(canvas) => {
                    world_changed = canvas.dirty.transform || parent_world_changed;
                    if world_changed {
                        canvas.world_transform = parent_world * canvas.transform;
                        canvas.dirty.transform = false;
                        recomputed = 1;
                    }
                    visible_changed = canvas.dirty.visibility || parent_visible_changed;
                    if visible_changed {
                        canvas.world_visible = parent_visible && canvas.visible;
                        canvas.dirty.visibility = false;
                    }
                    child_world = canvas.world_transform;
                    child_visible = canvas.world_visible;
                }
                None => {
                    world_changed = parent_world_changed;
                    visible_changed = parent_visible_changed;
                    child_world = parent_world;
                    child_visible = parent_visible;
                }
            }
            children = node.children.clone();
        }

        for child in children {
            recomputed += self.update_subtree(
                child,
                child_world,
                world_changed,
                child_visible,
                visible_changed,
            );
        }
        recomputed
    }

    // -- traversal ---------------------------------------------------------

    /// Depth-first pre-order traversal over all nodes; children are visited in
    /// `(z_index, creation order)` order.
    pub fn iter(&self) -> PreorderIter<'_> {
        PreorderIter::new(self, false)
    }

    /// Like [`SceneTree::iter`], but prunes invisible subtrees.
    pub fn iter_visible(&self) -> PreorderIter<'_> {
        PreorderIter::new(self, true)
    }

    // -- internals ---------------------------------------------------------

    fn sort_children_of(&mut self, parent: NodeId) {
        let children = {
            let Some(node) = self.get_mut(parent) else {
                return;
            };
            std::mem::take(&mut node.children)
        };
        let mut keyed: Vec<(NodeId, (i32, u64))> = children
            .into_iter()
            .map(|child| (child, self.child_sort_key(child)))
            .collect();
        keyed.sort_by_key(|(_, key)| *key);
        let sorted: Vec<NodeId> = keyed.into_iter().map(|(child, _)| child).collect();
        if let Some(node) = self.get_mut(parent) {
            node.children = sorted;
        }
    }

    fn child_sort_key(&self, id: NodeId) -> (i32, u64) {
        self.get(id)
            .map_or((0, u64::MAX), |node| (node.z_index(), node.order()))
    }

    fn collect_subtree(&self, id: NodeId) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut stack = vec![id];
        while let Some(current) = stack.pop() {
            if let Some(node) = self.get(current) {
                out.push(current);
                for &child in &node.children {
                    stack.push(child);
                }
            }
        }
        out
    }

    fn is_ancestor(&self, ancestor: NodeId, node: NodeId) -> bool {
        let mut current = self.get(node).and_then(Node::parent);
        while let Some(id) = current {
            if id == ancestor {
                return true;
            }
            current = self.get(id).and_then(Node::parent);
        }
        false
    }
}

/// Depth-first pre-order iterator returned by [`SceneTree::iter`].
pub struct PreorderIter<'a> {
    tree: &'a SceneTree,
    stack: Vec<NodeId>,
    visible_only: bool,
    started: bool,
}

impl<'a> PreorderIter<'a> {
    fn new(tree: &'a SceneTree, visible_only: bool) -> Self {
        Self {
            tree,
            stack: Vec::new(),
            visible_only,
            started: false,
        }
    }
}

impl Iterator for PreorderIter<'_> {
    type Item = NodeId;

    fn next(&mut self) -> Option<NodeId> {
        if !self.started {
            self.started = true;
            self.stack.push(self.tree.root());
        }
        loop {
            let id = self.stack.pop()?;
            let node = self.tree.get(id)?;
            if self.visible_only && !node.is_visible() {
                continue;
            }
            for &child in node.children().iter().rev() {
                self.stack.push(child);
            }
            return Some(id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::NodeKind;
    use draw_core::Vec2;
    use std::f32::consts::FRAC_PI_2;

    const EPS: f32 = 1e-5;

    fn approx(a: Vec2, b: Vec2) -> bool {
        (a - b).length() < EPS
    }

    fn names(tree: &SceneTree) -> Vec<String> {
        tree.iter()
            .map(|id| tree.node(id).name().to_string())
            .collect()
    }

    #[test]
    fn root_and_children() {
        let mut tree = SceneTree::new();
        let root = tree.root();
        assert_eq!(tree.node(root).name(), "root");
        assert_eq!(tree.node(root).kind(), NodeKind::Node);
        assert!(!tree.node(root).is_canvas_item());
        assert_eq!(tree.node_count(), 1);

        let a = tree.add_node2d(root, "A");
        let b = tree.add_node(root, "B");
        assert_eq!(tree.parent(a), Some(root));
        assert_eq!(tree.children(root), Some(&[a, b][..]));
        assert!(tree.node(a).is_canvas_item());
        assert!(!tree.node(b).is_canvas_item());
        assert_eq!(tree.node_count(), 3);
    }

    #[test]
    fn world_transform_propagation() {
        let mut tree = SceneTree::new();
        let root = tree.root();
        let a = tree.add_node2d(root, "A");
        let b = tree.add_node2d(a, "B");

        // B local = translate(5, 0)
        assert!(tree.set_position(b, Vec2::new(5.0, 0.0)));
        // A local = rotate(90deg) then translate(10, 0)
        assert!(tree.set_position(a, Vec2::new(10.0, 0.0)));
        assert!(tree.set_rotation(a, FRAC_PI_2));

        tree.update();

        let world_b = tree.world_transform(b).unwrap();
        // rotate (5,0) -> (0,5), then translate by (10,0) -> (10,5)
        assert!(
            approx(world_b.origin, Vec2::new(10.0, 5.0)),
            "{:?}",
            world_b.origin
        );
        assert!(approx(
            tree.world_transform(a).unwrap().origin,
            Vec2::new(10.0, 0.0)
        ));
        assert!(approx(tree.position(b).unwrap(), Vec2::new(5.0, 0.0)));
    }

    #[test]
    fn changing_parent_updates_children() {
        let mut tree = SceneTree::new();
        let root = tree.root();
        let a = tree.add_node2d(root, "A");
        let b = tree.add_node2d(a, "B");
        tree.set_position(a, Vec2::new(1.0, 2.0));
        tree.set_position(b, Vec2::new(3.0, 4.0));
        tree.update();
        assert!(approx(
            tree.world_transform(b).unwrap().origin,
            Vec2::new(4.0, 6.0)
        ));

        tree.set_position(a, Vec2::new(10.0, 20.0));
        tree.update();
        assert!(approx(
            tree.world_transform(b).unwrap().origin,
            Vec2::new(13.0, 24.0)
        ));
    }

    #[test]
    fn visibility_propagates() {
        let mut tree = SceneTree::new();
        let root = tree.root();
        let a = tree.add_node2d(root, "A");
        let b = tree.add_node2d(a, "B");
        tree.update();

        assert_eq!(tree.is_visible(a), Some(true));
        assert_eq!(tree.is_visible_in_tree(b), Some(true));

        tree.set_visible(a, false);
        tree.update();
        assert_eq!(tree.is_visible(a), Some(false));
        assert_eq!(tree.is_visible_in_tree(a), Some(false));
        assert_eq!(tree.is_visible_in_tree(b), Some(false));

        tree.set_visible(a, true);
        tree.update();
        assert_eq!(tree.is_visible_in_tree(b), Some(true));
    }

    #[test]
    fn z_index_orders_traversal() {
        let mut tree = SceneTree::new();
        let root = tree.root();
        let a = tree.add_node2d(root, "A");
        let b = tree.add_node2d(root, "B");
        let c = tree.add_node2d(root, "C");

        // default order is creation order
        assert_eq!(names(&tree), ["root", "A", "B", "C"]);

        tree.set_z_index(c, -1);
        tree.set_z_index(a, 5);
        assert_eq!(names(&tree), ["root", "C", "B", "A"]);
        assert_eq!(tree.z_index(a), Some(5));
        let _ = b;
    }

    #[test]
    fn iter_visible_prunes_hidden_subtrees() {
        let mut tree = SceneTree::new();
        let root = tree.root();
        let a = tree.add_node2d(root, "A");
        let b = tree.add_node2d(a, "B");
        let c = tree.add_node2d(root, "C");
        tree.update();

        let all: Vec<String> = tree
            .iter()
            .map(|id| tree.node(id).name().to_string())
            .collect();
        assert_eq!(all, ["root", "A", "B", "C"]);

        tree.set_visible(a, false);
        let visible: Vec<String> = tree
            .iter_visible()
            .map(|id| tree.node(id).name().to_string())
            .collect();
        assert_eq!(visible, ["root", "C"]);
        let _ = (b, c);
    }

    #[test]
    fn dirty_flags_skip_clean_nodes() {
        let mut tree = SceneTree::new();
        let root = tree.root();
        let a = tree.add_node2d(root, "A");
        let b = tree.add_node2d(a, "B");

        // first update computes world transforms for A and B
        assert_eq!(tree.update(), 2);
        // nothing changed -> no recomputation
        assert_eq!(tree.update(), 0);

        tree.set_position(a, Vec2::new(1.0, 1.0));
        // A recomputes; B recomputes because its parent changed
        assert_eq!(tree.update(), 2);
        assert_eq!(tree.update(), 0);

        tree.set_visible(b, false);
        // only visibility changed, transforms stay clean
        assert_eq!(tree.update(), 0);
    }

    #[test]
    fn remove_frees_whole_subtree() {
        let mut tree = SceneTree::new();
        let root = tree.root();
        let a = tree.add_node2d(root, "A");
        let b = tree.add_node2d(a, "B");
        let c = tree.add_node2d(b, "C");

        assert_eq!(tree.node_count(), 4);
        assert!(tree.remove(a));
        assert_eq!(tree.node_count(), 1);
        assert!(!tree.contains(a));
        assert!(!tree.contains(b));
        assert!(!tree.contains(c));
        assert_eq!(tree.children(root), Some(&[][..]));

        // stale handles are rejected
        assert!(tree.get(a).is_none());
        assert!(!tree.remove(a));
        // root cannot be removed
        assert!(!tree.remove(root));
    }

    #[test]
    fn reparent_moves_subtree_and_rejects_cycles() {
        let mut tree = SceneTree::new();
        let root = tree.root();
        let p1 = tree.add_node2d(root, "P1");
        let p2 = tree.add_node2d(root, "P2");
        let child = tree.add_node2d(p1, "Child");

        assert!(tree.reparent(child, p2));
        assert_eq!(tree.parent(child), Some(p2));
        assert_eq!(tree.children(p1), Some(&[][..]));
        assert_eq!(tree.children(p2), Some(&[child][..]));

        // cannot reparent a node under its own descendant
        assert!(!tree.reparent(p2, child));
        // cannot reparent the root
        assert!(!tree.reparent(root, p1));

        // moved subtree recomputes on next update
        tree.set_position(p2, Vec2::new(7.0, 0.0));
        tree.set_position(child, Vec2::new(1.0, 0.0));
        assert!(tree.update() >= 2);
        assert!(approx(
            tree.world_transform(child).unwrap().origin,
            Vec2::new(8.0, 0.0)
        ));
    }
}
