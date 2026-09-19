//! Overlay placement: given an anchor rectangle and the overlay's size, resolve
//! where it goes, flipping to the opposite side when it would leave the viewport
//! and clamping inside a margin.
//!
//! Pure geometry (no `Ui`/theme), so it is unit-testable on its own.

use draw_core::{Rect, Size};

/// Where an overlay is placed relative to its anchor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    /// Above the anchor (flips below when it would clip the top edge).
    Above,
    /// Below the anchor (flips above when it would clip the bottom edge).
    Below,
    /// Left of the anchor (flips right when it would clip the left edge).
    Left,
    /// Right of the anchor (flips left when it would clip the right edge).
    Right,
    /// Centered on the anchor.
    Center,
    /// Horizontally centered in the viewport, near the top edge.
    TopCenter,
    /// Horizontally centered in the viewport, near the bottom edge.
    BottomCenter,
}

/// Resolves the overlay rectangle.
///
/// `anchor` is in viewport coordinates. `offset` is the gap between the anchor
/// and the overlay; `margin` keeps the overlay off the viewport edges.
pub fn place(
    anchor: Rect,
    content: Size,
    viewport: Rect,
    placement: Placement,
    offset: f32,
    margin: f32,
) -> Rect {
    let (w, h) = (content.width, content.height);
    let min_x = viewport.left() + margin;
    let min_y = viewport.top() + margin;
    let max_x = viewport.right() - margin - w;
    let max_y = viewport.bottom() - margin - h;

    let (preferred_x, preferred_y) = match placement {
        Placement::Above => (anchor.center().x - w * 0.5, anchor.top() - offset - h),
        Placement::Below => (anchor.center().x - w * 0.5, anchor.bottom() + offset),
        Placement::Left => (anchor.left() - offset - w, anchor.center().y - h * 0.5),
        Placement::Right => (anchor.right() + offset, anchor.center().y - h * 0.5),
        Placement::Center => (anchor.center().x - w * 0.5, anchor.center().y - h * 0.5),
        Placement::TopCenter => (viewport.center().x - w * 0.5, min_y),
        Placement::BottomCenter => (viewport.center().x - w * 0.5, max_y),
    };

    let (mut x, mut y) = (preferred_x, preferred_y);
    match placement {
        Placement::Above if y < min_y => y = anchor.bottom() + offset,
        Placement::Below if y > max_y => y = anchor.top() - offset - h,
        Placement::Left if x < min_x => x = anchor.right() + offset,
        Placement::Right if x > max_x => x = anchor.left() - offset - w,
        _ => {}
    }

    // Clamp into the viewport. When the content is larger than the viewport the
    // range is inverted, so pin the content to the minimum edge instead.
    x = if min_x <= max_x {
        x.clamp(min_x, max_x)
    } else {
        min_x
    };
    y = if min_y <= max_y {
        y.clamp(min_y, max_y)
    } else {
        min_y
    };

    Rect::from_min_size(draw_core::Vec2::new(x, y), content)
}

#[cfg(test)]
mod tests {
    use super::*;
    use draw_core::Vec2;

    fn viewport() -> Rect {
        Rect::from_min_size(Vec2::ZERO, Size::new(400.0, 300.0))
    }

    fn anchor() -> Rect {
        Rect::from_min_size(Vec2::new(180.0, 140.0), Size::new(40.0, 20.0))
    }

    #[test]
    fn below_is_below_the_anchor() {
        let out = place(
            anchor(),
            Size::new(100.0, 40.0),
            viewport(),
            Placement::Below,
            8.0,
            8.0,
        );
        assert!((out.left() - 150.0).abs() < 1e-3);
        assert!((out.top() - 168.0).abs() < 1e-3);
    }

    #[test]
    fn above_is_above_the_anchor() {
        let out = place(
            anchor(),
            Size::new(100.0, 40.0),
            viewport(),
            Placement::Above,
            8.0,
            8.0,
        );
        assert!((out.top() - 92.0).abs() < 1e-3);
    }

    #[test]
    fn below_flips_above_near_the_bottom_edge() {
        let anchor = Rect::from_min_size(Vec2::new(180.0, 280.0), Size::new(40.0, 16.0));
        let out = place(
            anchor,
            Size::new(100.0, 40.0),
            viewport(),
            Placement::Below,
            8.0,
            8.0,
        );
        assert!(out.bottom() <= anchor.top() + 1e-3, "did not flip: {out:?}");
    }

    #[test]
    fn right_flips_left_near_the_right_edge() {
        let anchor = Rect::from_min_size(Vec2::new(380.0, 140.0), Size::new(16.0, 20.0));
        let out = place(
            anchor,
            Size::new(100.0, 40.0),
            viewport(),
            Placement::Right,
            8.0,
            8.0,
        );
        assert!(out.right() <= anchor.left() + 1e-3, "did not flip: {out:?}");
    }

    #[test]
    fn clamps_inside_the_margin() {
        let anchor = Rect::from_min_size(Vec2::new(390.0, 10.0), Size::new(8.0, 8.0));
        let out = place(
            anchor,
            Size::new(100.0, 40.0),
            viewport(),
            Placement::Center,
            0.0,
            8.0,
        );
        assert!(out.left() >= 8.0 - 1e-3);
        assert!(out.right() <= 400.0 - 8.0 + 1e-3);
        assert!(out.top() >= 8.0 - 1e-3);
        assert!(out.bottom() <= 300.0 - 8.0 + 1e-3);
    }

    #[test]
    fn bottom_center_pins_to_the_bottom_margin() {
        let out = place(
            viewport(),
            Size::new(120.0, 30.0),
            viewport(),
            Placement::BottomCenter,
            0.0,
            10.0,
        );
        assert!((out.center().x - 200.0).abs() < 1e-3);
        assert!((out.bottom() - 290.0).abs() < 1e-3);
    }

    #[test]
    fn oversized_content_pins_to_the_top_left_margin() {
        let out = place(
            anchor(),
            Size::new(4000.0, 4000.0),
            viewport(),
            Placement::Below,
            8.0,
            8.0,
        );
        assert!((out.left() - 8.0).abs() < 1e-3);
        assert!((out.top() - 8.0).abs() < 1e-3);
    }
}
