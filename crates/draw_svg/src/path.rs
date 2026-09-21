//! SVG path data (`d`) parsing and flattening into polylines.
//!
//! Implements the full path grammar (`M L H V C S Q T A Z`, absolute and
//! relative) and flattens curves to line segments with an adaptive
//! subdivision, so the result can be stroked with straight `Line` commands.

use draw_core::Vec2;

use crate::{Subpath, SvgError};

/// Flattening tolerance for béziers, in user (viewBox) units.
const TOLERANCE: f32 = 0.02;
/// Recursion guard for pathological control polygons.
const MAX_DEPTH: u32 = 16;
/// Angular step when sampling elliptical arcs (~5.6°).
const ARC_STEP: f32 = std::f32::consts::PI / 32.0;
const TAU: f32 = std::f32::consts::TAU;

/// Parses a `d` attribute into flattened subpaths (user space).
pub fn parse(d: &str) -> Result<Vec<Subpath>, SvgError> {
    let mut lex = Lexer::new(d);
    let mut out: Vec<Subpath> = Vec::new();
    let mut current: Vec<Vec2> = Vec::new();
    let mut pos = Vec2::ZERO;
    let mut start = Vec2::ZERO;
    let mut command: Option<char> = None;
    let mut previous: Option<char> = None;
    let mut cubic_ctrl: Option<Vec2> = None;
    let mut quad_ctrl: Option<Vec2> = None;

    loop {
        lex.skip_sep();
        if lex.eof() {
            break;
        }
        if let Some(letter) = lex.peek_alpha() {
            lex.bump();
            command = Some(letter);
        }
        let letter = command.ok_or_else(|| {
            SvgError::BadPath("path data must begin with a command letter".to_string())
        })?;
        let relative = letter.is_ascii_lowercase();
        match letter.to_ascii_uppercase() {
            'M' => {
                flush(&mut out, &mut current);
                let mut point = lex.point()?;
                if relative {
                    point += pos;
                }
                pos = point;
                start = point;
                current.push(point);
                command = Some(if relative { 'l' } else { 'L' });
            }
            'L' => {
                ensure_started(&mut current, pos);
                let mut point = lex.point()?;
                if relative {
                    point += pos;
                }
                current.push(point);
                pos = point;
            }
            'H' => {
                ensure_started(&mut current, pos);
                let x = lex.number()?;
                pos = Vec2::new(if relative { pos.x + x } else { x }, pos.y);
                current.push(pos);
            }
            'V' => {
                ensure_started(&mut current, pos);
                let y = lex.number()?;
                pos = Vec2::new(pos.x, if relative { pos.y + y } else { y });
                current.push(pos);
            }
            'C' => {
                prepare(&mut current, pos);
                let from = pos;
                let (mut c1, mut c2, mut to) = (lex.point()?, lex.point()?, lex.point()?);
                if relative {
                    c1 += from;
                    c2 += from;
                    to += from;
                }
                flatten_cubic(from, c1, c2, to, &mut current);
                cubic_ctrl = Some(c2);
                quad_ctrl = None;
                pos = to;
            }
            'S' => {
                prepare(&mut current, pos);
                let from = pos;
                let c1 = reflect(previous, cubic_ctrl, from, &['C', 'S']);
                let (mut c2, mut to) = (lex.point()?, lex.point()?);
                if relative {
                    c2 += from;
                    to += from;
                }
                flatten_cubic(from, c1, c2, to, &mut current);
                cubic_ctrl = Some(c2);
                quad_ctrl = None;
                pos = to;
            }
            'Q' => {
                prepare(&mut current, pos);
                let from = pos;
                let (mut ctrl, mut to) = (lex.point()?, lex.point()?);
                if relative {
                    ctrl += from;
                    to += from;
                }
                flatten_quad(from, ctrl, to, &mut current);
                quad_ctrl = Some(ctrl);
                cubic_ctrl = None;
                pos = to;
            }
            'T' => {
                prepare(&mut current, pos);
                let from = pos;
                let ctrl = reflect(previous, quad_ctrl, from, &['Q', 'T']);
                let mut to = lex.point()?;
                if relative {
                    to += from;
                }
                flatten_quad(from, ctrl, to, &mut current);
                quad_ctrl = Some(ctrl);
                cubic_ctrl = None;
                pos = to;
            }
            'A' => {
                prepare(&mut current, pos);
                let from = pos;
                let rx = lex.number()?;
                let ry = lex.number()?;
                let rotation = lex.number()?;
                let large = lex.flag()?;
                let sweep = lex.flag()?;
                let mut to = lex.point()?;
                if relative {
                    to += from;
                }
                flatten_arc(from, rx, ry, rotation, large, sweep, to, &mut current);
                cubic_ctrl = None;
                quad_ctrl = None;
                pos = to;
            }
            'Z' => {
                if !current.is_empty() {
                    if current.last().copied() != Some(start) {
                        current.push(start);
                    }
                    out.push(Subpath::new(std::mem::take(&mut current), true));
                }
                pos = start;
                cubic_ctrl = None;
                quad_ctrl = None;
                command = None;
            }
            other => {
                return Err(SvgError::BadPath(format!(
                    "unsupported path command `{other}`"
                )));
            }
        }
        previous = Some(letter);
    }

    flush(&mut out, &mut current);
    Ok(out)
}

fn flush(out: &mut Vec<Subpath>, current: &mut Vec<Vec2>) {
    if !current.is_empty() {
        out.push(Subpath::new(std::mem::take(current), false));
    }
}

/// A drawing command after a `Z` starts its own subpath at the current point.
fn ensure_started(current: &mut Vec<Vec2>, pos: Vec2) {
    if current.is_empty() {
        current.push(pos);
    }
}

/// Same as [`ensure_started`], but for curves (never pushes the endpoint yet).
fn prepare(current: &mut Vec<Vec2>, pos: Vec2) {
    ensure_started(current, pos);
}

/// Reflection of a control point for the smooth `S` / `T` commands.
fn reflect(previous: Option<char>, control: Option<Vec2>, from: Vec2, allowed: &[char]) -> Vec2 {
    match (previous, control) {
        (Some(letter), Some(control)) if allowed.contains(&letter.to_ascii_uppercase()) => {
            from * 2.0 - control
        }
        _ => from,
    }
}

// -- flattening ------------------------------------------------------------

fn flatten_cubic(p0: Vec2, p1: Vec2, p2: Vec2, p3: Vec2, out: &mut Vec<Vec2>) {
    rec_cubic(p0, p1, p2, p3, 0, out);
}

fn rec_cubic(p0: Vec2, p1: Vec2, p2: Vec2, p3: Vec2, depth: u32, out: &mut Vec<Vec2>) {
    if depth >= MAX_DEPTH || flat_enough(p0, p1, p2, p3) {
        out.push(p3);
        return;
    }
    let p01 = (p0 + p1) * 0.5;
    let p12 = (p1 + p2) * 0.5;
    let p23 = (p2 + p3) * 0.5;
    let p012 = (p01 + p12) * 0.5;
    let p123 = (p12 + p23) * 0.5;
    let mid = (p012 + p123) * 0.5;
    rec_cubic(p0, p01, p012, mid, depth + 1, out);
    rec_cubic(mid, p123, p23, p3, depth + 1, out);
}

fn flat_enough(p0: Vec2, p1: Vec2, p2: Vec2, p3: Vec2) -> bool {
    point_line_distance(p1, p0, p3) + point_line_distance(p2, p0, p3) <= TOLERANCE
}

fn point_line_distance(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let ab = b - a;
    let length = ab.length();
    if length <= f32::EPSILON {
        return (p - a).length();
    }
    (p - a).cross(ab).abs() / length
}

fn flatten_quad(p0: Vec2, ctrl: Vec2, p1: Vec2, out: &mut Vec<Vec2>) {
    // Elevate the quadratic to a cubic, then reuse the cubic flattener.
    let c1 = p0 + (ctrl - p0) * (2.0 / 3.0);
    let c2 = p1 + (ctrl - p1) * (2.0 / 3.0);
    flatten_cubic(p0, c1, c2, p1, out);
}

#[allow(clippy::too_many_arguments)]
fn flatten_arc(
    from: Vec2,
    rx: f32,
    ry: f32,
    rotation_deg: f32,
    large_arc: bool,
    sweep: bool,
    to: Vec2,
    out: &mut Vec<Vec2>,
) {
    if (from - to).length() <= f32::EPSILON {
        return;
    }
    let mut rx = rx.abs();
    let mut ry = ry.abs();
    if rx <= f32::EPSILON || ry <= f32::EPSILON {
        out.push(to);
        return;
    }
    let phi = rotation_deg.to_radians();
    let (sin_phi, cos_phi) = phi.sin_cos();
    let half = (from - to) * 0.5;
    let x1p = cos_phi * half.x + sin_phi * half.y;
    let y1p = -sin_phi * half.x + cos_phi * half.y;

    // Scale the radii up if they are too small to reach `to` (SVG F.6.6).
    let lambda = (x1p * x1p) / (rx * rx) + (y1p * y1p) / (ry * ry);
    if lambda > 1.0 {
        let scale = lambda.sqrt();
        rx *= scale;
        ry *= scale;
    }

    let numerator = (rx * rx * ry * ry - rx * rx * y1p * y1p - ry * ry * x1p * x1p).max(0.0);
    let denominator = rx * rx * y1p * y1p + ry * ry * x1p * x1p;
    let mut coefficient = if denominator <= f32::EPSILON {
        0.0
    } else {
        (numerator / denominator).sqrt()
    };
    if large_arc == sweep {
        coefficient = -coefficient;
    }
    let cxp = coefficient * (rx * y1p / ry);
    let cyp = coefficient * (-ry * x1p / rx);
    let center = Vec2::new(
        cos_phi * cxp - sin_phi * cyp + (from.x + to.x) * 0.5,
        sin_phi * cxp + cos_phi * cyp + (from.y + to.y) * 0.5,
    );

    let ux = (x1p - cxp) / rx;
    let uy = (y1p - cyp) / ry;
    let vx = (-x1p - cxp) / rx;
    let vy = (-y1p - cyp) / ry;
    let theta1 = uy.atan2(ux);
    let mut delta = (ux * vy - uy * vx).atan2(ux * vx + uy * vy);
    if !sweep && delta > 0.0 {
        delta -= TAU;
    } else if sweep && delta < 0.0 {
        delta += TAU;
    }

    let segments = (delta.abs() / ARC_STEP).ceil().max(1.0) as u32;
    let before = out.len();
    for step in 1..=segments {
        let angle = theta1 + delta * (step as f32 / segments as f32);
        let (sin_t, cos_t) = angle.sin_cos();
        let ex = rx * cos_t;
        let ey = ry * sin_t;
        out.push(Vec2::new(
            cos_phi * ex - sin_phi * ey + center.x,
            sin_phi * ex + cos_phi * ey + center.y,
        ));
    }
    if out.len() > before {
        if let Some(last) = out.last_mut() {
            *last = to;
        }
    }
}

// -- lexer -----------------------------------------------------------------

struct Lexer<'a> {
    bytes: &'a [u8],
    index: usize,
}

impl<'a> Lexer<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            bytes: source.as_bytes(),
            index: 0,
        }
    }

    fn eof(&self) -> bool {
        self.index >= self.bytes.len()
    }

    fn byte(&self) -> Option<u8> {
        self.bytes.get(self.index).copied()
    }

    fn bump(&mut self) {
        self.index += 1;
    }

    fn skip_sep(&mut self) {
        while let Some(byte) = self.byte() {
            if byte == b',' || (byte as char).is_ascii_whitespace() {
                self.index += 1;
            } else {
                break;
            }
        }
    }

    fn peek_alpha(&mut self) -> Option<char> {
        self.skip_sep();
        match self.byte() {
            Some(byte) if byte.is_ascii_alphabetic() => Some(byte as char),
            _ => None,
        }
    }

    fn number(&mut self) -> Result<f32, SvgError> {
        self.skip_sep();
        let start = self.index;
        if matches!(self.byte(), Some(b'+') | Some(b'-')) {
            self.index += 1;
        }
        let mut digits = false;
        while matches!(self.byte(), Some(b'0'..=b'9')) {
            self.index += 1;
            digits = true;
        }
        if self.byte() == Some(b'.') {
            self.index += 1;
            while matches!(self.byte(), Some(b'0'..=b'9')) {
                self.index += 1;
                digits = true;
            }
        }
        if digits && matches!(self.byte(), Some(b'e') | Some(b'E')) {
            self.index += 1;
            if matches!(self.byte(), Some(b'+') | Some(b'-')) {
                self.index += 1;
            }
            while matches!(self.byte(), Some(b'0'..=b'9')) {
                self.index += 1;
            }
        }
        if !digits {
            return Err(SvgError::BadPath(format!(
                "expected a number at byte {start}"
            )));
        }
        std::str::from_utf8(&self.bytes[start..self.index])
            .ok()
            .and_then(|text| text.parse().ok())
            .ok_or_else(|| SvgError::BadPath(format!("invalid number at byte {start}")))
    }

    fn point(&mut self) -> Result<Vec2, SvgError> {
        Ok(Vec2::new(self.number()?, self.number()?))
    }

    fn flag(&mut self) -> Result<bool, SvgError> {
        self.skip_sep();
        match self.byte() {
            Some(b'0') => {
                self.index += 1;
                Ok(false)
            }
            Some(b'1') => {
                self.index += 1;
                Ok(true)
            }
            _ => Err(SvgError::BadPath(
                "expected an arc flag (0 or 1)".to_string(),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn points(d: &str) -> Vec<Vec2> {
        parse(d).expect("valid path").remove(0).points
    }

    #[test]
    fn a_move_to_line_to_is_two_points() {
        assert_eq!(
            points("M2 2 L10 10"),
            vec![Vec2::new(2.0, 2.0), Vec2::new(10.0, 10.0)]
        );
    }

    #[test]
    fn relative_commands_accumulate() {
        assert_eq!(
            points("m1 1 l2 0 l0 2"),
            vec![
                Vec2::new(1.0, 1.0),
                Vec2::new(3.0, 1.0),
                Vec2::new(3.0, 3.0)
            ]
        );
    }

    #[test]
    fn h_and_v_move_one_axis() {
        assert_eq!(
            points("M1 1 H5 V4"),
            vec![
                Vec2::new(1.0, 1.0),
                Vec2::new(5.0, 1.0),
                Vec2::new(5.0, 4.0)
            ]
        );
    }

    #[test]
    fn compact_numbers_without_separators_parse() {
        // `10-20` means `10 -20`; `.5.5` means `.5 .5`.
        assert_eq!(
            points("M10-20l.5.5"),
            vec![Vec2::new(10.0, -20.0), Vec2::new(10.5, -19.5)]
        );
    }

    #[test]
    fn a_closed_subpath_repeats_the_start() {
        let subpaths = parse("M0 0 L4 0 L4 4 Z").unwrap();
        assert_eq!(subpaths.len(), 1);
        assert!(subpaths[0].closed);
        assert_eq!(subpaths[0].points.len(), 4);
        assert_eq!(subpaths[0].points[3], Vec2::new(0.0, 0.0));
    }

    #[test]
    fn a_cubic_flattens_between_its_endpoints() {
        let pts = points("M0 0 C0 10 10 10 10 0");
        assert_eq!(pts[0], Vec2::new(0.0, 0.0));
        assert_eq!(*pts.last().unwrap(), Vec2::new(10.0, 0.0));
        assert!(pts.len() > 4, "curve should be subdivided: {}", pts.len());
        assert!(pts.iter().all(|p| p.y >= -0.01), "curve dips below start");
    }

    #[test]
    fn a_smooth_cubic_reflects_the_previous_control_point() {
        // The second curve should start heading the same way the first ended.
        let pts = points("M0 0 C0 4 4 4 4 0 S8-4 8 0");
        assert_eq!(*pts.last().unwrap(), Vec2::new(8.0, 0.0));
        assert!(pts.iter().any(|p| p.y < -0.1), "S curve should bow upward");
    }

    #[test]
    fn an_arc_reaches_its_endpoint_and_bows() {
        let pts = points("M0 0 A5 5 0 0 1 10 0");
        assert_eq!(*pts.last().unwrap(), Vec2::new(10.0, 0.0));
        // The arc bows away from the chord (sign depends on the sweep flag).
        assert!(
            pts.iter().any(|p| p.y.abs() > 1.0),
            "arc should bow: {pts:?}"
        );
    }

    #[test]
    fn an_unsupported_command_is_an_error() {
        assert!(parse("M0 0 X1 1").is_err());
    }

    #[test]
    fn multiple_subpaths_are_separated_by_move_to() {
        let subpaths = parse("M0 0 L1 1 M2 2 L3 3").unwrap();
        assert_eq!(subpaths.len(), 2);
        assert_eq!(subpaths[0].points.len(), 2);
        assert_eq!(subpaths[1].points[0], Vec2::new(2.0, 2.0));
    }
}
