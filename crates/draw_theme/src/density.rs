//! Layout density: how much room controls and spacing take.
//!
//! Density is a token like the palette: the component library reads its spacing
//! and control metrics from [`Theme`](crate::Theme), so switching to a compact
//! theme is a token swap, not a second code path. Colors and type sizes are
//! unaffected.

/// Which control metric set a component uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ControlSize {
    /// The default height/padding.
    #[default]
    Regular,
    /// A compact "mini" control.
    Mini,
}

/// Spacing and control metrics for a [`Theme`](crate::Theme).
///
/// `space_scale` multiplies every [`Space`](crate::Space) step, so a theme can
/// tighten all padding at once while the named scale stays the source of truth.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Density {
    /// Multiplier applied to every spacing step (1.0 = the base scale).
    pub space_scale: f32,
    /// Regular control (button / input) height.
    pub control_height: f32,
    /// Mini control height.
    pub control_height_mini: f32,
    /// Horizontal padding inside a control.
    pub control_padding_x: f32,
    /// Vertical padding inside a control.
    pub control_padding_y: f32,
    /// Default list / menu row height.
    pub row_height: f32,
    /// Size a control gets when it does not ask for one explicitly.
    pub default_control: ControlSize,
}

impl Density {
    /// The default density: the documented scale, regular controls.
    pub const COMFORTABLE: Self = Self {
        space_scale: 1.0,
        control_height: 36.0,
        control_height_mini: 32.0,
        control_padding_x: 12.0,
        control_padding_y: 8.0,
        row_height: 36.0,
        default_control: ControlSize::Regular,
    };

    /// A tighter density: 0.75x spacing, shorter controls, mini by default.
    pub const COMPACT: Self = Self {
        space_scale: 0.75,
        control_height: 28.0,
        control_height_mini: 24.0,
        control_padding_x: 8.0,
        control_padding_y: 4.0,
        row_height: 28.0,
        default_control: ControlSize::Mini,
    };
}

impl Default for Density {
    fn default() -> Self {
        Self::COMFORTABLE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_is_tighter_than_comfortable() {
        let comfortable = Density::COMFORTABLE;
        let compact = Density::COMPACT;
        assert!(compact.space_scale < comfortable.space_scale);
        assert!(compact.control_height < comfortable.control_height);
        assert!(compact.control_padding_x < comfortable.control_padding_x);
        assert_eq!(compact.default_control, ControlSize::Mini);
        assert_eq!(comfortable.default_control, ControlSize::Regular);
    }
}
