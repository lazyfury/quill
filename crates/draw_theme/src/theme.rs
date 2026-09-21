//! The [`Theme`] trait and the built-in [`DefaultTheme`] implementation.
//!
//! A theme is any type that can resolve the design tokens components ask for
//! (colors, spacing, control metrics, type sizes, motion). The crate ships
//! [`DefaultTheme`] as the reference implementation; an application can define
//! its own type and [`impl Theme`](Theme) to override any token — including the
//! palette, the surface mapping and the font scale — without forking the
//! component library.

use std::sync::LazyLock;

use draw_core::Color;

use crate::density::{ControlSize, Density};
use crate::palette::Palette;
use crate::scale::{Motion, Radius, Space, TextSize};

/// Light or dark appearance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Mode {
    #[default]
    Light,
    Dark,
}

impl Mode {
    pub const fn is_dark(self) -> bool {
        matches!(self, Self::Dark)
    }

    pub const fn is_light(self) -> bool {
        matches!(self, Self::Light)
    }
}

/// The four structural surface levels. Every background difference should map
/// to one of these and have a structural purpose.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SurfaceLevel {
    /// Page / base background.
    Base,
    /// Primary surface (panels, sidebars).
    Surface,
    /// Raised surface (cards, toolbars).
    Raised,
    /// Floating surface (menus, modals, tooltips).
    Floating,
}

/// Design tokens a component reads to paint itself.
///
/// Implement this to theme the whole component library with an application's own
/// values. Only [`palette`](Theme::palette) and [`mode`](Theme::mode) are
/// required; every other token has a default derived from those (and from
/// [`density`](Theme::density)), so an override only needs to name the tokens it
/// actually changes.
///
/// ```
/// use draw_core::Color;
/// use draw_theme::{DefaultTheme, Mode, Palette, SurfaceLevel, Theme};
///
/// struct BrandTheme(DefaultTheme);
///
/// impl Theme for BrandTheme {
///     fn palette(&self) -> &Palette { self.0.palette() }
///     fn mode(&self) -> Mode { self.0.mode() }
///
///     // A floating surface that differs from the raised one.
///     fn surface(&self, level: SurfaceLevel) -> Color {
///         if level == SurfaceLevel::Floating {
///             Color::WHITE
///         } else {
///             self.0.surface(level)
///         }
///     }
/// }
/// ```
pub trait Theme {
    /// The full color system for this theme.
    fn palette(&self) -> &Palette;

    /// Light or dark appearance.
    fn mode(&self) -> Mode;

    /// Layout density (spacing / control metrics). Colors and type are
    /// unaffected, so a compact theme is a token swap.
    fn density(&self) -> Density {
        Density::COMFORTABLE
    }

    fn is_dark(&self) -> bool {
        self.mode().is_dark()
    }

    fn is_light(&self) -> bool {
        self.mode().is_light()
    }

    /// Base page background.
    fn background(&self) -> Color {
        self.palette().background
    }

    /// Primary text color.
    fn foreground(&self) -> Color {
        self.palette().foreground
    }

    /// Secondary text color.
    fn muted(&self) -> Color {
        self.palette().muted
    }

    /// Tertiary / disabled text color.
    fn subtle(&self) -> Color {
        self.palette().subtle
    }

    /// Structural border color.
    fn border(&self) -> Color {
        self.palette().border
    }

    /// Resolves a [`SurfaceLevel`] to its fill color.
    fn surface(&self, level: SurfaceLevel) -> Color {
        let palette = self.palette();
        match level {
            SurfaceLevel::Base => palette.background,
            SurfaceLevel::Surface => palette.surface,
            SurfaceLevel::Raised => palette.surface_raised,
            SurfaceLevel::Floating => palette.surface_raised,
        }
    }

    /// Font size for a text role.
    fn font_size(&self, size: TextSize) -> f32 {
        size.px()
    }

    /// Spacing step in pixels, scaled by the theme's [`Density`].
    fn spacing(&self, space: Space) -> f32 {
        space.px() * self.density().space_scale
    }

    /// Height of a control at `size` (regular or mini).
    fn control_height(&self, size: ControlSize) -> f32 {
        match size {
            ControlSize::Regular => self.density().control_height,
            ControlSize::Mini => self.density().control_height_mini,
        }
    }

    /// Horizontal padding inside a control.
    fn control_padding_x(&self) -> f32 {
        self.density().control_padding_x
    }

    /// Vertical padding inside a control.
    fn control_padding_y(&self) -> f32 {
        self.density().control_padding_y
    }

    /// Default list / menu row height.
    fn row_height(&self) -> f32 {
        self.density().row_height
    }

    /// Size a control gets when it does not ask for one explicitly.
    fn default_control(&self) -> ControlSize {
        self.density().default_control
    }

    /// Radius step in pixels.
    fn radius(&self, radius: Radius) -> f32 {
        radius.px()
    }

    /// Fast motion duration in seconds.
    fn motion_fast(&self) -> f32 {
        Motion::FAST
    }
}

/// The reference [`Theme`]: a [`Mode`] plus the [`Palette`] and [`Density`]
/// derived from it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DefaultTheme {
    pub mode: Mode,
    pub palette: Palette,
    /// Layout density (spacing / control metrics). Colors and type are
    /// unaffected, so a compact theme is a token swap.
    pub density: Density,
}

impl Default for DefaultTheme {
    fn default() -> Self {
        Self::light()
    }
}

impl DefaultTheme {
    pub fn light() -> Self {
        Self {
            mode: Mode::Light,
            palette: Palette::light(),
            density: Density::default(),
        }
    }

    pub fn dark() -> Self {
        Self {
            mode: Mode::Dark,
            palette: Palette::dark(),
            density: Density::default(),
        }
    }

    /// Returns a copy of this theme with a different layout density.
    pub fn with_density(mut self, density: Density) -> Self {
        self.density = density;
        self
    }

    /// This theme with [`Density::COMPACT`] (tighter spacing, mini controls).
    pub fn compact(self) -> Self {
        self.with_density(Density::COMPACT)
    }

    /// Builds the theme for an explicit mode.
    pub fn with_mode(mode: Mode) -> Self {
        match mode {
            Mode::Light => Self::light(),
            Mode::Dark => Self::dark(),
        }
    }

    /// Flips light/dark in place.
    pub fn toggle(&mut self) {
        *self = match self.mode {
            Mode::Light => Self::dark(),
            Mode::Dark => Self::light(),
        };
    }
}

impl Theme for DefaultTheme {
    fn palette(&self) -> &Palette {
        &self.palette
    }

    fn mode(&self) -> Mode {
        self.mode
    }

    fn density(&self) -> Density {
        self.density
    }
}

static DEFAULT_LIGHT: LazyLock<DefaultTheme> = LazyLock::new(DefaultTheme::light);
static DEFAULT_DARK: LazyLock<DefaultTheme> = LazyLock::new(DefaultTheme::dark);
static COMPACT_LIGHT: LazyLock<DefaultTheme> = LazyLock::new(|| DefaultTheme::light().compact());
static COMPACT_DARK: LazyLock<DefaultTheme> = LazyLock::new(|| DefaultTheme::dark().compact());

/// The built-in theme for `mode` at the default density, as a `'static` trait
/// object ready to hand to components.
pub fn default_theme(mode: Mode) -> &'static dyn Theme {
    match mode {
        Mode::Light => &*DEFAULT_LIGHT,
        Mode::Dark => &*DEFAULT_DARK,
    }
}

/// The built-in [`Density::COMPACT`] theme for `mode`, as a `'static` trait
/// object.
pub fn compact_theme(mode: Mode) -> &'static dyn Theme {
    match mode {
        Mode::Light => &*COMPACT_LIGHT,
        Mode::Dark => &*COMPACT_DARK,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_light() {
        assert_eq!(DefaultTheme::default().mode, Mode::Light);
        assert_eq!(DefaultTheme::light().background().to_rgba8()[0], 0xFF);
    }

    #[test]
    fn toggle_flips_mode_and_palette() {
        let mut theme = DefaultTheme::light();
        theme.toggle();
        assert_eq!(theme.mode, Mode::Dark);
        assert_eq!(theme.background().to_rgba8(), [0x0A, 0x0A, 0x0A, 0xFF]);
        theme.toggle();
        assert_eq!(theme.mode, Mode::Light);
    }

    #[test]
    fn surface_levels_resolve_distinctly() {
        let theme = DefaultTheme::dark();
        assert_eq!(theme.surface(SurfaceLevel::Base), theme.palette.background);
        assert_eq!(theme.surface(SurfaceLevel::Surface), theme.palette.surface);
        assert_ne!(
            theme.surface(SurfaceLevel::Surface),
            theme.surface(SurfaceLevel::Raised)
        );
    }

    #[test]
    fn a_custom_theme_can_override_one_token() {
        struct FloatWhite(DefaultTheme);
        impl Theme for FloatWhite {
            fn palette(&self) -> &Palette {
                self.0.palette()
            }
            fn mode(&self) -> Mode {
                self.0.mode()
            }
            fn surface(&self, level: SurfaceLevel) -> Color {
                if level == SurfaceLevel::Floating {
                    Color::WHITE
                } else {
                    self.0.surface(level)
                }
            }
        }

        let theme = FloatWhite(DefaultTheme::dark());
        assert_eq!(theme.surface(SurfaceLevel::Floating), Color::WHITE);
        assert_eq!(
            theme.surface(SurfaceLevel::Raised),
            DefaultTheme::dark().surface(SurfaceLevel::Raised)
        );
    }

    #[test]
    fn static_theme_helpers_share_mode_and_density() {
        assert_eq!(default_theme(Mode::Dark).mode(), Mode::Dark);
        assert_eq!(
            compact_theme(Mode::Light).default_control(),
            ControlSize::Mini
        );
        assert_eq!(
            default_theme(Mode::Light).default_control(),
            ControlSize::Regular
        );
    }
}
