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
            | Self::PointerMove { position } => Some(*position),
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
        assert!(EventResult::Handled.is_handled());
        assert!(!EventResult::Ignored.is_handled());
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
