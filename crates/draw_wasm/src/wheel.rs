//! DOM wheel -> `InputEvent::Wheel` delta conversion.
//!
//! Pure and host-neutral, so it is unit-tested on native; the DOM listener that
//! feeds it lives in [`crate::runner`]. Sign convention: `InputEvent::Wheel`'s
//! `y > 0` means "scroll down", and `WheelEvent.delta_y` is already positive for
//! a downward scroll, so this does **not** negate (unlike winit's `LineDelta`).

/// One wheel line / notch in logical pixels.
pub const WHEEL_LINE_HEIGHT: f32 = 48.0;

/// `WheelEvent.DOM_DELTA_LINE`: `delta_y` is in lines.
pub const DOM_DELTA_LINE: u32 = 1;
/// `WheelEvent.DOM_DELTA_PAGE`: `delta_y` is in pages.
pub const DOM_DELTA_PAGE: u32 = 2;

/// A page scroll is treated as this many lines.
const PAGE_LINES: f64 = 3.0;

/// DOM `(deltaY, deltaMode)` -> logical pixels for `InputEvent::Wheel.delta.y`.
///
/// The result keeps DOM's sign (down is positive). Pixel deltas are already in
/// CSS / logical pixels, so they pass through unchanged.
pub fn wheel_pixels(delta_y: f64, delta_mode: u32) -> f32 {
    let pixels = match delta_mode {
        DOM_DELTA_LINE => delta_y * WHEEL_LINE_HEIGHT as f64,
        DOM_DELTA_PAGE => delta_y * PAGE_LINES * WHEEL_LINE_HEIGHT as f64,
        // `DOM_DELTA_PIXEL` (0) and anything unknown: already CSS pixels.
        _ => delta_y,
    };
    pixels as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `WheelEvent.DOM_DELTA_PIXEL` (the default mode).
    const DOM_DELTA_PIXEL: u32 = 0;

    #[test]
    fn pixel_deltas_pass_through_with_sign() {
        // DOM already reports "down" as positive, matching the IR.
        assert_eq!(wheel_pixels(40.0, DOM_DELTA_PIXEL), 40.0);
        assert_eq!(wheel_pixels(-40.0, DOM_DELTA_PIXEL), -40.0);
    }

    #[test]
    fn line_deltas_scale_by_the_line_height() {
        assert_eq!(wheel_pixels(1.0, DOM_DELTA_LINE), WHEEL_LINE_HEIGHT);
        assert_eq!(wheel_pixels(-2.0, DOM_DELTA_LINE), -2.0 * WHEEL_LINE_HEIGHT);
    }

    #[test]
    fn a_page_step_is_larger_than_a_line() {
        assert!(wheel_pixels(1.0, DOM_DELTA_PAGE) > wheel_pixels(1.0, DOM_DELTA_LINE));
    }
}
