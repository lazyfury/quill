//! Painting a parsed document into the backend-neutral IR.
//!
//! Geometry is mapped from user (viewBox) space into the target rectangle with
//! `preserveAspectRatio="xMidYMid meet"` semantics (uniform scale, centred), and
//! each subpath is stroked with `Line` segments plus `FillCircle` for round
//! joins/caps.

use draw_core::{Color, Rect, Vec2};
use draw_render::{Paint, PaintContext};

use crate::{LineCap, LineJoin, SvgDocument, ViewBox};

impl SvgDocument {
    /// Strokes the document into `ctx`, scaled and centred inside `target`.
    ///
    /// `current` resolves `stroke="currentColor"`; documents that carry explicit
    /// colors use those instead.
    pub fn draw(&self, ctx: &mut PaintContext, target: Rect, current: Color) {
        let mapping = Mapping::new(self.view_box, target);
        for shape in &self.shapes {
            let paint = match shape.stroke.source {
                crate::ColorSource::None => continue,
                crate::ColorSource::CurrentColor => Paint::new(current),
                crate::ColorSource::Color(color) => Paint::new(color),
            };
            let width = shape.stroke.width * mapping.scale;
            if width <= 0.0 {
                continue;
            }
            for subpath in &shape.subpaths {
                emit(
                    ctx,
                    subpath,
                    &mapping,
                    width,
                    shape.stroke.cap,
                    shape.stroke.join,
                    paint,
                );
            }
        }
    }
}

/// User-space -> target mapping (uniform scale, centred).
struct Mapping {
    origin: Vec2,
    scale: f32,
    min: Vec2,
}

impl Mapping {
    fn new(view_box: ViewBox, target: Rect) -> Self {
        let scale = if view_box.size.width > 0.0 && view_box.size.height > 0.0 {
            (target.size.width / view_box.size.width).min(target.size.height / view_box.size.height)
        } else {
            1.0
        };
        let drawn = Vec2::new(view_box.size.width * scale, view_box.size.height * scale);
        let origin =
            target.origin + (Vec2::new(target.size.width, target.size.height) - drawn) * 0.5;
        Self {
            origin,
            scale,
            min: view_box.min,
        }
    }

    fn point(&self, point: Vec2) -> Vec2 {
        self.origin + (point - self.min) * self.scale
    }
}

fn emit(
    ctx: &mut PaintContext,
    subpath: &crate::Subpath,
    mapping: &Mapping,
    width: f32,
    cap: LineCap,
    join: LineJoin,
    paint: Paint,
) {
    // Segments shorter than this (logical px) are treated as coincident.
    const MIN_SEGMENT: f32 = 0.05;
    // A joint whose outgoing direction is within ~10° of the incoming one does
    // not need a round cover.
    const STRAIGHT_JOINT_DOT: f32 = 0.985;
    let points: Vec<Vec2> = subpath.points.iter().map(|p| mapping.point(*p)).collect();
    let count = points.len();
    if count == 0 {
        return;
    }
    if count == 1 {
        if cap == LineCap::Round {
            ctx.fill_circle(points[0], width * 0.5, paint);
        }
        return;
    }

    let segments = if subpath.closed { count } else { count - 1 };
    for segment in 0..segments {
        let from = points[segment];
        let to = points[(segment + 1) % count];
        // Flattening can emit coincident points; a zero-length quad would add
        // nothing but a command.
        if (to - from).length_squared() < MIN_SEGMENT * MIN_SEGMENT {
            continue;
        }
        ctx.draw_line(from, to, width, paint);
    }

    if join == LineJoin::Round {
        let joints = if subpath.closed {
            0..count
        } else {
            1..count.saturating_sub(1)
        };
        for index in joints {
            // A round join only matters when the outline actually turns. For a
            // flattened arc the segments are nearly collinear and the cover
            // would be invisible, so skip it (this is most of the command
            // budget for icon-sized artwork).
            let previous = points[(index + count - 1) % count];
            let next = points[(index + 1) % count];
            let incoming = (points[index] - previous).normalize_or_zero();
            let outgoing = (next - points[index]).normalize_or_zero();
            if incoming.dot(outgoing) > STRAIGHT_JOINT_DOT {
                continue;
            }
            ctx.fill_circle(points[index], width * 0.5, paint);
        }
    }

    if cap == LineCap::Round && !subpath.closed {
        ctx.fill_circle(points[0], width * 0.5, paint);
        ctx.fill_circle(points[count - 1], width * 0.5, paint);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use draw_core::Size;
    use draw_render::DrawCommand;

    fn draw(svg: &str, target: Rect, color: Color) -> Vec<DrawCommand> {
        let document = SvgDocument::parse(svg).unwrap();
        let mut ctx = PaintContext::new();
        document.draw(&mut ctx, target, color);
        ctx.into_draw_list().into_commands()
    }

    fn square(size: f32) -> Rect {
        Rect::from_min_size(Vec2::ZERO, Size::splat(size))
    }

    #[test]
    fn a_line_maps_into_the_target_box() {
        let commands = draw(
            r#"<svg viewBox="0 0 24 24" stroke="currentColor"><path d="M0 0 L24 0" /></svg>"#,
            square(24.0),
            Color::BLACK,
        );
        let line = commands
            .iter()
            .find_map(|command| match command {
                DrawCommand::Line { from, to, .. } => Some((*from, *to)),
                _ => None,
            })
            .expect("a Line command");
        assert_eq!(line.0, Vec2::new(0.0, 0.0));
        assert_eq!(line.1, Vec2::new(24.0, 0.0));
    }

    #[test]
    fn a_wider_target_scales_and_centres() {
        // 24x24 viewBox into a 48x24 box: scale stays 1 (meet), centred on x.
        let commands = draw(
            r#"<svg viewBox="0 0 24 24" stroke="black"><path d="M0 0 L24 0" /></svg>"#,
            Rect::from_min_size(Vec2::ZERO, Size::new(48.0, 24.0)),
            Color::BLACK,
        );
        let line = commands
            .iter()
            .find_map(|command| match command {
                DrawCommand::Line { from, to, .. } => Some((*from, *to)),
                _ => None,
            })
            .unwrap();
        assert_eq!(line.0, Vec2::new(12.0, 0.0));
        assert_eq!(line.1, Vec2::new(36.0, 0.0));
    }

    #[test]
    fn current_color_resolves_to_the_passed_paint() {
        let commands = draw(
            r#"<svg viewBox="0 0 24 24" stroke="currentColor"><path d="M0 0 L4 0" /></svg>"#,
            square(24.0),
            Color::RED,
        );
        let paint = commands
            .iter()
            .find_map(|command| match command {
                DrawCommand::Line { paint, .. } => Some(*paint),
                _ => None,
            })
            .unwrap();
        assert_eq!(paint, Paint::new(Color::RED));
    }

    #[test]
    fn an_explicit_color_overrides_the_passed_paint() {
        let commands = draw(
            r##"<svg viewBox="0 0 24 24" stroke="#0000ff"><path d="M0 0 L4 0" /></svg>"##,
            square(24.0),
            Color::RED,
        );
        let paint = commands
            .iter()
            .find_map(|command| match command {
                DrawCommand::Line { paint, .. } => Some(*paint),
                _ => None,
            })
            .unwrap();
        assert_eq!(paint, Paint::new(Color::BLUE));
    }

    #[test]
    fn stroke_none_draws_nothing() {
        let commands = draw(
            r#"<svg viewBox="0 0 24 24" stroke="none"><path d="M0 0 L4 0" /></svg>"#,
            square(24.0),
            Color::BLACK,
        );
        assert!(commands.is_empty());
    }

    #[test]
    fn a_round_join_adds_a_filled_circle_at_the_corner() {
        let commands = draw(
            r#"<svg viewBox="0 0 24 24" stroke="black" stroke-linejoin="round"><path d="M0 0 L12 12 L24 0" /></svg>"#,
            square(24.0),
            Color::BLACK,
        );
        assert!(commands
            .iter()
            .any(|command| matches!(command, DrawCommand::FillCircle { .. })));
        assert_eq!(
            commands
                .iter()
                .filter(|command| matches!(command, DrawCommand::Line { .. }))
                .count(),
            2
        );
    }

    #[test]
    fn butt_caps_emit_no_extra_circles() {
        let commands = draw(
            r#"<svg viewBox="0 0 24 24" stroke="black"><path d="M0 0 L24 0" /></svg>"#,
            square(24.0),
            Color::BLACK,
        );
        assert_eq!(commands.len(), 1, "one line, no caps: {commands:?}");
    }
}
