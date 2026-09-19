//! GUI hit testing, event dispatch and the backend-neutral input router.
//!
//! Owns the interaction stage of the pipeline over the control data and the
//! viewport GUI state: hit testing, the `_gui_input` stage, and the
//! `_input -> world -> GUI -> _unhandled_input` order hosts run through
//! [`route_input`].

use draw_core::{Cursor, EventResult, InputEvent, Key, NodeId, PointerButton, Vec2};
use draw_scene::SceneTree;

use crate::control::{Control, MouseFilter};
use crate::widget::Widget;

/// Controls whose parent is not itself a control (the UI roots).
fn root_controls(tree: &SceneTree) -> Vec<NodeId> {
    tree.iter()
        .filter(|id| {
            tree.data::<Control>(*id).is_some()
                && tree
                    .parent(*id)
                    .map_or(true, |parent| tree.data::<Control>(parent).is_none())
        })
        .collect()
}

/// Control children of `id` (non-control nodes are ignored).
fn control_children(tree: &SceneTree, id: NodeId) -> Vec<NodeId> {
    tree.children(id)
        .map(|children| {
            children
                .iter()
                .copied()
                .filter(|child| tree.data::<Control>(*child).is_some())
                .collect()
        })
        .unwrap_or_default()
}

/// Returns the topmost control under `position`, respecting visibility and
/// [`MouseFilter`].
pub fn hit_test(tree: &SceneTree, position: Vec2) -> Option<NodeId> {
    for root in root_controls(tree).into_iter().rev() {
        if let Some(hit) = hit_node(tree, root, position) {
            return Some(hit);
        }
    }
    None
}

fn hit_node(tree: &SceneTree, id: NodeId, position: Vec2) -> Option<NodeId> {
    // Children are drawn after the parent, so test them first (topmost first).
    for child in control_children(tree, id).iter().rev() {
        if !tree.is_visible_in_tree(*child).unwrap_or(false) {
            continue;
        }
        if let Some(hit) = hit_node(tree, *child, position) {
            return Some(hit);
        }
    }
    let control = tree.data::<Control>(id)?;
    if control.data.mouse_filter != MouseFilter::Ignore && control.data.rect.contains(position) {
        Some(id)
    } else {
        None
    }
}

/// Runs the GUI input stage (`_gui_input` in Godot).
///
/// `SceneTree::route_input_with` owns `_input`, world pick and
/// `_unhandled_input` around it.
pub fn handle_input(tree: &mut SceneTree, event: &InputEvent) -> EventResult {
    match event {
        InputEvent::PointerMove { position } => {
            // Pointer capture: while dragging, the owning node receives every
            // move (even outside its rect) and hover is not re-evaluated.
            if let Some(dragging) = crate::gui_state_of(tree).and_then(|state| state.dragging) {
                let delta = *position
                    - crate::gui_state_of(tree).map_or(Vec2::ZERO, |state| state.drag_last);
                crate::gui_state_mut(tree).drag_last = *position;
                if let Some(callback) = tree
                    .data::<Control>(dragging)
                    .and_then(|control| control.drag_callback.clone())
                {
                    (callback.borrow_mut())(tree, crate::DragPhase::Move, delta);
                }
                return EventResult::Handled;
            }
            let hit = hit_test(tree, *position);
            set_hover(tree, hit);
            if hit.is_some() {
                EventResult::Handled
            } else {
                EventResult::Ignored
            }
        }
        InputEvent::PointerLeave => {
            set_hover(tree, None);
            EventResult::Ignored
        }
        InputEvent::PointerDown {
            position,
            button: PointerButton::Left,
        } => {
            let hit = hit_test(tree, *position);
            set_hover(tree, hit);
            // A drag handle (or an ancestor) captures the pointer on down.
            if let Some(drag) = hit.and_then(|id| nearest_with_drag(tree, id)) {
                {
                    let state = crate::gui_state_mut(tree);
                    state.focused = Some(drag);
                    state.dragging = Some(drag);
                    state.pressed = Some(drag);
                    state.drag_last = *position;
                }
                if let Some(callback) = tree
                    .data::<Control>(drag)
                    .and_then(|control| control.drag_callback.clone())
                {
                    (callback.borrow_mut())(tree, crate::DragPhase::Start, Vec2::ZERO);
                }
                return EventResult::Handled;
            }
            crate::gui_state_mut(tree).focused = hit;
            if let Some(id) = hit {
                crate::gui_state_mut(tree).pressed = Some(id);
                if let Some(control) = tree.data_mut::<Control>(id) {
                    if let Widget::Button(button) = &mut control.widget {
                        button.state.pressed = true;
                    }
                }
                EventResult::Handled
            } else {
                EventResult::Ignored
            }
        }
        InputEvent::PointerUp {
            position,
            button: PointerButton::Left,
        } => {
            if let Some(dragging) = crate::gui_state_of(tree).and_then(|state| state.dragging) {
                if let Some(callback) = tree
                    .data::<Control>(dragging)
                    .and_then(|control| control.drag_callback.clone())
                {
                    (callback.borrow_mut())(tree, crate::DragPhase::End, Vec2::ZERO);
                }
                let state = crate::gui_state_mut(tree);
                state.dragging = None;
                state.pressed = None;
                return EventResult::Handled;
            }
            let hit = hit_test(tree, *position);
            let pressed = crate::gui_state_of(tree).and_then(|state| state.pressed);
            if let Some(pressed) = pressed {
                if let Some(control) = tree.data_mut::<Control>(pressed) {
                    if let Widget::Button(button) = &mut control.widget {
                        button.state.pressed = false;
                    }
                }
                if hit == Some(pressed) {
                    activate(tree, pressed);
                }
            }
            crate::gui_state_mut(tree).pressed = None;
            EventResult::Handled
        }
        InputEvent::KeyDown { key } if matches!(*key, Key::Enter | Key::Space) => {
            let focused = crate::gui_state_of(tree).and_then(|state| state.focused);
            match focused {
                Some(focused)
                    if tree
                        .data::<Control>(focused)
                        .is_some_and(|control| control.widget.is_button()) =>
                {
                    activate(tree, focused);
                    EventResult::Handled
                }
                _ => EventResult::Ignored,
            }
        }
        _ => EventResult::Ignored,
    }
}

/// Runs the full routing order (`_input` -> world -> GUI -> `_unhandled_input`).
pub fn route_input(tree: &mut SceneTree, event: &InputEvent) -> EventResult {
    tree.route_input_with(&mut GuiStage, event)
}

struct GuiStage;

impl draw_scene::GuiInput for GuiStage {
    fn gui_input(&mut self, tree: &mut SceneTree, event: &InputEvent) -> EventResult {
        handle_input(tree, event)
    }
}

fn activate(tree: &mut SceneTree, id: NodeId) {
    if let Some(control) = tree.data_mut::<Control>(id) {
        if let Widget::Button(button) = &mut control.widget {
            button.state.click_count += 1;
        }
    }
    // Dispatch to the nearest ancestor with a callback: a component root owns
    // its click, so a hit on a descendant activates it.
    let mut current = Some(id);
    let mut callback = None;
    while let Some(node) = current {
        if let Some(registered) = tree
            .data::<Control>(node)
            .and_then(|control| control.callback.clone())
        {
            callback = Some(registered);
            break;
        }
        current = tree.parent(node);
    }
    if let Some(callback) = callback {
        (callback.borrow_mut())();
    }
}

/// Nearest ancestor (including `id`) that owns a drag callback.
fn nearest_with_drag(tree: &SceneTree, id: NodeId) -> Option<NodeId> {
    let mut current = Some(id);
    while let Some(node) = current {
        if tree
            .data::<Control>(node)
            .is_some_and(|control| control.drag_callback.is_some())
        {
            return Some(node);
        }
        current = tree.parent(node);
    }
    None
}

fn set_hover(tree: &mut SceneTree, hit: Option<NodeId>) {
    let old = crate::gui_state_of(tree).and_then(|state| state.hovered);
    if old == hit {
        return;
    }
    if let Some(old) = old {
        if let Some(control) = tree.data_mut::<Control>(old) {
            if let Widget::Button(button) = &mut control.widget {
                button.state.hovered = false;
            }
        }
    }
    if let Some(new) = hit {
        if let Some(control) = tree.data_mut::<Control>(new) {
            if let Widget::Button(button) = &mut control.widget {
                button.state.hovered = true;
            }
        }
    }
    crate::gui_state_mut(tree).hovered = hit;
}

// -- queries -----------------------------------------------------------------

/// The node currently under the pointer.
pub fn hovered(tree: &SceneTree) -> Option<NodeId> {
    crate::gui_state_of(tree).and_then(|state| state.hovered)
}

/// Cursor the host should show for the current pointer position.
///
/// The hovered control's explicit [`Cursor`] wins (walking up to the nearest
/// ancestor that set one); otherwise a control with a click or drag callback
/// reports [`Cursor::Pointer`].
pub fn hovered_cursor(tree: &SceneTree) -> Cursor {
    let Some(hit) = hovered(tree) else {
        return Cursor::Default;
    };
    let mut interactive = false;
    let mut current = Some(hit);
    while let Some(node) = current {
        if let Some(control) = tree.data::<Control>(node) {
            if let Some(provider) = &control.cursor_provider {
                let cursor = provider();
                if cursor != Cursor::Default {
                    return cursor;
                }
            }
            if control.data.cursor != Cursor::Default {
                return control.data.cursor;
            }
            if control.callback.is_some() || control.drag_callback.is_some() {
                interactive = true;
            }
        }
        current = tree.parent(node);
    }
    if interactive {
        Cursor::Pointer
    } else {
        Cursor::Default
    }
}

/// Whether the pointer is currently over a clickable button.
pub fn hovered_is_button(tree: &SceneTree) -> bool {
    hovered(tree).is_some_and(|id| {
        tree.data::<Control>(id)
            .is_some_and(|c| c.widget.is_button())
    })
}

/// The focused node, from the viewport GUI state.
pub fn focused(tree: &SceneTree) -> Option<NodeId> {
    crate::gui_state_of(tree).and_then(|state| state.focused)
}

/// Whether `id` or any ancestor has a click callback.
pub fn is_interactive(tree: &SceneTree, id: NodeId) -> bool {
    let mut current = Some(id);
    while let Some(node) = current {
        if tree
            .data::<Control>(node)
            .is_some_and(|control| control.callback.is_some() || control.drag_callback.is_some())
        {
            return true;
        }
        current = tree.parent(node);
    }
    false
}
