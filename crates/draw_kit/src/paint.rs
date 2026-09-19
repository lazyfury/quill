//! Backend-neutral surface helpers used by the component library.
//!
//! Rounded rectangles are a first-class [`DrawCommand`](draw_render::DrawCommand)
//! now, so surfaces map almost directly onto the IR.

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

/// Fills a rounded rectangle (radius is clamped by the backend).
pub fn fill_rounded_rect(ctx: &mut PaintContext, rect: Rect, radius: f32, color: Color) {
    if color.is_transparent() || rect.size.width <= 0.0 || rect.size.height <= 0.0 {
        return;
    }
    ctx.fill_rounded_rect(rect, radius, color);
}

/// Draws a rounded surface: the fill fills the rect; the border is stroked
/// just inside its bounds so it never bleeds outside the component.
pub fn surface(ctx: &mut PaintContext, rect: Rect, style: &SurfaceStyle) {
    match style.border {
        Some(border) if style.border_width > 0.0 => {
            if !style.fill.is_transparent() {
                let inner = inset(rect, style.border_width);
                if inner.size.width > 0.0 && inner.size.height > 0.0 {
                    ctx.fill_rounded_rect(
                        inner,
                        (style.radius - style.border_width).max(0.0),
                        style.fill,
                    );
                }
            }
            let half = style.border_width * 0.5;
            let outline = inset(rect, half);
            if outline.size.width > 0.0 && outline.size.height > 0.0 {
                ctx.stroke_rounded_rect(
                    outline,
                    (style.radius - half).max(0.0),
                    style.border_width,
                    border,
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

    fn count(list: &draw_render::DrawList, f: impl Fn(&DrawCommand) -> bool) -> usize {
        list.commands().iter().filter(|c| f(c)).count()
    }

    #[test]
    fn rounded_fill_emits_one_command() {
        let mut ctx = PaintContext::new();
        fill_rounded_rect(&mut ctx, rect(), 6.0, Color::WHITE);
        let list = ctx.into_draw_list();
        assert_eq!(list.len(), 1);
        assert!(matches!(
            list.commands()[0],
            DrawCommand::FillRoundedRect { .. }
        ));
    }

    #[test]
    fn transparent_fill_emits_nothing() {
        let mut ctx = PaintContext::new();
        fill_rounded_rect(&mut ctx, rect(), 4.0, Color::TRANSPARENT);
        assert!(ctx.into_draw_list().is_empty());
    }

    #[test]
    fn surface_with_border_fills_inside_and_strokes_inside() {
        let mut ctx = PaintContext::new();
        let style = SurfaceStyle::new(Color::WHITE)
            .border(Color::BLACK)
            .radius(6.0);
        surface(&mut ctx, rect(), &style);
        let list = ctx.into_draw_list();
        assert_eq!(
            count(&list, |c| matches!(c, DrawCommand::FillRoundedRect { .. })),
            1
        );
        assert_eq!(
            count(&list, |c| matches!(
                c,
                DrawCommand::StrokeRoundedRect { .. }
            )),
            1
        );
        // The stroke outline stays inside the surface bounds.
        if let DrawCommand::StrokeRoundedRect { rect: outline, .. } = list.commands()[1] {
            assert!(outline.left() >= rect().left());
            assert!(outline.right() <= rect().right());
        } else {
            panic!("expected a rounded stroke");
        }
    }
}
