//! Hit testing and event dispatch.

use super::*;
use draw_core::{EventResult, InputEvent, Key, PointerButton, Vec2};

impl Ui {
    /// Returns the topmost control under `position`, respecting visibility and
    /// [`MouseFilter`].
    pub fn hit_test(&self, position: Vec2) -> Option<NodeId> {
        self.hit_node(self.root, position)
    }

    fn hit_node(&self, id: NodeId, position: Vec2) -> Option<NodeId> {
        // Children are drawn after the parent, so test them first (topmost first).
        let children = self.children_vec(id);
        for child in children.iter().rev() {
            if !self.tree.is_visible_in_tree(*child).unwrap_or(false) {
                continue;
            }
            if let Some(hit) = self.hit_node(*child, position) {
                return Some(hit);
            }
        }
        let control = self.controls.get(&id)?;
        if control.mouse_filter != MouseFilter::Ignore && control.rect.contains(position) {
            Some(id)
        } else {
            None
        }
    }

    /// Dispatches an event. MVP does target routing only; capture/bubble is a
    /// future extension point.
    pub fn handle_input(&mut self, event: &InputEvent) -> EventResult {
        let result = match event {
            InputEvent::PointerMove { position } => {
                let hit = self.hit_test(*position);
                self.set_hover(hit);
                if hit.is_some() {
                    EventResult::Handled
                } else {
                    EventResult::Ignored
                }
            }
            InputEvent::PointerLeave => {
                self.set_hover(None);
                EventResult::Ignored
            }
            InputEvent::PointerDown {
                position,
                button: PointerButton::Left,
            } => {
                let hit = self.hit_test(*position);
                self.set_hover(hit);
                self.focused = hit;
                if let Some(id) = hit {
                    self.pressed = Some(id);
                    if let Some(Widget::Button(button)) = self.widgets.get_mut(&id) {
                        button.state.pressed = true;
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
                let hit = self.hit_test(*position);
                if let Some(pressed) = self.pressed.take() {
                    if let Some(Widget::Button(button)) = self.widgets.get_mut(&pressed) {
                        button.state.pressed = false;
                    }
                    if hit == Some(pressed) {
                        self.activate(pressed);
                    }
                }
                EventResult::Handled
            }
            InputEvent::KeyDown { key } if matches!(*key, Key::Enter | Key::Space) => {
                match self.focused {
                    Some(focused) if self.widgets.get(&focused).is_some_and(Widget::is_button) => {
                        self.activate(focused);
                        EventResult::Handled
                    }
                    _ => EventResult::Ignored,
                }
            }
            _ => EventResult::Ignored,
        };

        self.dispatch_click_callbacks();
        result
    }

    fn activate(&mut self, id: NodeId) {
        if let Some(Widget::Button(button)) = self.widgets.get_mut(&id) {
            button.state.click_count += 1;
        }
        // Dispatch even for non-`Button` nodes: themed components register
        // click callbacks there (`Ui::set_on_click`).
        self.activated.push(id);
    }

    fn dispatch_click_callbacks(&mut self) {
        let activated = std::mem::take(&mut self.activated);
        for id in activated {
            // A component root owns its click, so a hit on any descendant
            // activates the nearest ancestor with a callback.
            let mut current = Some(id);
            let mut callback = None;
            while let Some(node) = current {
                if let Some(registered) = self.callbacks.get(&node).cloned() {
                    callback = Some(registered);
                    break;
                }
                current = self.tree.parent(node);
            }
            if let Some(callback) = callback {
                (callback.borrow_mut())();
            }
        }
    }

    fn set_hover(&mut self, hit: Option<NodeId>) {
        if self.hovered == hit {
            return;
        }
        if let Some(old) = self.hovered {
            if let Some(Widget::Button(button)) = self.widgets.get_mut(&old) {
                button.state.hovered = false;
            }
        }
        self.hovered = hit;
        if let Some(new) = hit {
            if let Some(Widget::Button(button)) = self.widgets.get_mut(&new) {
                button.state.hovered = true;
            }
        }
    }
}
