//! Monochrome surface palette plus a small set of semantic accents.

use draw_core::Color;

/// Semantic accent roles. Accents are used only for success/warning/error/
/// information/selection/focus, never as decoration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Semantic {
    /// Primary interactive / selected / focus color.
    Accent,
    Success,
    Warning,
    Error,
    Info,
}

/// The full color system for one [`Mode`](crate::Mode).
///
/// Values come straight from the design spec. Accent colors are the only
/// non-monochrome entries and each carries a fixed meaning.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Palette {
    pub background: Color,
    pub foreground: Color,
    pub surface: Color,
    pub surface_raised: Color,
    pub surface_hover: Color,
    pub muted: Color,
    pub subtle: Color,
    pub border: Color,
    pub border_subtle: Color,
    pub code_surface: Color,

    pub accent: Color,
    pub success: Color,
    pub warning: Color,
    pub error: Color,
    pub info: Color,

    /// Text/icon color placed on top of `accent`.
    pub on_accent: Color,
    /// Translucent ring drawn around focused controls.
    pub focus_ring: Color,
    /// Translucent background behind selected rows/items.
    pub selection: Color,
}

impl Palette {
    /// Monochrome light palette (`#FFFFFF` base).
    pub fn light() -> Self {
        let accent = rgba(0x25, 0x63, 0xEB, 0xFF);
        Self {
            background: rgba(0xFF, 0xFF, 0xFF, 0xFF),
            foreground: rgba(0x11, 0x11, 0x11, 0xFF),
            surface: rgba(0xFA, 0xFA, 0xFA, 0xFF),
            surface_raised: rgba(0xF5, 0xF5, 0xF5, 0xFF),
            surface_hover: rgba(0xF2, 0xF2, 0xF2, 0xFF),
            muted: rgba(0x73, 0x73, 0x73, 0xFF),
            subtle: rgba(0xA3, 0xA3, 0xA3, 0xFF),
            border: rgba(0xE5, 0xE5, 0xE5, 0xFF),
            border_subtle: rgba(0xEE, 0xEE, 0xEE, 0xFF),
            code_surface: rgba(0xF7, 0xF7, 0xF7, 0xFF),
            accent,
            success: rgba(0x16, 0xA3, 0x4A, 0xFF),
            warning: rgba(0xD9, 0x77, 0x06, 0xFF),
            error: rgba(0xDC, 0x26, 0x26, 0xFF),
            info: rgba(0x25, 0x63, 0xEB, 0xFF),
            on_accent: rgba(0xFF, 0xFF, 0xFF, 0xFF),
            focus_ring: accent.with_alpha(0.45),
            selection: accent.with_alpha(0.12),
        }
    }

    /// Monochrome dark palette (`#0A0A0A` base).
    pub fn dark() -> Self {
        let accent = rgba(0x3B, 0x82, 0xF6, 0xFF);
        Self {
            background: rgba(0x0A, 0x0A, 0x0A, 0xFF),
            foreground: rgba(0xF5, 0xF5, 0xF5, 0xFF),
            surface: rgba(0x11, 0x11, 0x11, 0xFF),
            surface_raised: rgba(0x17, 0x17, 0x17, 0xFF),
            surface_hover: rgba(0x1C, 0x1C, 0x1C, 0xFF),
            muted: rgba(0xA3, 0xA3, 0xA3, 0xFF),
            subtle: rgba(0x73, 0x73, 0x73, 0xFF),
            border: rgba(0x26, 0x26, 0x26, 0xFF),
            border_subtle: rgba(0x1F, 0x1F, 0x1F, 0xFF),
            code_surface: rgba(0x11, 0x11, 0x11, 0xFF),
            accent,
            success: rgba(0x22, 0xC5, 0x5E, 0xFF),
            warning: rgba(0xF5, 0x9E, 0x0B, 0xFF),
            error: rgba(0xEF, 0x44, 0x44, 0xFF),
            info: rgba(0x3B, 0x82, 0xF6, 0xFF),
            on_accent: rgba(0x0A, 0x0A, 0x0A, 0xFF),
            focus_ring: accent.with_alpha(0.55),
            selection: accent.with_alpha(0.18),
        }
    }

    /// Resolves a semantic role to its color.
    pub fn semantic(&self, role: Semantic) -> Color {
        match role {
            Semantic::Accent => self.accent,
            Semantic::Success => self.success,
            Semantic::Warning => self.warning,
            Semantic::Error => self.error,
            Semantic::Info => self.info,
        }
    }
}

fn rgba(r: u8, g: u8, b: u8, a: u8) -> Color {
    Color::from_rgba8(r, g, b, a)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn light_palette_matches_spec() {
        let p = Palette::light();
        assert_eq!(p.background.to_rgba8(), [0xFF, 0xFF, 0xFF, 0xFF]);
        assert_eq!(p.foreground.to_rgba8(), [0x11, 0x11, 0x11, 0xFF]);
        assert_eq!(p.border.to_rgba8(), [0xE5, 0xE5, 0xE5, 0xFF]);
        assert_eq!(p.code_surface.to_rgba8(), [0xF7, 0xF7, 0xF7, 0xFF]);
    }

    #[test]
    fn dark_palette_matches_spec() {
        let p = Palette::dark();
        assert_eq!(p.background.to_rgba8(), [0x0A, 0x0A, 0x0A, 0xFF]);
        assert_eq!(p.foreground.to_rgba8(), [0xF5, 0xF5, 0xF5, 0xFF]);
        assert_eq!(p.border.to_rgba8(), [0x26, 0x26, 0x26, 0xFF]);
        assert_eq!(p.surface_raised.to_rgba8(), [0x17, 0x17, 0x17, 0xFF]);
    }

    #[test]
    fn semantic_roles_resolve() {
        let p = Palette::light();
        assert_eq!(p.semantic(Semantic::Error), p.error);
        assert_eq!(p.semantic(Semantic::Accent), p.accent);
    }
}
