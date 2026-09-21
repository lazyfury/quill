//! SVG basic shapes (`rect` / `circle` / `ellipse` / `line` / `polyline` /
//! `polygon`) flattened to subpaths. `path` data lives in [`crate::path`].

use std::f32::consts::TAU;

use draw_core::Vec2;

use crate::Subpath;

/// Segment count for a full circle / ellipse.
const ELLIPSE_SEGMENTS: u32 = 64;
/// Segment count for one rounded-rectangle corner (a quarter arc).
const CORNER_SEGMENTS: u32 = 12;

pub fn line(x1: f32, y1: f32, x2: f32, y2: f32) -> Subpath {
    Subpath::new(vec![Vec2::new(x1, y1), Vec2::new(x2, y2)], false)
}

pub fn poly(points: Vec<Vec2>, closed: bool) -> Subpath {
    Subpath::new(points, closed)
}

pub fn circle(cx: f32, cy: f32, r: f32) -> Subpath {
    ellipse(cx, cy, r, r)
}

pub fn ellipse(cx: f32, cy: f32, rx: f32, ry: f32) -> Subpath {
    let mut points = Vec::with_capacity(ELLIPSE_SEGMENTS as usize);
    for segment in 0..ELLIPSE_SEGMENTS {
        let angle = TAU * segment as f32 / ELLIPSE_SEGMENTS as f32;
        points.push(Vec2::new(cx + rx * angle.cos(), cy + ry * angle.sin()));
    }
    Subpath::new(points, true)
}

/// A rectangle, optionally with rounded corners (radii clamped to half sides).
pub fn rect(x: f32, y: f32, width: f32, height: f32, rx: f32, ry: f32) -> Subpath {
    let rx = rx.max(0.0).min(width * 0.5);
    let ry = ry.max(0.0).min(height * 0.5);
    if rx <= f32::EPSILON && ry <= f32::EPSILON {
        return Subpath::new(
            vec![
                Vec2::new(x, y),
                Vec2::new(x + width, y),
                Vec2::new(x + width, y + height),
                Vec2::new(x, y + height),
            ],
            true,
        );
    }

    let mut points = Vec::new();
    let (l, t, r, b) = (x, y, x + width, y + height);
    // Walk clockwise: top edge, top-right corner, right edge, ... bottom-left.
    points.push(Vec2::new(l + rx, t));
    points.push(Vec2::new(r - rx, t));
    push_corner(&mut points, r - rx, t + ry, rx, ry, -TAU / 4.0, 0.0);
    points.push(Vec2::new(r, b - ry));
    push_corner(&mut points, r - rx, b - ry, rx, ry, 0.0, TAU / 4.0);
    points.push(Vec2::new(l + rx, b));
    push_corner(&mut points, l + rx, b - ry, rx, ry, TAU / 4.0, TAU / 2.0);
    points.push(Vec2::new(l, t + ry));
    push_corner(&mut points, l + rx, t + ry, rx, ry, TAU / 2.0, TAU * 0.75);
    // The last corner ends where the first point started; drop the duplicate so
    // the seam does not become a zero-length segment.
    if points.len() > 1 && points.first() == points.last() {
        points.pop();
    }
    Subpath::new(points, true)
}

fn push_corner(points: &mut Vec<Vec2>, cx: f32, cy: f32, rx: f32, ry: f32, start: f32, end: f32) {
    for segment in 1..=CORNER_SEGMENTS {
        let t = segment as f32 / CORNER_SEGMENTS as f32;
        let angle = start + (end - start) * t;
        points.push(Vec2::new(cx + rx * angle.cos(), cy + ry * angle.sin()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_is_two_open_points() {
        let subpath = line(0.0, 0.0, 3.0, 4.0);
        assert_eq!(subpath.points.len(), 2);
        assert!(!subpath.closed);
    }

    #[test]
    fn a_square_rect_is_four_corners() {
        let subpath = rect(1.0, 2.0, 4.0, 6.0, 0.0, 0.0);
        assert_eq!(
            subpath.points,
            vec![
                Vec2::new(1.0, 2.0),
                Vec2::new(5.0, 2.0),
                Vec2::new(5.0, 8.0),
                Vec2::new(1.0, 8.0),
            ]
        );
        assert!(subpath.closed);
    }

    #[test]
    fn a_rounded_rect_keeps_all_points_inside_the_box() {
        let subpath = rect(0.0, 0.0, 10.0, 6.0, 2.0, 2.0);
        assert!(subpath.closed);
        assert!(subpath.points.len() > 4);
        for point in &subpath.points {
            assert!(
                (-0.01..=10.01).contains(&point.x) && (-0.01..=6.01).contains(&point.y),
                "point escaped the box: {point:?}"
            );
        }
    }

    #[test]
    fn a_circle_is_closed_and_on_radius() {
        let subpath = circle(5.0, 5.0, 2.0);
        assert!(subpath.closed);
        for point in &subpath.points {
            let distance = (*point - Vec2::new(5.0, 5.0)).length();
            assert!((distance - 2.0).abs() < 0.01, "off radius: {distance}");
        }
    }

    #[test]
    fn an_ellipse_uses_both_radii() {
        let subpath = ellipse(0.0, 0.0, 4.0, 1.0);
        let max_x = subpath
            .points
            .iter()
            .map(|p| p.x.abs())
            .fold(0.0_f32, f32::max);
        let max_y = subpath
            .points
            .iter()
            .map(|p| p.y.abs())
            .fold(0.0_f32, f32::max);
        assert!((max_x - 4.0).abs() < 0.05);
        assert!((max_y - 1.0).abs() < 0.05);
    }
}
