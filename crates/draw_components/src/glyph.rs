//! Basic vector glyphs drawn from primitives — no SVG files, no extra crate.
//!
//! Each [`Glyph`] is a tiny shape in a 24×24 viewbox (lines, stroked circles
//! and dots), painted into any rectangle by [`paint_glyph`]. Components stroke
//! them with the public [`PaintContext`] primitives, so the core stays
//! file- and backend-free: the checkbox's check mark, a menu chevron, a warning
//! triangle and friends are all geometry in code.
//!
//! ```ignore
//! use draw_components::{paint_glyph, Glyph};
//!
//! // Inside a foreground decorator:
//! paint_glyph(Glyph::Check, ctx, rect, color, 1.8);
//! ```

use draw_core::{Color, Rect, Vec2};
use draw_render::PaintContext;

/// The side of the coordinate space glyphs are authored in.
pub const GLYPH_VIEWBOX: f32 = 24.0;

/// A small monochrome symbol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Glyph {
    /// A check mark.
    Check,
    /// A diagonal cross (close / error).
    Cross,
    /// A short, centered horizontal bar (indeterminate).
    Dash,
    /// A full-width minus sign.
    Minus,
    /// A plus sign.
    Plus,
    ChevronDown,
    ChevronUp,
    ChevronLeft,
    ChevronRight,
    /// A triangle with an exclamation mark.
    Warning,
    /// A circled "i".
    Info,
    /// A magnifier.
    Search,
    /// A single dot.
    Dot,
}

/// The geometry of one glyph in the 24×24 viewbox.
struct Shape {
    /// Stroked segments: `((x1, y1), (x2, y2))`.
    lines: &'static [((f32, f32), (f32, f32))],
    /// Filled dots: `(x, y, radius)`.
    dots: &'static [(f32, f32, f32)],
    /// Stroked circles: `(x, y, radius)`.
    circles: &'static [(f32, f32, f32)],
}

const NO_LINES: &[((f32, f32), (f32, f32))] = &[];
const NO_CIRCLES: &[(f32, f32, f32)] = &[];

impl Glyph {
    fn shape(self) -> Shape {
        match self {
            Glyph::Check => Shape {
                lines: &[((4.5, 12.5), (10.0, 18.0)), ((10.0, 18.0), (19.5, 6.5))],
                dots: &[],
                circles: &[],
            },
            Glyph::Cross => Shape {
                lines: &[((6.0, 6.0), (18.0, 18.0)), ((18.0, 6.0), (6.0, 18.0))],
                dots: &[],
                circles: &[],
            },
            Glyph::Dash => Shape {
                lines: &[((8.0, 12.0), (16.0, 12.0))],
                dots: &[],
                circles: &[],
            },
            Glyph::Minus => Shape {
                lines: &[((5.0, 12.0), (19.0, 12.0))],
                dots: &[],
                circles: &[],
            },
            Glyph::Plus => Shape {
                lines: &[((12.0, 5.0), (12.0, 19.0)), ((5.0, 12.0), (19.0, 12.0))],
                dots: &[],
                circles: &[],
            },
            Glyph::ChevronDown => Shape {
                lines: &[((6.0, 9.5), (12.0, 15.5)), ((12.0, 15.5), (18.0, 9.5))],
                dots: &[],
                circles: &[],
            },
            Glyph::ChevronUp => Shape {
                lines: &[((6.0, 14.5), (12.0, 8.5)), ((12.0, 8.5), (18.0, 14.5))],
                dots: &[],
                circles: &[],
            },
            Glyph::ChevronLeft => Shape {
                lines: &[((15.0, 6.0), (9.0, 12.0)), ((9.0, 12.0), (15.0, 18.0))],
                dots: &[],
                circles: &[],
            },
            Glyph::ChevronRight => Shape {
                lines: &[((9.0, 6.0), (15.0, 12.0)), ((15.0, 12.0), (9.0, 18.0))],
                dots: &[],
                circles: &[],
            },
            Glyph::Warning => Shape {
                lines: &[
                    ((12.0, 4.5), (20.5, 19.0)),
                    ((20.5, 19.0), (3.5, 19.0)),
                    ((3.5, 19.0), (12.0, 4.5)),
                    ((12.0, 10.0), (12.0, 13.8)),
                ],
                dots: &[(12.0, 16.4, 1.2)],
                circles: NO_CIRCLES,
            },
            Glyph::Info => Shape {
                lines: &[((12.0, 11.0), (12.0, 16.5))],
                dots: &[(12.0, 7.6, 1.3)],
                circles: &[(12.0, 12.0, 9.0)],
            },
            Glyph::Search => Shape {
                lines: &[((14.7, 14.7), (20.0, 20.0))],
                dots: &[],
                circles: &[(10.5, 10.5, 6.0)],
            },
            Glyph::Dot => Shape {
                lines: NO_LINES,
                dots: &[(12.0, 12.0, 2.6)],
                circles: NO_CIRCLES,
            },
        }
    }
}

/// Paints `glyph` centered in `rect`, scaled to the smaller side.
///
/// `stroke` is the line width in logical pixels; the dots scale with `rect`.
pub fn paint_glyph(glyph: Glyph, ctx: &mut PaintContext, rect: Rect, color: Color, stroke: f32) {
    let shape = glyph.shape();
    let side = rect.size.width.min(rect.size.height).max(1.0);
    let scale = side / GLYPH_VIEWBOX;
    let origin = rect.center() - Vec2::splat(side / 2.0);
    let map = |x: f32, y: f32| origin + Vec2::new(x * scale, y * scale);

    for ((x1, y1), (x2, y2)) in shape.lines {
        ctx.draw_line(map(*x1, *y1), map(*x2, *y2), stroke, color);
    }
    for (x, y, radius) in shape.circles {
        ctx.stroke_circle(map(*x, *y), radius * scale, stroke, color);
    }
    for (x, y, radius) in shape.dots {
        ctx.fill_circle(map(*x, *y), radius * scale, color);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use draw_render::DrawCommand;

    fn commands(glyph: Glyph) -> Vec<DrawCommand> {
        let mut ctx = PaintContext::new();
        paint_glyph(
            glyph,
            &mut ctx,
            Rect::from_min_size(Vec2::ZERO, draw_core::Size::splat(24.0)),
            Color::WHITE,
            2.0,
        );
        ctx.into_draw_list().commands().to_vec()
    }

    #[test]
    fn every_glyph_paints_at_least_one_primitive() {
        for glyph in [
            Glyph::Check,
            Glyph::Cross,
            Glyph::Dash,
            Glyph::Minus,
            Glyph::Plus,
            Glyph::ChevronDown,
            Glyph::ChevronUp,
            Glyph::ChevronLeft,
            Glyph::ChevronRight,
            Glyph::Warning,
            Glyph::Info,
            Glyph::Search,
            Glyph::Dot,
        ] {
            let list = commands(glyph);
            assert!(!list.is_empty(), "{glyph:?} painted nothing");
            assert!(
                list.iter()
                    .all(|command| !matches!(command, DrawCommand::FillRect { .. })),
                "{glyph:?} should draw strokes/dots, not a fill"
            );
        }
    }

    #[test]
    fn a_check_is_two_segments_meeting_at_the_elbow() {
        let list = commands(Glyph::Check);
        let lines: Vec<_> = list
            .iter()
            .filter_map(|command| match command {
                DrawCommand::Line { from, to, .. } => Some((*from, *to)),
                _ => None,
            })
            .collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].1, lines[1].0, "the two strokes share the elbow");
    }
}
