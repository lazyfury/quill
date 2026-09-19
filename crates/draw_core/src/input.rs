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
}
