//! Animation previews: `draw_anim` drives an external value that a control's
//! foreground reads at paint time.
//!
//! The `Animator` itself lives in [`crate::DemoApp`]; these cards only clone the
//! shared `Rc<Cell<f32>>`, so one tween powers both. They also show the Stage 27
//! contract: an external-value tween never touches the tree, so a host must keep
//! scheduling frames while it runs — that is exactly what
//! [`crate::DemoApp::needs_frame`] reports.

use draw_anim::Easing;
use draw_components::{Card, Component, Panel};
use draw_core::{Color, Rect, Size, Vec2};
use draw_theme::SurfaceLevel;

use super::Ctx;

/// A progress track whose fill and handle follow the shared animation value.
pub(crate) fn tween(card: Card, ctx: &mut Ctx) -> Card {
    let theme = ctx.theme;
    let value = ctx.state.animation.clone();
    let accent = theme.palette().accent;
    let track = theme.surface(SurfaceLevel::Surface);

    let bar = Panel::new()
        .color(Color::TRANSPARENT)
        .flat()
        .min_size(0.0, 48.0)
        .grow(1.0)
        .foreground(move |ctx, rect, _state| {
            let t = value.get().clamp(0.0, 1.0);
            let y = rect.center().y;
            let track_rect = Rect::from_min_size(
                Vec2::new(rect.left(), y - 3.0),
                Size::new(rect.size.width, 6.0),
            );
            ctx.fill_rounded_rect(track_rect, 3.0, track);

            let x = rect.left() + rect.size.width * t;
            if x > track_rect.left() {
                let fill = Rect::from_min_max(track_rect.origin, Vec2::new(x, track_rect.bottom()));
                ctx.fill_rounded_rect(fill, 3.0, accent);
            }
            ctx.fill_circle(Vec2::new(x, y), 7.0, accent);
        });

    card.child(bar)
}

/// Four easing curves sampled against the same progress, each a dot sliding
/// along its own track.
pub(crate) fn easing(card: Card, ctx: &mut Ctx) -> Card {
    let theme = ctx.theme;
    let value = ctx.state.animation.clone();
    let accent = theme.palette().accent;
    let track = theme.surface(SurfaceLevel::Surface);
    let curves = [
        Easing::Linear,
        Easing::QuadInOut,
        Easing::CubicOut,
        Easing::BackOut,
    ];

    let plot = Panel::new()
        .color(Color::TRANSPARENT)
        .flat()
        .min_size(0.0, 80.0)
        .grow(1.0)
        .foreground(move |ctx, rect, _state| {
            let t = value.get().clamp(0.0, 1.0);
            let slot = rect.size.height / curves.len() as f32;
            for (index, curve) in curves.iter().enumerate() {
                let y = rect.top() + slot * (index as f32 + 0.5);
                let line = Rect::from_min_size(
                    Vec2::new(rect.left(), y - 2.0),
                    Size::new(rect.size.width, 4.0),
                );
                ctx.fill_rounded_rect(line, 2.0, track);
                let x = rect.left() + rect.size.width * curve.ease(t);
                ctx.fill_circle(Vec2::new(x, y), 5.0, accent);
            }
        });

    card.child(plot)
}
