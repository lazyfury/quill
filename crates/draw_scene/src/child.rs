//! Attaching front-end values to the tree as children.
//!
//! `draw_scene` knows only the tree and node ids, so [`SceneChild`] stays
//! backend- and UI-neutral; `draw_components` implements it for every UI component.
//! Composition is expressed with [`SceneTree::add_child`].

use draw_core::NodeId;

use crate::SceneTree;

/// A value that builds itself into the tree as a child node.
///
/// This is the single composition entry point for the scene: callers write
/// `tree.add_child(parent, component)`, or compose a whole scene declaratively
/// and mount it once with [`SceneChild::into_tree`] / [`SceneTree::from_component`].
pub trait SceneChild: Sized {
    /// Builds `self` under `parent`, returning the node it created.
    fn attach(self, tree: &mut SceneTree, parent: NodeId) -> NodeId;

    /// Builds a fresh [`SceneTree`] with `self` mounted at the root.
    ///
    /// This is the ergonomic top-level constructor: compose the scene with
    /// `Component::child`, then mount once. To mount into an existing tree,
    /// use [`SceneTree::add_child`].
    fn into_tree(self) -> SceneTree {
        SceneTree::from_component(self)
    }
}

impl SceneTree {
    /// Creates a tree and mounts `root` under its root node.
    pub fn from_component<C: SceneChild>(root: C) -> Self {
        let mut tree = SceneTree::new();
        tree.add_child(tree.root(), root);
        tree
    }

    /// Builds `child` under `parent` and returns its node id.
    pub fn add_child<C: SceneChild>(&mut self, parent: NodeId, child: C) -> NodeId {
        child.attach(self, parent)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Probe;
    impl SceneChild for Probe {
        fn attach(self, tree: &mut SceneTree, parent: NodeId) -> NodeId {
            tree.add_node(parent, "probe")
        }
    }

    #[test]
    fn from_component_mounts_under_the_root() {
        let tree = SceneTree::from_component(Probe);
        let children = tree.children(tree.root()).unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(tree.node(children[0]).name(), "probe");
    }

    #[test]
    fn into_tree_mounts_under_the_root() {
        let tree = Probe.into_tree();
        assert_eq!(tree.children(tree.root()).unwrap().len(), 1);
    }
}
