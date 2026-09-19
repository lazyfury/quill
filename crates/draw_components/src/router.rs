//! A view router: a container that shows exactly one child view at a time.
//!
//! Routing is explicit and cheap. All views are built once as children of a
//! container; the router toggles scene visibility so only the active one is laid
//! out, painted and hit-tested (`draw_ui` skips hidden controls). Because the
//! views stay mounted, their state (scroll position, focus, text bindings)
//! survives a switch.
//!
//! ```ignore
//! use std::cell::Cell;
//! use std::rc::Rc;
//!
//! let route = Rc::new(Cell::new(0));
//! let detail = tree.add_child(root, Panel::new().color(Color::TRANSPARENT).flat());
//! let notes = build_notes_view(&mut tree, detail);      // a child of `detail`
//! let settings = build_settings_view(&mut tree, detail);
//!
//! let mut router = Router::with_route(detail, route.clone());
//! router.add_node(notes);
//! router.add_node(settings);
//! router.sync(&mut tree);                                // apply route 0
//!
//! // Click callbacks switch by writing to the shared cell:
//! // Button::new("Settings").on_click({ let r = route.clone(); move || r.set(1) });
//!
//! // In the frame update, after events, apply the route:
//! router.sync(&mut tree);
//! ```

use std::cell::Cell;
use std::rc::Rc;

use draw_core::NodeId;
use draw_scene::SceneTree;

use crate::base::Component;

/// A container that shows exactly one of its child views.
///
/// Wrap a control (typically a transparent panel that fills its parent) and
/// register each view with [`add`](Router::add) / [`add_node`](Router::add_node).
/// The active view is chosen by a shared [`Rc<Cell<usize>>`] route; call
/// [`sync`](Router::sync) once per frame to apply it.
pub struct Router {
    root: NodeId,
    route: Rc<Cell<usize>>,
    views: Vec<NodeId>,
    names: Vec<&'static str>,
}

impl Router {
    /// Wraps `root` as a router with a fresh route starting at view `0`.
    pub fn new(root: NodeId) -> Self {
        Self::with_route(root, Rc::new(Cell::new(0)))
    }

    /// Wraps `root` and shares `route` with the caller, so click callbacks can
    /// switch views by writing to the cell.
    pub fn with_route(root: NodeId, route: Rc<Cell<usize>>) -> Self {
        Self {
            root,
            route,
            views: Vec::new(),
            names: Vec::new(),
        }
    }

    /// The shared route cell (clone it into click callbacks).
    pub fn route(&self) -> Rc<Cell<usize>> {
        self.route.clone()
    }

    /// The router container node.
    pub fn root(&self) -> NodeId {
        self.root
    }

    /// Builds `view` under the router, tracks it, and returns its node.
    pub fn add<C: Component + 'static>(&mut self, tree: &mut SceneTree, view: C) -> NodeId {
        self.add_named(tree, "", view)
    }

    /// Like [`add`](Router::add) but also registers a route name.
    pub fn add_named<C: Component + 'static>(
        &mut self,
        tree: &mut SceneTree,
        name: &'static str,
        view: C,
    ) -> NodeId {
        let id = view.build(tree, self.root);
        self.views.push(id);
        self.names.push(name);
        id
    }

    /// Tracks an already-built node as a view (e.g. one whose internals were
    /// captured while composing it directly under the router).
    pub fn add_node(&mut self, id: NodeId) -> usize {
        self.views.push(id);
        self.names.push("");
        self.views.len() - 1
    }

    /// Number of registered views.
    pub fn len(&self) -> usize {
        self.views.len()
    }

    /// Whether no views are registered.
    pub fn is_empty(&self) -> bool {
        self.views.is_empty()
    }

    /// Current route index (may exceed the view count until clamped by
    /// [`sync`](Router::sync)).
    pub fn index(&self) -> usize {
        self.route.get()
    }

    /// Node of view `index`, if registered.
    pub fn view(&self, index: usize) -> Option<NodeId> {
        self.views.get(index).copied()
    }

    /// Name of view `index`, if it was registered with one.
    pub fn name(&self, index: usize) -> Option<&'static str> {
        self.names
            .get(index)
            .copied()
            .filter(|name| !name.is_empty())
    }

    /// Index of the view registered under `name`.
    pub fn index_of(&self, name: &str) -> Option<usize> {
        self.names.iter().position(|registered| *registered == name)
    }

    /// Applies the current route immediately: shows the target view, hides the
    /// rest, and marks the changed ones for relayout.
    pub fn sync(&mut self, tree: &mut SceneTree) {
        if self.views.is_empty() {
            return;
        }
        let index = self.route.get().min(self.views.len() - 1);
        for (i, id) in self.views.iter().enumerate() {
            let want = i == index;
            if tree.is_visible(*id) != Some(want) {
                tree.set_visible(*id, want);
                draw_ui::mark_dirty(tree, *id);
            }
        }
    }

    /// Sets the route to `index` (clamped by [`sync`](Router::sync)) and applies
    /// it immediately.
    pub fn go(&mut self, tree: &mut SceneTree, index: usize) {
        self.route.set(index);
        self.sync(tree);
    }

    /// Sets the route by name and applies it immediately. Returns `false` if no
    /// view has that name.
    pub fn go_name(&mut self, tree: &mut SceneTree, name: &str) -> bool {
        match self.index_of(name) {
            Some(index) => {
                self.go(tree, index);
                true
            }
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use draw_core::Color;
    use draw_scene::SceneTree;

    use crate::base::Panel;

    fn setup() -> (SceneTree, Router, NodeId, NodeId) {
        let mut tree = SceneTree::new();
        let host = tree.add_child(tree.root(), Panel::new().color(Color::TRANSPARENT).flat());
        let a = tree.add_child(host, Panel::new().color(Color::RED));
        let b = tree.add_child(host, Panel::new().color(Color::BLUE));
        let mut router = Router::new(host);
        router.add_node(a);
        router.add_node(b);
        router.sync(&mut tree);
        tree.update();
        (tree, router, a, b)
    }

    #[test]
    fn only_the_active_view_is_visible() {
        let (tree, _router, a, b) = setup();
        assert_eq!(tree.is_visible(a), Some(true));
        assert_eq!(tree.is_visible(b), Some(false));
    }

    #[test]
    fn go_switches_the_visible_view() {
        let (mut tree, mut router, a, b) = setup();
        router.go(&mut tree, 1);
        assert_eq!(tree.is_visible(a), Some(false));
        assert_eq!(tree.is_visible(b), Some(true));
    }

    #[test]
    fn named_routes_resolve() {
        let mut tree = SceneTree::new();
        let host = tree.add_child(tree.root(), Panel::new().color(Color::TRANSPARENT).flat());
        let mut router = Router::new(host);
        router.add_named(&mut tree, "notes", Panel::new());
        router.add_named(&mut tree, "settings", Panel::new());

        assert_eq!(router.len(), 2);
        assert_eq!(router.name(1), Some("settings"));
        assert_eq!(router.index_of("settings"), Some(1));
        assert!(router.go_name(&mut tree, "settings"));
        assert_eq!(router.index(), 1);
        assert!(!router.go_name(&mut tree, "missing"));
    }

    #[test]
    fn add_builds_components_under_the_router() {
        let mut tree = SceneTree::new();
        let host = tree.add_child(tree.root(), Panel::new().color(Color::TRANSPARENT).flat());
        let mut router = Router::new(host);
        let view = router.add(&mut tree, Panel::new());
        assert_eq!(router.view(0), Some(view));
        assert_eq!(tree.parent(view), Some(host));
    }
}
