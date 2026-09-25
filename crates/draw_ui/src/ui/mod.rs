//! The crate-internal UI implementation namespace.
//!
//! `draw_ui` owns layout, paint and input. Every `Control`'s drawing/layout
//! runtime — layout data and [`Widget`] — lives in the [`SceneTree`] node's
//! extension slot ([`Control`]); the text measurer, GUI interaction state and
//! layout cache live in the root node's [`UiRootState`]. The theme is not
//! stored here: it is a value passed to component constructors. Application
//! concerns (construction, backend submission) live in `draw_components`.
//!
//! The logic is split so each file stays small:
//!
//! - [`layout`] — resolving absolute rectangles.
//! - [`paint`] — emitting the backend-neutral `DrawList`.
//! - `input` (crate root) — hit testing and the GUI input stage.

mod layout;
mod paint;

use std::rc::Rc;

use draw_core::NodeId;
use draw_scene::SceneTree;

use crate::control::{
    bump_paint_generation, control_mut, control_of, control_visible, gui_state, root_state,
    root_state_mut, CachedText, ControlData, LayoutCache,
};
use crate::decor::{DecorRef, InteractState};
use crate::layout::{layout_text, TextMeasurer, TextOptions};
use crate::widget::Widget;

/// The UI layout/paint implementation namespace. Zero-sized: the theme, text
/// measurer, control runtime and layout cache all live on the [`SceneTree`].
pub(crate) struct Ui;

impl Default for Ui {
    fn default() -> Self {
        Self::new()
    }
}

impl Ui {
    /// Creates a UI environment handle.
    pub fn new() -> Self {
        Ui
    }

    /// Number of times the full measure/arrange pass has run.
    pub fn layout_count(&self, tree: &SceneTree) -> u64 {
        root_state(tree).map_or(0, |state| state.layout.borrow().count)
    }

    /// Number of controls arranged during the last [`layout`](Ui::layout) pass.
    pub fn last_arranged_nodes(&self, tree: &SceneTree) -> usize {
        root_state(tree).map_or(0, |state| state.layout.borrow().last_arranged)
    }

    /// Forces the next [`layout`](Ui::layout) call to recompute the whole tree.
    pub fn invalidate_layout(&mut self, tree: &mut SceneTree) {
        self.mark_all_dirty(tree);
    }

    /// Replaces the text measurer (stored on the tree root) and invalidates
    /// layout.
    pub fn set_text_measurer(&mut self, tree: &mut SceneTree, measurer: Rc<dyn TextMeasurer>) {
        {
            let state = root_state_mut(tree);
            state.text_measurer = measurer;
            state.layout.borrow_mut().text.clear();
        }
        self.mark_all_dirty(tree);
    }

    /// Lays out `text` for control `id`, reusing the cached result when the
    /// inputs are unchanged.
    pub(super) fn layout_text_cached(
        &self,
        cache: &mut LayoutCache,
        measurer: &dyn TextMeasurer,
        id: NodeId,
        text: &str,
        font_size: f32,
        width: f32,
        options: TextOptions,
    ) -> Rc<[String]> {
        if let Some(entry) = cache.text.get(&id) {
            if entry.text == text
                && entry.font_size_bits == font_size.to_bits()
                && entry.width_bits == width.to_bits()
                && entry.options == options
            {
                return entry.lines.clone();
            }
        }
        let lines: Rc<[String]> = layout_text(measurer, text, font_size, width, options).into();
        cache.text.insert(
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
    pub(crate) fn mark_dirty(&self, tree: &mut SceneTree, id: NodeId) {
        let parent = tree.parent(id);
        {
            let mut cache = root_state_mut(tree).layout.borrow_mut();
            cache.valid = false;
            cache.order.remove(&id);
            if let Some(parent) = parent {
                cache.order.remove(&parent);
            }
        }
        let mut current = Some(id);
        while let Some(node) = current {
            if let Some(control) = control_mut(tree, node) {
                if control.layout_dirty {
                    // Ancestors are already dirty by invariant.
                    break;
                }
                control.layout_dirty = true;
            }
            current = tree.parent(node);
        }
        bump_paint_generation(tree);
    }

    /// Marks the whole tree dirty (structure changed, measurer swapped, ...).
    pub(crate) fn mark_all_dirty(&self, tree: &mut SceneTree) {
        {
            let mut cache = root_state_mut(tree).layout.borrow_mut();
            cache.valid = false;
            cache.order.clear();
        }
        for id in tree.iter().collect::<Vec<_>>() {
            if let Some(control) = control_mut(tree, id) {
                control.layout_dirty = true;
            }
        }
        bump_paint_generation(tree);
    }

    /// Whether the next [`layout`](Ui::layout) call has work to do.
    pub fn needs_layout(&self, tree: &SceneTree) -> bool {
        root_state(tree).is_some_and(|state| !state.layout.borrow().valid)
    }

    /// Monotonic counter of painted-UI changes (see [`UiRootState`]).
    pub fn paint_generation(&self, tree: &SceneTree) -> u64 {
        root_state(tree).map_or(0, |state| state.paint_generation)
    }

    /// Number of controls in this UI (root included when mounted by a host).
    pub fn control_count(&self, tree: &SceneTree) -> usize {
        tree.iter()
            .filter(|id| control_of(tree, *id).is_some())
            .count()
    }

    /// Layout data for control `id`, read from the node's extension slot.
    pub fn control<'a>(&self, tree: &'a SceneTree, id: NodeId) -> Option<&'a ControlData> {
        control_of(tree, id).map(|control| &control.data)
    }

    /// The control's visual widget, read from the node's extension slot.
    pub fn widget<'a>(&self, tree: &'a SceneTree, id: NodeId) -> Option<&'a Widget> {
        control_of(tree, id).map(|control| &control.widget)
    }

    /// Attaches themed chrome to `id`, painted by [`Ui::paint`] around the
    /// control's own content.
    pub fn add_decor(&mut self, tree: &mut SceneTree, id: NodeId, decor: DecorRef) {
        if let Some(control) = control_mut(tree, id) {
            control.decorations.push(decor);
            bump_paint_generation(tree);
        }
    }

    /// Turns subtree clipping on or off for `id`.
    ///
    /// The clip itself is resolved by the next [`Ui::layout`] (every control's
    /// `clip_rect` is a function of the resolved rectangles), so this only has
    /// to invalidate layout.
    pub fn set_clip(&mut self, tree: &mut SceneTree, id: NodeId, clip: bool) {
        let changed = match control_mut(tree, id) {
            Some(control) if control.data.clip != clip => {
                control.data.clip = clip;
                true
            }
            _ => false,
        };
        if changed {
            self.mark_dirty(tree, id);
        }
    }

    /// Decorators attached to `id`, in paint order.
    pub fn decor<'a>(&self, tree: &'a SceneTree, id: NodeId) -> &'a [DecorRef] {
        control_of(tree, id)
            .map(|control| control.decorations.as_slice())
            .unwrap_or(&[])
    }

    /// Hover/pressed/focused state of `id`, inherited from its ancestors.
    pub fn state_for(&self, tree: &SceneTree, id: NodeId) -> InteractState {
        let state = gui_state(tree).copied().unwrap_or_default();
        InteractState {
            hovered: state
                .hovered
                .is_some_and(|node| is_self_or_ancestor(tree, node, id)),
            pressed: state
                .pressed
                .is_some_and(|node| is_self_or_ancestor(tree, node, id)),
            focused: state
                .focused
                .is_some_and(|node| is_self_or_ancestor(tree, node, id)),
            disabled: control_of(tree, id).is_some_and(|control| control.data.disabled),
        }
    }

    /// Control children of `id` (non-control nodes are ignored, as in Godot
    /// containers).
    pub(super) fn children_vec(&self, tree: &SceneTree, id: NodeId) -> Vec<NodeId> {
        tree.children(id)
            .map(|children| {
                children
                    .iter()
                    .copied()
                    .filter(|child| {
                        control_of(tree, *child).is_some() && control_visible(tree, *child)
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}

fn is_self_or_ancestor(tree: &SceneTree, candidate: NodeId, node: NodeId) -> bool {
    let mut current = Some(candidate);
    while let Some(id) = current {
        if id == node {
            return true;
        }
        current = tree.parent(id);
    }
    false
}
