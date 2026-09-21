//! `draw_svg` — backend-neutral SVG (vector) rendering for quill.
//!
//! Strategy ("option C"): parse a small SVG subset down to **flattened polylines**
//! and stroke them with the existing [`draw_render`] primitives — [`Line`] for
//! segments and [`FillCircle`] for round joins/caps. No external dependency, no
//! GPU/DOM/browser types, so the same document can be painted by any backend.
//!
//! [`Line`]: draw_render::DrawCommand::Line
//! [`FillCircle`]: draw_render::DrawCommand::FillCircle
//!
//! ```text
//! SVG text -> SvgDocument (flattened polylines) -> PaintContext -> DrawList -> any backend
//! ```
//!
//! # Supported
//!
//! - Elements: `<svg>`, `<g>`, `<path>`, `<line>`, `<polyline>`, `<polygon>`,
//!   `<rect>` (incl. rounded), `<circle>`, `<ellipse>`.
//! - Path data: `M L H V C S Q T A Z`, absolute and relative.
//! - Presentation attributes (inherited through `<g>` / `<svg>`): `stroke`,
//!   `stroke-width`, `stroke-linecap`, `stroke-linejoin`, plus the `viewBox`.
//! - Colors: `none`, `currentColor`, `#rgb` / `#rrggbb` / `#rrggbbaa`, and a few
//!   named colors (`black` / `white` / `red` / `green` / `blue` / `yellow`).
//!
//! # Not yet
//!
//! **Stroke only**: `fill` is parsed but not rendered, so fill-only artwork does
//! not appear. `<style>` CSS, gradients, patterns, masks, filters, `<use>`,
//! `transform`, and text are ignored. `stroke="currentColor"` resolves to the
//! color passed to [`SvgDocument::draw`], which is exactly what an icon pack
//! such as [Lucide](https://lucide.dev) needs.
//!
//! # Example
//!
//! ```
//! use draw_render::PaintContext;
//! use draw_core::{Color, Rect, Size, Vec2};
//!
//! let svg = draw_svg::SvgDocument::parse(
//!     r#"<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
//!          <path d="M4 12h16" />
//!        </svg>"#,
//! )
//! .unwrap();
//!
//! let mut ctx = PaintContext::new();
//! let target = Rect::from_min_size(Vec2::ZERO, Size::splat(24.0));
//! svg.draw(&mut ctx, target, Color::BLACK);
//! assert!(!ctx.into_draw_list().is_empty());
//! ```

use std::fmt;

use draw_core::{Color, Rect, Size, Vec2};

mod draw;
mod pack;
mod parse;
mod path;
mod shapes;

pub use pack::IconPack;

/// The SVG `viewBox`: a user-space origin and size that maps onto the target
/// rectangle at draw time.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewBox {
    pub min: Vec2,
    pub size: Size,
}

impl ViewBox {
    /// The common icon grid (`0 0 24 24`), used when `<svg>` has no `viewBox`.
    pub const ICON_24: Self = Self {
        min: Vec2::ZERO,
        size: Size::splat(24.0),
    };

    pub const fn new(min_x: f32, min_y: f32, width: f32, height: f32) -> Self {
        Self {
            min: Vec2::new(min_x, min_y),
            size: Size::new(width, height),
        }
    }

    /// The viewBox as an axis-aligned rectangle in user space.
    pub fn rect(self) -> Rect {
        Rect::from_min_size(self.min, self.size)
    }
}

impl Default for ViewBox {
    fn default() -> Self {
        Self::ICON_24
    }
}

/// How a stroked subpath ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineCap {
    /// Flat end exactly at the vertex (the IR's native `Line` cap).
    #[default]
    Butt,
    /// A half-disc of the stroke radius past the vertex.
    Round,
    /// A square of the stroke radius past the vertex.
    Square,
}

/// How two stroked segments meet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineJoin {
    #[default]
    Miter,
    /// A disc of the stroke radius fills the joint.
    Round,
    Bevel,
}

/// Where a stroke's color comes from.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum ColorSource {
    /// `stroke="none"` (or `transparent`): the shape is not stroked.
    #[default]
    None,
    /// `stroke="currentColor"`: resolved from the paint passed to
    /// [`SvgDocument::draw`].
    CurrentColor,
    /// An explicit color from the document.
    Color(Color),
}

/// A stroke's paint source, width and joins.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StrokeStyle {
    pub source: ColorSource,
    /// Width in very same units as the geometry (user / viewBox units).
    pub width: f32,
    pub cap: LineCap,
    pub join: LineJoin,
}

impl Default for StrokeStyle {
    fn default() -> Self {
        Self {
            source: ColorSource::None,
            width: 1.0,
            cap: LineCap::Butt,
            join: LineJoin::Miter,
        }
    }
}

/// A flattened piece of geometry: connected points plus whether it is closed.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Subpath {
    pub points: Vec<Vec2>,
    pub closed: bool,
}

impl Subpath {
    pub fn new(points: Vec<Vec2>, closed: bool) -> Self {
        Self { points, closed }
    }
}

/// One drawable element with its resolved stroke style.
#[derive(Debug, Clone, PartialEq)]
pub struct Shape {
    pub subpaths: Vec<Subpath>,
    pub stroke: StrokeStyle,
}

/// A parsed SVG document.
#[derive(Debug, Clone, PartialEq)]
pub struct SvgDocument {
    pub view_box: ViewBox,
    pub shapes: Vec<Shape>,
}

impl SvgDocument {
    /// Parses an SVG document string. Returns an error for malformed input, but
    /// tolerates unknown elements/attributes by ignoring them.
    pub fn parse(source: &str) -> Result<Self, SvgError> {
        parse::parse(source)
    }

    /// The user-space bounds (`viewBox`) as a rectangle.
    pub fn bounds(&self) -> Rect {
        self.view_box.rect()
    }

    /// `true` when the document has no stroked geometry to draw.
    pub fn is_empty(&self) -> bool {
        self.shapes.is_empty()
    }
}

impl std::str::FromStr for SvgDocument {
    type Err = SvgError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

/// A parse failure. Unknown elements/attributes are *not* errors; these signal
/// malformed path data or an unreadable root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SvgError {
    /// The document had no `<svg>` root.
    MissingRoot,
    /// An attribute value could not be parsed.
    BadAttribute(String),
    /// A `d` / `points` attribute was malformed.
    BadPath(String),
    /// Reading an icon file from an [`IconPack`] failed.
    Io(String),
}

impl fmt::Display for SvgError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingRoot => write!(f, "the document has no <svg> root"),
            Self::BadAttribute(message) => write!(f, "bad attribute: {message}"),
            Self::BadPath(message) => write!(f, "bad path data: {message}"),
            Self::Io(message) => write!(f, "icon read error: {message}"),
        }
    }
}

impl std::error::Error for SvgError {}
