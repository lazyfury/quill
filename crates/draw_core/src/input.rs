use std::collections::HashSet;

use crate::vec2::Vec2;

/// Which pointer button produced an event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PointerButton {
    Left,
    Right,
    Middle,
}

/// A minimal keyboard key model (no full keymap / text shaping in MVP).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Key {
    /// A printable character.
    Character(char),
    Enter,
    Escape,
    Backspace,
    Delete,
    Tab,
    Space,
    Home,
    End,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    /// Function keys `F1`..`F12` (e.g. debug shortcuts; some OSes intercept
    /// the top row unless "use F1, F2.. as standard function keys" is enabled).
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
}

/// A backend-neutral input event delivered to the scene/UI.
///
/// Deliberately small for the MVP. Capture/bubble phases are a future
/// extension; event routing currently targets a single control.
#[derive(Debug, Clone, PartialEq)]
pub enum InputEvent {
    PointerDown {
        position: Vec2,
        button: PointerButton,
    },
    PointerUp {
        position: Vec2,
        button: PointerButton,
    },
    PointerMove {
        position: Vec2,
    },
    PointerLeave,
    /// Mouse wheel / trackpad scroll. `delta` is in logical pixels; `y > 0`
    /// scrolls down, `x > 0` scrolls right.
    Wheel {
        position: Vec2,
        delta: Vec2,
    },
    KeyDown {
        key: Key,
    },
    KeyUp {
        key: Key,
    },
    /// Committed text input (IME / typing).
    TextInput {
        text: String,
    },
}

impl InputEvent {
    /// The pointer position, when the event carries one.
    pub fn position(&self) -> Option<Vec2> {
        match self {
            Self::PointerDown { position, .. }
            | Self::PointerUp { position, .. }
            | Self::PointerMove { position }
            | Self::Wheel { position, .. } => Some(*position),
            _ => None,
        }
    }
}

/// Whether an event was consumed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventResult {
    Ignored,
    Handled,
}

impl EventResult {
    pub fn is_handled(self) -> bool {
        matches!(self, Self::Handled)
    }
}

/// Tracks held pointer buttons and keys, plus the last pointer position.
///
/// Hosts feed every raw [`InputEvent`] through [`apply`](InputState::apply) and
/// can then query `is_pointer_down` / `is_key_down` while handling a later
/// event (drag, held-key repeat, modifier state). This is the backend-neutral
/// replacement for polling the OS directly.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct InputState {
    pointer: Vec2,
    buttons: HashSet<PointerButton>,
    keys: HashSet<Key>,
}

impl InputState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Last known pointer position in logical viewport coordinates.
    pub fn pointer(&self) -> Vec2 {
        self.pointer
    }

    pub fn is_pointer_down(&self, button: PointerButton) -> bool {
        self.buttons.contains(&button)
    }

    pub fn is_key_down(&self, key: Key) -> bool {
        self.keys.contains(&key)
    }

    /// Clears all held state (e.g. when the window loses focus).
    pub fn release_all(&mut self) {
        self.buttons.clear();
        self.keys.clear();
    }

    /// Updates the held state from an event. Call before routing the event, so
    /// handlers observe the state *after* this event.
    pub fn apply(&mut self, event: &InputEvent) {
        match event {
            InputEvent::PointerDown { position, button } => {
                self.pointer = *position;
                self.buttons.insert(*button);
            }
            InputEvent::PointerUp { position, button } => {
                self.pointer = *position;
                self.buttons.remove(button);
            }
            InputEvent::PointerMove { position } | InputEvent::Wheel { position, .. } => {
                self.pointer = *position;
            }
            InputEvent::KeyDown { key } => {
                self.keys.insert(*key);
            }
            InputEvent::KeyUp { key } => {
                self.keys.remove(key);
            }
            InputEvent::PointerLeave | InputEvent::TextInput { .. } => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn position_accessor() {
        let event = InputEvent::PointerDown {
            position: Vec2::new(3.0, 4.0),
            button: PointerButton::Left,
        };
        assert_eq!(event.position(), Some(Vec2::new(3.0, 4.0)));
        assert_eq!(InputEvent::PointerLeave.position(), None);
        assert_eq!(
            InputEvent::Wheel {
                position: Vec2::new(1.0, 2.0),
                delta: Vec2::ZERO,
            }
            .position(),
            Some(Vec2::new(1.0, 2.0))
        );
        assert!(EventResult::Handled.is_handled());
        assert!(!EventResult::Ignored.is_handled());
    }

    #[test]
    fn input_state_tracks_held_buttons_and_keys() {
        let mut state = InputState::new();
        state.apply(&InputEvent::PointerDown {
            position: Vec2::new(5.0, 6.0),
            button: PointerButton::Left,
        });
        state.apply(&InputEvent::KeyDown {
            key: Key::Character('a'),
        });
        assert_eq!(state.pointer(), Vec2::new(5.0, 6.0));
        assert!(state.is_pointer_down(PointerButton::Left));
        assert!(!state.is_pointer_down(PointerButton::Right));
        assert!(state.is_key_down(Key::Character('a')));

        state.apply(&InputEvent::PointerMove {
            position: Vec2::new(7.0, 8.0),
        });
        assert_eq!(state.pointer(), Vec2::new(7.0, 8.0));
        assert!(state.is_pointer_down(PointerButton::Left));

        state.apply(&InputEvent::PointerUp {
            position: Vec2::new(7.0, 8.0),
            button: PointerButton::Left,
        });
        state.apply(&InputEvent::KeyUp {
            key: Key::Character('a'),
        });
        assert!(!state.is_pointer_down(PointerButton::Left));
        assert!(!state.is_key_down(Key::Character('a')));

        state.apply(&InputEvent::KeyDown { key: Key::Enter });
        state.release_all();
        assert!(!state.is_key_down(Key::Enter));
    }

    #[test]
    fn function_keys_are_distinct() {
        let keys = [
            Key::F1,
            Key::F2,
            Key::F3,
            Key::F4,
            Key::F5,
            Key::F6,
            Key::F7,
            Key::F8,
            Key::F9,
            Key::F10,
            Key::F11,
            Key::F12,
        ];
        for (i, key) in keys.iter().enumerate() {
            for other in &keys[i + 1..] {
                assert_ne!(key, other);
            }
        }
    }
}
