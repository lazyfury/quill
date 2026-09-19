//! Backend-neutral drawing helpers used by the component library.
//!
//! The render IR has axis-aligned rects and circles but no rounded rect. We
//! compose one out of non-overlapping rects and corner circles so translucent
//! fills do not double-blend.

use draw_core::{Color, Rect, Vec2};
use draw_render::PaintContext;

/// A rounded surface: fill, optional hairline border and corner radius.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SurfaceStyle {
    pub fill: Color,
    pub border: Option<Color>,
    pub border_width: f32,
    pub radius: f32,
}

impl SurfaceStyle {
    /// A square-cornered opaque fill.
    pub const fn new(fill: Color) -> Self {
        Self {
            fill,
            border: None,
            border_width: 1.0,
            radius: 0.0,
        }
    }

    pub const fn border(mut self, color: Color) -> Self {
        self.border = Some(color);
        self
    }

    pub const fn border_opt(mut self, border: Option<Color>) -> Self {
        self.border = border;
        self
    }

    pub const fn border_width(mut self, width: f32) -> Self {
        self.border_width = width;
        self
    }

    pub const fn radius(mut self, radius: f32) -> Self {
        self.radius = radius;
        self
    }
}

/// Fills a rounded rectangle. `radius` is clamped to half the smaller side.
pub fn fill_rounded_rect(ctx: &mut PaintContext, rect: Rect, radius: f32, color: Color) {
    if color.is_transparent() || rect.size.width <= 0.0 || rect.size.height <= 0.0 {
        return;
    }
    let r = radius
        .max(0.0)
        .min(rect.size.width * 0.5)
        .min(rect.size.height * 0.5);
    if r <= 0.0 {
        ctx.fill_rect(rect, color);
        return;
    }

    let (left, top) = (rect.left(), rect.top());
    let (right, bottom) = (rect.right(), rect.bottom());

    // Interior plus straight edges (no overlaps).
    ctx.fill_rect(
        Rect::from_min_max(Vec2::new(left + r, top), Vec2::new(right - r, bottom)),
        color,
    );
    ctx.fill_rect(
        Rect::from_min_max(Vec2::new(left, top + r), Vec2::new(left + r, bottom - r)),
        color,
    );
    ctx.fill_rect(
        Rect::from_min_max(Vec2::new(right - r, top + r), Vec2::new(right, bottom - r)),
        color,
    );
    // Corner quarters.
    for center in [
        Vec2::new(left + r, top + r),
        Vec2::new(right - r, top + r),
        Vec2::new(left + r, bottom - r),
        Vec2::new(right - r, bottom - r),
    ] {
        ctx.fill_circle(center, r, color);
    }
}

/// Draws a rounded surface (border first, then the inset fill).
pub fn surface(ctx: &mut PaintContext, rect: Rect, style: &SurfaceStyle) {
    match style.border {
        Some(border) if style.border_width > 0.0 => {
            fill_rounded_rect(ctx, rect, style.radius, border);
            let inner = inset(rect, style.border_width);
            if inner.size.width > 0.0 && inner.size.height > 0.0 {
                fill_rounded_rect(
                    ctx,
                    inner,
                    (style.radius - style.border_width).max(0.0),
                    style.fill,
                );
            }
        }
        _ => fill_rounded_rect(ctx, rect, style.radius, style.fill),
    }
}

/// Shrinks a rect by `amount` on every side.
pub fn inset(rect: Rect, amount: f32) -> Rect {
    Rect::from_min_max(
        Vec2::new(rect.left() + amount, rect.top() + amount),
        Vec2::new(rect.right() - amount, rect.bottom() - amount),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use draw_render::DrawCommand;

    fn rect() -> Rect {
        Rect::from_min_max(Vec2::new(10.0, 20.0), Vec2::new(110.0, 60.0))
    }

    #[test]
    fn zero_radius_falls_back_to_plain_rect() {
        let mut ctx = PaintContext::new();
        fill_rounded_rect(&mut ctx, rect(), 0.0, Color::WHITE);
        let list = ctx.into_draw_list();
        assert_eq!(list.len(), 1);
        assert!(matches!(list.commands()[0], DrawCommand::FillRect { .. }));
    }

    #[test]
    fn rounded_fill_emits_rects_and_circles() {
        let mut ctx = PaintContext::new();
        fill_rounded_rect(&mut ctx, rect(), 6.0, Color::WHITE);
        let list = ctx.into_draw_list();
        let circles = list
            .commands()
            .iter()
            .filter(|c| matches!(c, DrawCommand::FillCircle { .. }))
            .count();
        assert_eq!(circles, 4, "expected four corner circles");
        assert!(list.len() > 4);
    }

    #[test]
    fn transparent_fill_emits_nothing() {
        let mut ctx = PaintContext::new();
        fill_rounded_rect(&mut ctx, rect(), 4.0, Color::TRANSPARENT);
        assert!(ctx.into_draw_list().is_empty());
    }

    #[test]
    fn surface_with_border_draws_border_and_inset_fill() {
        let mut ctx = PaintContext::new();
        let style = SurfaceStyle::new(Color::WHITE)
            .border(Color::BLACK)
            .radius(6.0);
        surface(&mut ctx, rect(), &style);
        // Border pass (rounded) + fill pass (rounded) are both present.
        let fills = ctx
            .into_draw_list()
            .commands()
            .iter()
            .filter(|c| matches!(c, DrawCommand::FillRect { .. }))
            .count();
        assert!(fills >= 6);
    }
}
