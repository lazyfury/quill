//! The [`Theme`] type: a mode plus the token palette derived from it.

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

/// A resolved design system: [`Mode`] + [`Palette`] + [`Density`] + scale
/// accessors.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Theme {
    pub mode: Mode,
    pub palette: Palette,
    /// Layout density (spacing / control metrics). Colors and type are
    /// unaffected, so a compact theme is a token swap.
    pub density: Density,
}

impl Default for Theme {
    fn default() -> Self {
        Self::light()
    }
}

impl Theme {
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

    pub const fn is_dark(&self) -> bool {
        self.mode.is_dark()
    }

    /// Base page background.
    pub const fn background(&self) -> Color {
        self.palette.background
    }

    /// Primary text color.
    pub const fn foreground(&self) -> Color {
        self.palette.foreground
    }

    /// Secondary text color.
    pub const fn muted(&self) -> Color {
        self.palette.muted
    }

    /// Tertiary / disabled text color.
    pub const fn subtle(&self) -> Color {
        self.palette.subtle
    }

    /// Structural border color.
    pub const fn border(&self) -> Color {
        self.palette.border
    }

    /// Resolves a [`SurfaceLevel`] to its fill color.
    pub const fn surface(&self, level: SurfaceLevel) -> Color {
        match level {
            SurfaceLevel::Base => self.palette.background,
            SurfaceLevel::Surface => self.palette.surface,
            SurfaceLevel::Raised => self.palette.surface_raised,
            SurfaceLevel::Floating => self.palette.surface_raised,
        }
    }

    /// Font size for a text role.
    pub const fn font_size(&self, size: TextSize) -> f32 {
        size.px()
    }

    /// Spacing step in pixels, scaled by the theme's [`Density`].
    pub fn spacing(&self, space: Space) -> f32 {
        space.px() * self.density.space_scale
    }

    /// Height of a control at `size` (regular or mini).
    pub fn control_height(&self, size: ControlSize) -> f32 {
        match size {
            ControlSize::Regular => self.density.control_height,
            ControlSize::Mini => self.density.control_height_mini,
        }
    }

    /// Horizontal padding inside a control.
    pub fn control_padding_x(&self) -> f32 {
        self.density.control_padding_x
    }

    /// Vertical padding inside a control.
    pub fn control_padding_y(&self) -> f32 {
        self.density.control_padding_y
    }

    /// Default list / menu row height.
    pub fn row_height(&self) -> f32 {
        self.density.row_height
    }

    /// Size a control gets when it does not ask for one explicitly.
    pub fn default_control(&self) -> ControlSize {
        self.density.default_control
    }

    /// Radius step in pixels.
    pub const fn radius(&self, radius: Radius) -> f32 {
        radius.px()
    }

    /// Fast motion duration in seconds.
    pub const fn motion_fast(&self) -> f32 {
        Motion::FAST
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_light() {
        assert_eq!(Theme::default().mode, Mode::Light);
        assert_eq!(Theme::light().background().to_rgba8()[0], 0xFF);
    }

    #[test]
    fn toggle_flips_mode_and_palette() {
        let mut theme = Theme::light();
        theme.toggle();
        assert_eq!(theme.mode, Mode::Dark);
        assert_eq!(theme.background().to_rgba8(), [0x0A, 0x0A, 0x0A, 0xFF]);
        theme.toggle();
        assert_eq!(theme.mode, Mode::Light);
    }

    #[test]
    fn surface_levels_resolve_distinctly() {
        let theme = Theme::dark();
        assert_eq!(theme.surface(SurfaceLevel::Base), theme.palette.background);
        assert_eq!(theme.surface(SurfaceLevel::Surface), theme.palette.surface);
        assert_ne!(
            theme.surface(SurfaceLevel::Surface),
            theme.surface(SurfaceLevel::Raised)
        );
    }
}
