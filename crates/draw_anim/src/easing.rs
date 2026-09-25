//! Easing curves: map a tween's normalized time (`0.0..=1.0`) to progress.
//!
//! Backend-neutral, pure math. `Back*` curves overshoot outside `0..=1`, which
//! is intentional (they are used for "pop" motion); callers that need a bounded
//! value should clamp the result themselves.

/// An interpolation curve applied to a tween's normalized time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum Easing {
    #[default]
    Linear,
    QuadIn,
    QuadOut,
    QuadInOut,
    CubicIn,
    CubicOut,
    CubicInOut,
    SineIn,
    SineOut,
    SineInOut,
    BackIn,
    BackOut,
    BackInOut,
}

/// Overshoot constant for the `Back*` curves.
const BACK_C1: f32 = 1.70158;
const BACK_C3: f32 = BACK_C1 + 1.0;

impl Easing {
    /// Evaluates the curve at `t`, clamping `t` to `0.0..=1.0`.
    pub fn ease(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Easing::Linear => t,
            Easing::QuadIn => t * t,
            Easing::QuadOut => 1.0 - (1.0 - t) * (1.0 - t),
            Easing::QuadInOut => {
                if t < 0.5 {
                    2.0 * t * t
                } else {
                    1.0 - (-2.0 * t + 2.0).powi(2) / 2.0
                }
            }
            Easing::CubicIn => t * t * t,
            Easing::CubicOut => 1.0 - (1.0 - t).powi(3),
            Easing::CubicInOut => {
                if t < 0.5 {
                    4.0 * t * t * t
                } else {
                    1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
                }
            }
            Easing::SineIn => 1.0 - (t * std::f32::consts::FRAC_PI_2).cos(),
            Easing::SineOut => (t * std::f32::consts::FRAC_PI_2).sin(),
            Easing::SineInOut => -((std::f32::consts::PI * t).cos() - 1.0) / 2.0,
            Easing::BackIn => BACK_C3 * t * t * t - BACK_C1 * t * t,
            Easing::BackOut => 1.0 + BACK_C3 * (t - 1.0).powi(3) + BACK_C1 * (t - 1.0).powi(2),
            Easing::BackInOut => {
                const C2: f32 = BACK_C1 * 1.525;
                if t < 0.5 {
                    let u = 2.0 * t;
                    u * u * ((C2 + 1.0) * u - C2) / 2.0
                } else {
                    let u = 2.0 * t - 2.0;
                    (u * u * ((C2 + 1.0) * u + C2) + 2.0) / 2.0
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [Easing; 13] = [
        Easing::Linear,
        Easing::QuadIn,
        Easing::QuadOut,
        Easing::QuadInOut,
        Easing::CubicIn,
        Easing::CubicOut,
        Easing::CubicInOut,
        Easing::SineIn,
        Easing::SineOut,
        Easing::SineInOut,
        Easing::BackIn,
        Easing::BackOut,
        Easing::BackInOut,
    ];

    #[test]
    fn every_curve_pins_both_endpoints() {
        for easing in ALL {
            assert!(easing.ease(0.0).abs() < 1e-5, "{easing:?} at 0");
            assert!((easing.ease(1.0) - 1.0).abs() < 1e-5, "{easing:?} at 1");
        }
    }

    #[test]
    fn ease_clamps_outside_the_unit_interval() {
        for easing in ALL {
            assert_eq!(easing.ease(-1.0), easing.ease(0.0), "{easing:?} below");
            assert_eq!(easing.ease(2.0), easing.ease(1.0), "{easing:?} above");
        }
    }

    #[test]
    fn linear_is_the_identity() {
        assert_eq!(Easing::Linear.ease(0.25), 0.25);
        assert_eq!(Easing::Linear.ease(0.75), 0.75);
    }

    #[test]
    fn back_out_overshoots_before_settling() {
        // A characteristic of `BackOut`: it passes 1.0 mid-curve.
        assert!(Easing::BackOut.ease(0.8) > 1.0);
    }

    #[test]
    fn in_out_curves_are_centered() {
        for easing in [Easing::QuadInOut, Easing::CubicInOut, Easing::SineInOut] {
            assert!((easing.ease(0.5) - 0.5).abs() < 1e-5, "{easing:?}");
        }
    }
}
