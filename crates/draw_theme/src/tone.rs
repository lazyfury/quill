//! Semantic color tones resolved against a [`Theme`](crate::Theme).

use draw_core::Color;

use crate::{Semantic, SurfaceLevel, Theme};

/// A semantic color role used by text and component chrome.
///
/// Tones keep component constructors free of raw colors while still allowing an
/// explicit color override.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Tone {
    /// Primary foreground text.
    #[default]
    Default,
    /// Secondary / muted text.
    Muted,
    /// Tertiary / disabled text.
    Subtle,
    /// Interactive / selected accent.
    Accent,
    Success,
    Warning,
    Error,
    Info,
    /// Text placed on top of the accent color.
    OnAccent,
    /// Transparent (used for hit areas / layout-only chrome).
    Transparent,
}

impl Tone {
    /// Resolves this tone to a concrete color.
    pub fn color(self, theme: &dyn Theme) -> Color {
        let palette = theme.palette();
        match self {
            Tone::Default => palette.foreground,
            Tone::Muted => palette.muted,
            Tone::Subtle => palette.subtle,
            Tone::Accent => palette.accent,
            Tone::Success => palette.success,
            Tone::Warning => palette.warning,
            Tone::Error => palette.error,
            Tone::Info => palette.info,
            Tone::OnAccent => palette.on_accent,
            Tone::Transparent => Color::TRANSPARENT,
        }
    }

    /// The [`Semantic`] role behind this tone, if any.
    pub fn semantic(self) -> Option<Semantic> {
        match self {
            Tone::Accent => Some(Semantic::Accent),
            Tone::Success => Some(Semantic::Success),
            Tone::Warning => Some(Semantic::Warning),
            Tone::Error => Some(Semantic::Error),
            Tone::Info => Some(Semantic::Info),
            _ => None,
        }
    }
}

/// Surface level used by card / panel components.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SurfaceTone {
    /// Page background.
    Base,
    /// Primary panel surface.
    Surface,
    /// Raised card surface.
    #[default]
    Raised,
    /// Floating menu / modal surface.
    Floating,
}

impl SurfaceTone {
    pub fn color(self, theme: &dyn Theme) -> Color {
        theme.surface(match self {
            SurfaceTone::Base => SurfaceLevel::Base,
            SurfaceTone::Surface => SurfaceLevel::Surface,
            SurfaceTone::Raised => SurfaceLevel::Raised,
            SurfaceTone::Floating => SurfaceLevel::Floating,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DefaultTheme;

    #[test]
    fn tones_resolve_against_palette() {
        let theme = DefaultTheme::dark();
        assert_eq!(Tone::Default.color(&theme), theme.palette.foreground);
        assert_eq!(Tone::Error.color(&theme), theme.palette.error);
        assert!(Tone::Transparent.color(&theme).is_transparent());
    }

    #[test]
    fn surface_tones_match_levels() {
        let theme = DefaultTheme::light();
        assert_eq!(SurfaceTone::Base.color(&theme), theme.palette.background);
        assert_eq!(
            SurfaceTone::Raised.color(&theme),
            theme.palette.surface_raised
        );
    }
}
