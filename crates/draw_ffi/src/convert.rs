//! Conversions between the core types and the C ABI records, in both
//! directions. Kept apart from the ABI functions so the marshalling can be
//! tested without touching raw pointers.

use draw_core::{Color, Rect, Size, Transform2D, Vec2};
use draw_render::{CornerRadii, DrawCommand, Paint};

use crate::types::*;

// -- core -> C -------------------------------------------------------------

pub(crate) fn vec2(v: Vec2) -> QuillVec2 {
    QuillVec2 { x: v.x, y: v.y }
}

pub(crate) fn rect(r: Rect) -> QuillRect {
    QuillRect {
        x: r.origin.x,
        y: r.origin.y,
        width: r.size.width,
        height: r.size.height,
    }
}

pub(crate) fn color(c: Color) -> QuillColor {
    QuillColor {
        r: c.r,
        g: c.g,
        b: c.b,
        a: c.a,
    }
}

pub(crate) fn paint(p: Paint) -> QuillPaint {
    QuillPaint {
        color: color(p.color),
    }
}

pub(crate) fn transform(t: Transform2D) -> QuillTransform {
    QuillTransform {
        x_axis: vec2(t.x_axis),
        y_axis: vec2(t.y_axis),
        origin: vec2(t.origin),
    }
}

pub(crate) fn corners(c: CornerRadii) -> QuillCornerRadii {
    QuillCornerRadii {
        top_left: c.top_left,
        top_right: c.top_right,
        bottom_right: c.bottom_right,
        bottom_left: c.bottom_left,
    }
}

// -- C -> core -------------------------------------------------------------

pub(crate) fn to_vec2(v: QuillVec2) -> Vec2 {
    Vec2::new(v.x, v.y)
}

pub(crate) fn to_rect(r: QuillRect) -> Rect {
    Rect::from_min_size(Vec2::new(r.x, r.y), Size::new(r.width, r.height))
}

pub(crate) fn to_color(c: QuillColor) -> Color {
    Color::new(c.r, c.g, c.b, c.a)
}

pub(crate) fn to_paint(p: QuillPaint) -> Paint {
    Paint::new(to_color(p.color))
}

pub(crate) fn to_transform(t: QuillTransform) -> Transform2D {
    Transform2D::new(to_vec2(t.x_axis), to_vec2(t.y_axis), to_vec2(t.origin))
}

pub(crate) fn to_corners(c: QuillCornerRadii) -> CornerRadii {
    CornerRadii::new(c.top_left, c.top_right, c.bottom_right, c.bottom_left)
}

/// Flattens one IR command into the C record.
pub(crate) fn command_record(command: &DrawCommand) -> QuillCommand {
    let mut out = QuillCommand::default();
    match command {
        DrawCommand::Save => out.tag = QuillCommandTag::Save,
        DrawCommand::Restore => out.tag = QuillCommandTag::Restore,
        DrawCommand::SetTransform(t) => {
            out.tag = QuillCommandTag::SetTransform;
            out.transform = transform(*t);
        }
        DrawCommand::SetOpacity(o) => {
            out.tag = QuillCommandTag::SetOpacity;
            out.opacity = *o;
        }
        DrawCommand::ClipRect(r) => {
            out.tag = QuillCommandTag::ClipRect;
            out.rect = rect(*r);
        }
        DrawCommand::FillRect { rect: r, paint: p } => {
            out.tag = QuillCommandTag::FillRect;
            out.rect = rect(*r);
            out.paint = paint(*p);
        }
        DrawCommand::StrokeRect {
            rect: r,
            paint: p,
            width,
        } => {
            out.tag = QuillCommandTag::StrokeRect;
            out.rect = rect(*r);
            out.paint = paint(*p);
            out.width = *width;
        }
        DrawCommand::Line {
            from,
            to,
            paint: p,
            width,
        } => {
            out.tag = QuillCommandTag::Line;
            out.from = vec2(*from);
            out.to = vec2(*to);
            out.paint = paint(*p);
            out.width = *width;
        }
        DrawCommand::FillCircle {
            center,
            radius,
            paint: p,
        } => {
            out.tag = QuillCommandTag::FillCircle;
            out.center = vec2(*center);
            out.radius = *radius;
            out.paint = paint(*p);
        }
        DrawCommand::StrokeCircle {
            center,
            radius,
            paint: p,
            width,
        } => {
            out.tag = QuillCommandTag::StrokeCircle;
            out.center = vec2(*center);
            out.radius = *radius;
            out.paint = paint(*p);
            out.width = *width;
        }
        DrawCommand::FillRoundedRect {
            rect: r,
            corners: c,
            paint: p,
        } => {
            out.tag = QuillCommandTag::FillRoundedRect;
            out.rect = rect(*r);
            out.corners = corners(*c);
            out.paint = paint(*p);
        }
        DrawCommand::StrokeRoundedRect {
            rect: r,
            corners: c,
            paint: p,
            width,
        } => {
            out.tag = QuillCommandTag::StrokeRoundedRect;
            out.rect = rect(*r);
            out.corners = corners(*c);
            out.paint = paint(*p);
            out.width = *width;
        }
        DrawCommand::DrawImage { .. } | DrawCommand::DrawText { .. } => {
            out.tag = QuillCommandTag::Unsupported;
        }
    }
    out
}
