use draw_core::{Edges, Rect, Size, Vec2};

/// How a control reacts to pointer events during hit testing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum MouseFilter {
    /// Consume the event and stop the search (topmost control wins).
    #[default]
    Stop,
    /// Report the hit but let a control underneath also be considered.
    Pass,
    /// Never hit-testable (transparent to the pointer).
    Ignore,
}

/// Layout data for a control.
///
/// The rectangle is resolved from the parent rectangle using the Godot-style
/// anchor/offset model:
///
/// ```text
/// left  = parent.left + parent.width  * anchor.left  + offset.left
/// right = parent.left + parent.width  * anchor.right + offset.right
/// ```
///
/// `anchor` components are normally `0.0` or `1.0`; `offset` is in pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ControlData {
    pub anchors: Edges,
    pub offsets: Edges,
    pub min_size: Size,
    /// Absolute rectangle in logical viewport coordinates, valid after layout.
    pub rect: Rect,
    pub mouse_filter: MouseFilter,
}

impl Default for ControlData {
    fn default() -> Self {
        Self {
            anchors: Edges::ZERO,
            offsets: Edges::ZERO,
            min_size: Size::ZERO,
            rect: Rect::ZERO,
            mouse_filter: MouseFilter::Stop,
        }
    }
}

impl ControlData {
    pub fn new() -> Self {
        Self::default()
    }

    /// Fills the parent (anchors span `0..1`, zero offsets).
    pub fn fill_parent() -> Self {
        Self {
            anchors: Edges::new(0.0, 0.0, 1.0, 1.0),
            ..Self::default()
        }
    }

    /// Resolves this control's rectangle against `parent`.
    ///
    /// The size is clamped from the top-left so it never shrinks below
    /// `min_size`.
    pub fn resolve_rect(&self, parent: Rect) -> Rect {
        let left = parent.left() + parent.size.width * self.anchors.left + self.offsets.left;
        let top = parent.top() + parent.size.height * self.anchors.top + self.offsets.top;
        let right = parent.left() + parent.size.width * self.anchors.right + self.offsets.right;
        let bottom = parent.top() + parent.size.height * self.anchors.bottom + self.offsets.bottom;

        let width = (right - left).max(self.min_size.width);
        let height = (bottom - top).max(self.min_size.height);
        Rect::from_min_size(Vec2::new(left, top), Size::new(width, height))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_parent_resolves_to_parent() {
        let parent = Rect::from_min_size(Vec2::new(10.0, 20.0), Size::new(200.0, 100.0));
        let control = ControlData::fill_parent();
        assert_eq!(control.resolve_rect(parent), parent);
    }

    #[test]
    fn top_left_anchor_with_offsets() {
        let parent = Rect::from_min_size(Vec2::ZERO, Size::new(200.0, 100.0));
        let control = ControlData {
            anchors: Edges::new(0.0, 0.0, 0.0, 0.0),
            offsets: Edges::new(10.0, 20.0, 60.0, 50.0),
            ..ControlData::default()
        };
        assert_eq!(
            control.resolve_rect(parent),
            Rect::from_min_size(Vec2::new(10.0, 20.0), Size::new(50.0, 30.0))
        );
    }

    #[test]
    fn min_size_is_enforced() {
        let parent = Rect::from_min_size(Vec2::ZERO, Size::new(10.0, 10.0));
        let control = ControlData {
            anchors: Edges::new(0.0, 0.0, 0.0, 0.0),
            offsets: Edges::ZERO,
            min_size: Size::new(80.0, 40.0),
            ..ControlData::default()
        };
        assert_eq!(
            control.resolve_rect(parent),
            Rect::from_min_size(Vec2::ZERO, Size::new(80.0, 40.0))
        );
    }

    #[test]
    fn right_anchor_grows_with_parent() {
        let parent = Rect::from_min_size(Vec2::ZERO, Size::new(200.0, 100.0));
        let control = ControlData {
            anchors: Edges::new(0.0, 0.0, 1.0, 0.0),
            offsets: Edges::new(0.0, 0.0, -20.0, 30.0),
            ..ControlData::default()
        };
        let rect = control.resolve_rect(parent);
        assert_eq!(rect.left(), 0.0);
        assert_eq!(rect.right(), 180.0);
        assert_eq!(rect.size.height, 30.0);
    }
}
