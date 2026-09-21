//! `draw_theme` — design tokens for the quill developer-native UI system.
//!
//! This crate is pure data: colors, spacing, radii, type sizes and motion
//! durations. It depends only on [`draw_core`] (for [`Color`]) and never on
//! `draw_ui` or any backend, so it is usable from every layer.
//!
//! ```rust
//! use draw_theme::{space, Mode, Space, TextSize, Theme};
//!
//! let theme = Theme::dark();
//! assert_eq!(theme.mode, Mode::Dark);
//! assert_eq!(space::MD, 12.0);
//! assert_eq!(theme.spacing(Space::MD), 12.0);
//! assert_eq!(theme.compact().spacing(Space::MD), 9.0);
//! assert_eq!(TextSize::Body.px(), 15.0);
//! ```
//!
//! The visual language is intentionally restrained: monochrome surfaces, thin
//! borders, compact controls and semantic accent colors only. See
//! `docs/design-system.md` for the full spec.

/// Crate name, kept for lightweight smoke checks.
pub const CRATE: &str = "draw_theme";

mod density;
mod palette;
mod scale;
mod theme;
mod tone;

pub use density::{ControlSize, Density};
pub use palette::{Palette, Semantic};
pub use scale::{
    border, control, motion, radius, space, Border, Control, Motion, Radius, Space, TextSize,
};
pub use theme::{Mode, SurfaceLevel, Theme};
pub use tone::{SurfaceTone, Tone};
