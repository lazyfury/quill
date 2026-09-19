//! The [`Theme`] type: a mode plus the token palette derived from it.

use draw_core::Color;

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

/// A resolved design system: [`Mode`] + [`Palette`] + scale accessors.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Theme {
    pub mode: Mode,
    pub palette: Palette,
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
        }
    }

    pub fn dark() -> Self {
        Self {
            mode: Mode::Dark,
            palette: Palette::dark(),
        }
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

    /// Spacing step in pixels.
    pub const fn spacing(&self, space: Space) -> f32 {
        space.px()
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
