//! Callback refs: getting a mounted node's `NodeId` out of a declarative
//! component chain.
//!
//! A [`Component`] is a pure spec: it has no identity until it is built into a
//! [`SceneTree`]. [`NodeRef`] is the Godot-style "object handle" for that
//! identity: create it before mount, pass it into a component with
//! [`Component::ref_`], mount the component with either
//! [`SceneTree::add_child`](draw_scene::SceneTree::add_child) or
//! [`Component::child`], then read the id after mount.

use std::cell::Cell;
use std::rc::Rc;

use draw_core::NodeId;
use draw_scene::{SceneChild, SceneTree};

use crate::base::{Component, Spec};
use draw_ui::Widget;

/// A slot that a mounted component fills with its `NodeId`.
///
/// Cheap to clone (all clones share one cell). Read it after mounting; it is
/// `None` until then.
#[derive(Clone, Default)]
pub struct NodeRef(Rc<Cell<Option<NodeId>>>);

impl NodeRef {
    pub fn new() -> Self {
        Self::default()
    }

    /// The mounted id, or `None` before the component is built.
    pub fn get(&self) -> Option<NodeId> {
        self.0.get()
    }

    /// True once the component has been mounted.
    pub fn is_set(&self) -> bool {
        self.get().is_some()
    }

    pub(crate) fn fill(&self, id: NodeId) {
        self.0.set(Some(id));
    }
}

/// Wraps a component and reports its mounted id to a callback.
///
/// `Ref<C>` is both a [`Component`] and a [`SceneChild`], so it behaves the
/// same whether it is mounted with `tree.add_child(parent, r)` or composed with
/// `parent.child(r)`: both paths run [`Component::build`], which fires the
/// callback. This is the component equivalent of React's `ref` callback.
pub struct Ref<C> {
    inner: C,
    on_mount: Box<dyn FnOnce(NodeId)>,
}

impl<C> Ref<C> {
    pub fn new(inner: C, on_mount: impl FnOnce(NodeId) + 'static) -> Self {
        Self {
            inner,
            on_mount: Box::new(on_mount),
        }
    }
}

impl<C: Component> Component for Ref<C> {
    fn spec(&mut self) -> &mut Spec {
        self.inner.spec()
    }

    fn name(&self) -> &'static str {
        self.inner.name()
    }

    fn widget(&self) -> Widget {
        self.inner.widget()
    }

    fn prepare(&mut self) {
        self.inner.prepare();
    }

    fn build(self, tree: &mut SceneTree, parent: NodeId) -> NodeId {
        let id = self.inner.build(tree, parent);
        (self.on_mount)(id);
        id
    }
}

impl<C: Component> SceneChild for Ref<C> {
    fn attach(self, tree: &mut SceneTree, parent: NodeId) -> NodeId {
        <Self as Component>::build(self, tree, parent)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::base::{Column, Panel};

    #[test]
    fn ref_fills_the_slot_when_added_to_the_tree() {
        let mut tree = SceneTree::new();
        let slot = NodeRef::new();
        let id = tree.add_child(tree.root(), Panel::new().ref_(&slot));
        assert_eq!(slot.get(), Some(id));
        assert!(slot.is_set());
    }

    #[test]
    fn ref_fills_the_slot_when_composed_as_a_child() {
        let mut tree = SceneTree::new();
        let slot = NodeRef::new();
        let column = tree.add_child(tree.root(), Column::new().child(Panel::new().ref_(&slot)));
        let child = tree.children(column).unwrap()[0];
        assert_eq!(slot.get(), Some(child));
    }

    #[test]
    fn with_ref_reports_the_mounted_id() {
        let mut tree = SceneTree::new();
        let reported = Rc::new(Cell::new(None));
        let value = reported.clone();
        let id = tree.add_child(
            tree.root(),
            Panel::new().with_ref(move |id| value.set(Some(id))),
        );
        assert_eq!(reported.get(), Some(id));
    }
}
