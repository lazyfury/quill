//! GUI hit testing, event dispatch and the backend-neutral input router.
//!
//! Owns the interaction stage of the pipeline over the control data and the
//! viewport GUI state: hit testing, the `_gui_input` stage, and the
//! `_input -> world -> GUI -> _unhandled_input` order hosts run through
//! [`route_input`].

use draw_core::{Cursor, EventResult, InputEvent, Key, NodeId, PointerButton, Vec2};
use draw_scene::SceneTree;

use crate::control::{control_visible, Control, MouseFilter};
use crate::widget::Widget;

/// Controls whose parent is not itself a control (the UI roots).
fn root_controls(tree: &SceneTree) -> Vec<NodeId> {
    tree.iter()
        .filter(|id| {
            tree.data::<Control>(*id).is_some()
                && control_visible(tree, *id)
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
        if !control_visible(tree, *child) {
            continue;
        }
        if let Some(hit) = hit_node(tree, *child, position) {
            return Some(hit);
        }
    }
    let control = tree.data::<Control>(id)?;
    // A control clipped away by an ancestor (`clip_rect` is inherited, so it is
    // already intersected with every clipping ancestor) is not clickable, even
    // where its own rectangle covers the pointer. Without this a half-scrolled
    // row would still take clicks below the list viewport.
    if control
        .data
        .clip_rect
        .is_some_and(|clip| !clip.contains(position))
    {
        return None;
    }
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
        InputEvent::Wheel { position, delta } => {
            // Scrolling is owned, not global: the nearest ancestor of the
            // control under the pointer that registered a scroll callback gets
            // the delta (a list scrolls itself), and anything else stays
            // `Ignored` so the event can fall through to `_unhandled_input`.
            let Some(owner) =
                hit_test(tree, *position).and_then(|id| nearest_with_scroll(tree, id))
            else {
                return EventResult::Ignored;
            };
            let Some(callback) = tree
                .data::<Control>(owner)
                .and_then(|control| control.scroll_callback.clone())
            else {
                return EventResult::Ignored;
            };
            (callback.borrow_mut())(*delta);
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

/// Nearest ancestor (including `id`) that owns a scroll callback.
fn nearest_with_scroll(tree: &SceneTree, id: NodeId) -> Option<NodeId> {
    let mut current = Some(id);
    while let Some(node) = current {
        if tree
            .data::<Control>(node)
            .is_some_and(|control| control.scroll_callback.is_some())
        {
            return Some(node);
        }
        current = tree.parent(node);
    }
    None
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::control::ControlData;
    use crate::layout::TextOptions;
    use crate::widget::Widget;
    use draw_core::{Color, Edges, Size, ViewportSize};
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    fn panel() -> Widget {
        Widget::Panel {
            color: Color::RED,
            border: None,
        }
    }

    fn label(text: &str) -> Widget {
        Widget::Label {
            text: text.to_string(),
            font_size: 12.0,
            color: Color::WHITE,
            options: TextOptions::default(),
        }
    }

    fn add(tree: &mut SceneTree, parent: NodeId, data: ControlData, widget: Widget) -> NodeId {
        let id = tree.add_control(parent, "test");
        tree.set_data(id, Control::new(data, widget));
        id
    }

    /// Anchored child at a pixel rectangle, as the list positions its rows.
    fn slab(
        tree: &mut SceneTree,
        parent: NodeId,
        left: f32,
        top: f32,
        right: f32,
        bottom: f32,
        widget: Widget,
    ) -> NodeId {
        add(
            tree,
            parent,
            ControlData {
                anchors: Edges::ZERO,
                offsets: Edges::new(left, top, right, bottom),
                ..ControlData::default()
            },
            widget,
        )
    }

    /// Point membership is not enough: a control can cover the pointer and
    /// still be cut away by an ancestor's clip.
    #[test]
    fn hit_testing_respects_the_clip() {
        let mut tree = SceneTree::new();
        let root = tree.root();
        let container = add(&mut tree, root, ControlData::fill_parent(), panel());
        let clipper = add(
            &mut tree,
            container,
            ControlData {
                anchors: Edges::ZERO,
                offsets: Edges::new(0.0, 0.0, 100.0, 100.0),
                clip: true,
                ..ControlData::default()
            },
            panel(),
        );
        let overhang = slab(&mut tree, clipper, 0.0, 0.0, 300.0, 20.0, label("overhang"));

        crate::layout(&mut tree, ViewportSize::new(Size::new(400.0, 400.0)));

        assert_eq!(
            hit_test(&tree, Vec2::new(50.0, 10.0)),
            Some(overhang),
            "inside the clip the overhanging control is the target"
        );
        assert_eq!(
            hit_test(&tree, Vec2::new(150.0, 10.0)),
            Some(container),
            "outside the clip the overhanging control is not there at all — the \
             pointer falls through to what is behind it"
        );
    }

    /// The wheel is owned, not global: the nearest ancestor with a scroll
    /// callback takes it, and a tree without one stays unhandled.
    #[test]
    fn the_wheel_goes_to_the_nearest_scroll_owner() {
        let mut tree = SceneTree::new();
        let root = tree.root();
        // A control whose parent is not a control is pinned to the viewport by
        // layout, so the scrollable panes live under a full-size one.
        let container = add(&mut tree, root, ControlData::fill_parent(), panel());
        let outer = slab(&mut tree, container, 0.0, 0.0, 200.0, 200.0, panel());
        let inner = slab(&mut tree, outer, 0.0, 0.0, 100.0, 50.0, panel());
        slab(&mut tree, inner, 0.0, 0.0, 100.0, 20.0, label("row"));
        crate::layout(&mut tree, ViewportSize::new(Size::new(400.0, 400.0)));

        let outer_scrolls = Rc::new(Cell::new(0.0));
        let inner_scrolls = Rc::new(Cell::new(0.0));
        for (id, total) in [
            (outer, outer_scrolls.clone()),
            (inner, inner_scrolls.clone()),
        ] {
            tree.data_mut::<Control>(id).unwrap().scroll_callback =
                Some(Rc::new(RefCell::new(move |delta: Vec2| {
                    total.set(total.get() + delta.y)
                })));
        }

        let handled = handle_input(
            &mut tree,
            &InputEvent::Wheel {
                position: Vec2::new(50.0, 10.0),
                delta: Vec2::new(0.0, 12.0),
            },
        );
        assert_eq!(handled, EventResult::Handled);
        assert_eq!(inner_scrolls.get(), 12.0, "the innermost owner wins");
        assert_eq!(outer_scrolls.get(), 0.0, "and the outer one is not asked");

        // Somewhere else entirely: nobody claims it, so the event can still
        // reach `_unhandled_input`.
        let handled = handle_input(
            &mut tree,
            &InputEvent::Wheel {
                position: Vec2::new(300.0, 300.0),
                delta: Vec2::new(0.0, 12.0),
            },
        );
        assert_eq!(handled, EventResult::Ignored);
        assert_eq!(inner_scrolls.get(), 12.0);
    }

    #[test]
    fn a_wheel_over_a_control_without_a_owner_is_ignored() {
        let mut tree = SceneTree::new();
        let root = tree.root();
        let container = add(&mut tree, root, ControlData::fill_parent(), panel());
        add(
            &mut tree,
            container,
            ControlData::fill_parent(),
            label("nothing to scroll"),
        );
        crate::layout(&mut tree, ViewportSize::new(Size::new(400.0, 400.0)));

        assert_eq!(
            handle_input(
                &mut tree,
                &InputEvent::Wheel {
                    position: Vec2::new(10.0, 10.0),
                    delta: Vec2::new(0.0, 12.0),
                },
            ),
            EventResult::Ignored
        );
    }
}
