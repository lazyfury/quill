//! The UI tree (`Ui`) and its public surface.
//!
//! `Ui` owns a [`SceneTree`] of `Control` nodes plus per-control layout
//! ([`ControlData`]) and behavior ([`Widget`]). The logic is split so each file
//! stays small:
//!
//! - [`build`] — adding controls and property setters.
//! - [`layout`] — resolving absolute rectangles.
//! - [`paint`] — emitting the backend-neutral `DrawList`.
//! - [`input`] — hit testing and event dispatch.

mod build;
mod input;
mod layout;
mod paint;

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use draw_core::{Edges, NodeId};
use draw_scene::SceneTree;

use crate::control::{ControlData, MouseFilter};
use crate::widget::{ButtonState, Widget};

/// A callback invoked when a control is activated (clicked / Enter).
pub type ClickCallback = Rc<RefCell<dyn FnMut()>>;

/// The UI tree: a [`SceneTree`] of `Control` nodes plus layout, painting and
/// input dispatch.
///
/// Layout is absolute: after [`Ui::layout`], each control has a resolved
/// viewport-space [`Rect`](draw_core::Rect) in its [`ControlData`]. Painting
/// iterates the scene tree in draw order, and input uses reverse-order hit
/// testing.
pub struct Ui {
    pub(super) tree: SceneTree,
    pub(super) root: NodeId,
    pub(super) controls: HashMap<NodeId, ControlData>,
    pub(super) widgets: HashMap<NodeId, Widget>,
    pub(super) callbacks: HashMap<NodeId, ClickCallback>,
    pub(super) hovered: Option<NodeId>,
    pub(super) pressed: Option<NodeId>,
    pub(super) focused: Option<NodeId>,
    pub(super) activated: Vec<NodeId>,
}

impl Default for Ui {
    fn default() -> Self {
        Self::new()
    }
}

impl Ui {
    /// Creates a UI with a root control that fills the viewport.
    pub fn new() -> Self {
        let mut tree = SceneTree::new();
        let tree_root = tree.root();
        let root = tree.add_control(tree_root, "Root");

        let mut controls = HashMap::new();
        controls.insert(
            root,
            ControlData {
                anchors: Edges::new(0.0, 0.0, 1.0, 1.0),
                mouse_filter: MouseFilter::Ignore,
                ..ControlData::default()
            },
        );

        Self {
            tree,
            root,
            controls,
            widgets: HashMap::new(),
            callbacks: HashMap::new(),
            hovered: None,
            pressed: None,
            focused: None,
            activated: Vec::new(),
        }
    }

    pub fn tree(&self) -> &SceneTree {
        &self.tree
    }

    pub fn tree_mut(&mut self) -> &mut SceneTree {
        &mut self.tree
    }

    pub fn root(&self) -> NodeId {
        self.root
    }

    /// Number of controls in this UI (root included).
    pub fn control_count(&self) -> usize {
        self.controls.len()
    }

    pub fn control(&self, id: NodeId) -> Option<&ControlData> {
        self.controls.get(&id)
    }

    pub fn widget(&self, id: NodeId) -> Option<&Widget> {
        self.widgets.get(&id)
    }

    pub fn hovered(&self) -> Option<NodeId> {
        self.hovered
    }

    pub fn focused(&self) -> Option<NodeId> {
        self.focused
    }

    pub fn button_state(&self, id: NodeId) -> Option<ButtonState> {
        match self.widgets.get(&id) {
            Some(Widget::Button(button)) => Some(button.state),
            _ => None,
        }
    }

    pub fn click_count(&self, id: NodeId) -> u32 {
        self.button_state(id).map_or(0, |state| state.click_count)
    }

    pub(super) fn children_vec(&self, id: NodeId) -> Vec<NodeId> {
        self.tree
            .children(id)
            .map(|children| children.to_vec())
            .unwrap_or_default()
    }
}
