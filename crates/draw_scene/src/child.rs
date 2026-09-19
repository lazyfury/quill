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
/// `tree.add_child(parent, component)`.
pub trait SceneChild {
    /// Builds `self` under `parent`, returning the node it created.
    fn attach(self, tree: &mut SceneTree, parent: NodeId) -> NodeId;
}

impl SceneTree {
    /// Builds `child` under `parent` and returns its node id.
    pub fn add_child<C: SceneChild>(&mut self, parent: NodeId, child: C) -> NodeId {
        child.attach(self, parent)
    }
}
