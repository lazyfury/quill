//! Spacing, radius, type, border, control-metric and motion scales.

/// The 13-step spacing scale (in logical pixels).
///
/// Spacing should come from this set rather than arbitrary values.
pub mod space {
    pub const XXXS: f32 = 2.0;
    pub const XXS: f32 = 4.0;
    pub const XS: f32 = 6.0;
    pub const SM: f32 = 8.0;
    pub const MD: f32 = 12.0;
    pub const LG: f32 = 16.0;
    pub const XL: f32 = 20.0;
    pub const XXL: f32 = 24.0;
    pub const XXXL: f32 = 32.0;
    pub const HUGE: f32 = 40.0;
    pub const GIANT: f32 = 48.0;
    pub const MASSIVE: f32 = 64.0;
    pub const COLOSSAL: f32 = 80.0;

    /// Every step in ascending order.
    pub const STEPS: [f32; 13] = [
        XXXS, XXS, XS, SM, MD, LG, XL, XXL, XXXL, HUGE, GIANT, MASSIVE, COLOSSAL,
    ];
}

/// Radius scale (in logical pixels).
pub mod radius {
    /// No radius (square corners).
    pub const NONE: f32 = 0.0;
    /// Small controls, tags.
    pub const SM: f32 = 4.0;
    /// Inputs and buttons.
    pub const MD: f32 = 6.0;
    /// Cards.
    pub const LG: f32 = 8.0;
    /// Panels and modals.
    pub const PANEL: f32 = 10.0;
    /// Fully rounded (status dots, avatars, pills).
    pub const FULL: f32 = 9999.0;
}

/// Stroke widths for structural borders.
pub mod border {
    /// Default hairline border.
    pub const HAIRLINE: f32 = 1.0;
    /// Emphasized border (focus, active).
    pub const FOCUS: f32 = 1.5;
}

/// Compact control metrics shared by the component library.
pub mod control {
    /// Default control height.
    pub const HEIGHT: f32 = 36.0;
    /// Small control height.
    pub const HEIGHT_SM: f32 = 32.0;
    /// Large control height.
    pub const HEIGHT_LG: f32 = 40.0;
    /// Square icon button.
    pub const ICON_SIZE: f32 = 32.0;
    /// Horizontal padding inside a button or input.
    pub const PADDING_X: f32 = 12.0;
    /// Vertical padding inside a button or input.
    pub const PADDING_Y: f32 = 8.0;
    /// Default icon size.
    pub const ICON: f32 = 16.0;
    /// Default list/menu row height.
    pub const ROW: f32 = 36.0;
    /// Compact list/menu row height.
    pub const ROW_SM: f32 = 32.0;
    /// Tab height.
    pub const TAB: f32 = 34.0;
}

/// Motion durations in milliseconds. Motion communicates state changes only.
pub mod motion {
    /// Fast state change (hover, press).
    pub const FAST_MS: u32 = 100;
    /// Standard state change.
    pub const NORMAL_MS: u32 = 150;
    /// Slowest allowed state change.
    pub const SLOW_MS: u32 = 200;

    pub const FAST: f32 = FAST_MS as f32 / 1000.0;
    pub const NORMAL: f32 = NORMAL_MS as f32 / 1000.0;
    pub const SLOW: f32 = SLOW_MS as f32 / 1000.0;
}

/// Restrained type scale (in logical pixels).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TextSize {
    /// Hero text, 48–64px.
    Display,
    /// Page title, 28–40px.
    Title,
    /// Section heading, 20–24px.
    Heading,
    /// Subsection heading, 16–18px.
    Subheading,
    /// Body copy, 14–16px.
    Body,
    /// Secondary text, 12–14px.
    Small,
    /// Metadata / labels, 11–12px.
    Caption,
}

impl TextSize {
    /// Font size in logical pixels.
    pub const fn px(self) -> f32 {
        match self {
            Self::Display => 56.0,
            Self::Title => 34.0,
            Self::Heading => 22.0,
            Self::Subheading => 17.0,
            Self::Body => 15.0,
            Self::Small => 13.0,
            Self::Caption => 11.5,
        }
    }

    /// Recommended line height for this size.
    pub const fn line_height(self) -> f32 {
        let ratio = match self {
            Self::Display => 1.10,
            Self::Title => 1.20,
            Self::Heading => 1.30,
            Self::Subheading => 1.40,
            Self::Body => 1.50,
            Self::Small => 1.45,
            Self::Caption => 1.40,
        };
        self.px() * ratio
    }
}

/// Named spacing steps mirroring [`space`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Space {
    None,
    XXXS,
    XXS,
    XS,
    SM,
    MD,
    LG,
    XL,
    XXL,
    XXXL,
    Huge,
    Giant,
    Massive,
    Colossal,
}

impl Space {
    pub const fn px(self) -> f32 {
        match self {
            Self::None => 0.0,
            Self::XXXS => space::XXXS,
            Self::XXS => space::XXS,
            Self::XS => space::XS,
            Self::SM => space::SM,
            Self::MD => space::MD,
            Self::LG => space::LG,
            Self::XL => space::XL,
            Self::XXL => space::XXL,
            Self::XXXL => space::XXXL,
            Self::Huge => space::HUGE,
            Self::Giant => space::GIANT,
            Self::Massive => space::MASSIVE,
            Self::Colossal => space::COLOSSAL,
        }
    }
}

/// Named radius steps mirroring [`radius`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Radius {
    None,
    Sm,
    Md,
    Lg,
    Panel,
    Full,
}

impl Radius {
    pub const fn px(self) -> f32 {
        match self {
            Self::None => radius::NONE,
            Self::Sm => radius::SM,
            Self::Md => radius::MD,
            Self::Lg => radius::LG,
            Self::Panel => radius::PANEL,
            Self::Full => radius::FULL,
        }
    }
}

/// Alias used by [`Control`] for readability.
pub type Border = f32;

/// Motion durations exposed as a type (see [`motion`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Motion;

impl Motion {
    pub const FAST: f32 = motion::FAST;
    pub const NORMAL: f32 = motion::NORMAL;
    pub const SLOW: f32 = motion::SLOW;
}

/// Control metrics exposed as a type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Control;

impl Control {
    pub const HEIGHT: f32 = control::HEIGHT;
    pub const HEIGHT_SM: f32 = control::HEIGHT_SM;
    pub const HEIGHT_LG: f32 = control::HEIGHT_LG;
    pub const ICON: f32 = control::ICON;
    pub const ROW: f32 = control::ROW;
    pub const ROW_SM: f32 = control::ROW_SM;
    pub const TAB: f32 = control::TAB;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn space_scale_is_monotonic() {
        let steps = space::STEPS;
        assert!(steps.windows(2).all(|w| w[0] < w[1]));
        assert_eq!(space::STEPS[0], 2.0);
        assert_eq!(space::STEPS[space::STEPS.len() - 1], 80.0);
    }

    #[test]
    fn text_scale_orders_and_sizes() {
        assert!(TextSize::Display.px() > TextSize::Title.px());
        assert!(TextSize::Title.px() > TextSize::Heading.px());
        assert!(TextSize::Heading.px() > TextSize::Body.px());
        assert!(TextSize::Body.px() > TextSize::Caption.px());
        assert_eq!(TextSize::Body.px(), 15.0);
    }

    #[test]
    fn named_scales_match_modules() {
        assert_eq!(Space::LG.px(), 16.0);
        assert_eq!(Radius::Md.px(), 6.0);
        assert_eq!(Radius::Full.px(), 9999.0);
        assert_eq!(Control::HEIGHT, 36.0);
        assert_eq!(Motion::FAST, 0.1);
    }
}
