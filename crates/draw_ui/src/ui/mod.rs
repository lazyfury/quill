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
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use draw_core::{Edges, NodeId, Viewport};
use draw_scene::SceneTree;

use crate::control::{ControlData, MouseFilter};
use crate::layout::{layout_text, ApproxTextMeasurer, ContentSize, TextMeasurer, TextOptions};
use crate::widget::{ButtonState, Widget};

/// Cached laid-out text for one control (paint-side).
pub(super) struct CachedText {
    text: String,
    font_size_bits: u32,
    width_bits: u32,
    options: TextOptions,
    lines: Rc<[String]>,
}

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
    pub(super) text_measurer: Rc<dyn TextMeasurer>,
    pub(super) layout_valid: bool,
    pub(super) layout_viewport: Viewport,
    pub(super) layout_count: u64,
    /// Nodes whose layout inputs changed (plus their ancestors).
    pub(super) dirty: HashSet<NodeId>,
    /// Cached child ordering per container (`LayoutStyle::order`).
    pub(super) order_cache: RefCell<HashMap<NodeId, Vec<NodeId>>>,
    /// Nodes arranged during the last [`layout`](Ui::layout) pass.
    pub(super) last_arranged: usize,
    /// Paint-side cache of wrapped/clipped lines per control.
    pub(super) text_cache: RefCell<HashMap<NodeId, CachedText>>,
    /// Per-pass memoization of `measure_node` results.
    pub(super) measure_cache: RefCell<HashMap<(NodeId, u32, u32), ContentSize>>,
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
            text_measurer: Rc::new(ApproxTextMeasurer),
            layout_valid: false,
            layout_viewport: Viewport::default(),
            layout_count: 0,
            dirty: HashSet::new(),
            order_cache: RefCell::new(HashMap::new()),
            last_arranged: 0,
            text_cache: RefCell::new(HashMap::new()),
            measure_cache: RefCell::new(HashMap::new()),
        }
    }

    pub fn tree(&self) -> &SceneTree {
        &self.tree
    }

    pub fn tree_mut(&mut self) -> &mut SceneTree {
        self.mark_all_dirty();
        &mut self.tree
    }

    pub fn root(&self) -> NodeId {
        self.root
    }

    /// Number of times the full measure/arrange pass has run.
    pub fn layout_count(&self) -> u64 {
        self.layout_count
    }

    /// Number of controls arranged during the last [`layout`](Ui::layout)
    /// pass. Lower than `control_count()` when a change only dirtied part of
    /// the tree.
    pub fn last_arranged_nodes(&self) -> usize {
        self.last_arranged
    }

    /// Forces the next [`layout`](Ui::layout) call to recompute the whole tree.
    pub fn invalidate_layout(&mut self) {
        self.mark_all_dirty();
    }

    /// Replaces the text measurer and invalidates layout.
    pub fn set_text_measurer(&mut self, measurer: Rc<dyn TextMeasurer>) {
        self.text_measurer = measurer;
        self.text_cache.borrow_mut().clear();
        self.mark_all_dirty();
    }

    /// Lays out `text` for control `id`, reusing the cached result when the
    /// inputs are unchanged.
    pub(super) fn layout_text_cached(
        &self,
        id: NodeId,
        text: &str,
        font_size: f32,
        width: f32,
        options: TextOptions,
    ) -> Rc<[String]> {
        let mut cache = self.text_cache.borrow_mut();
        if let Some(entry) = cache.get(&id) {
            if entry.text == text
                && entry.font_size_bits == font_size.to_bits()
                && entry.width_bits == width.to_bits()
                && entry.options == options
            {
                return entry.lines.clone();
            }
        }
        let lines: Rc<[String]> =
            layout_text(self.text_measurer.as_ref(), text, font_size, width, options).into();
        cache.insert(
            id,
            CachedText {
                text: text.to_string(),
                font_size_bits: font_size.to_bits(),
                width_bits: width.to_bits(),
                options,
                lines: lines.clone(),
            },
        );
        lines
    }

    /// Marks `id` and all of its ancestors as needing layout, and drops the
    /// cached child ordering for `id` and its parent.
    pub(super) fn mark_dirty(&mut self, id: NodeId) {
        self.layout_valid = false;
        self.order_cache.borrow_mut().remove(&id);
        if let Some(parent) = self.tree.parent(id) {
            self.order_cache.borrow_mut().remove(&parent);
        }
        let mut current = Some(id);
        while let Some(node) = current {
            if !self.dirty.insert(node) {
                // Ancestors are already dirty by invariant.
                break;
            }
            current = self.tree.parent(node);
        }
    }

    /// Marks the whole tree dirty (structure changed, measurer swapped, ...).
    pub(super) fn mark_all_dirty(&mut self) {
        self.layout_valid = false;
        self.dirty.clear();
        self.dirty.extend(self.controls.keys().copied());
        self.order_cache.borrow_mut().clear();
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

    /// Whether the pointer is currently over a clickable button.
    ///
    /// Hosts use this to give cursor feedback (e.g. the Canvas runner sets a
    /// `pointer` CSS cursor).
    pub fn hovered_is_button(&self) -> bool {
        self.hovered
            .is_some_and(|id| self.widgets.get(&id).is_some_and(Widget::is_button))
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
