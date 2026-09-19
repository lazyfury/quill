//! Backend-neutral pointer cursor kinds.

/// The cursor a control asks the host to show while the pointer is over it.
///
/// Hosts map this onto their own cursor API (winit `CursorIcon`, the CSS
/// `cursor` property, ...). Backends never appear here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Cursor {
    /// The platform default arrow.
    #[default]
    Default,
    /// A clickable control (hand / pointing cursor).
    Pointer,
    /// Text selection.
    Text,
    /// Horizontal split resize (`↔`).
    ColResize,
    /// Vertical split resize (`↕`).
    RowResize,
    /// Something draggable.
    Grab,
    /// Something currently being dragged.
    Grabbing,
}
