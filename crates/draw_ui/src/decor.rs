//! Per-node decorations: chrome painted around a control and torn down with it.
//!
//! The core `Widget` enum stays closed, so components that need a surface /
//! foreground (or shared interaction state) install a [`NodeDecor`] on their
//! root node instead. `paint` runs `paint_behind` before the control's own
//! content and `paint_front` after it, in the same single pass — hosts do not
//! run separate surface/foreground passes.
//!
//! The factory functions at the bottom build the common decorators. They take
//! resolved colors (the closures capture what they need), so `draw_ui` never
//! reads a theme; `draw_components` keeps only the component builders.

use std::rc::Rc;

use draw_core::Rect;
use draw_render::PaintContext;

use crate::paint::{self, SurfaceStyle};

/// Hover/pressed/focused state of a control, resolved against its ancestors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct InteractState {
    pub hovered: bool,
    pub pressed: bool,
    pub focused: bool,
}

/// Chrome attached to one control node.
pub trait NodeDecor {
    /// Painted before the control's widget content.
    fn paint_behind(&self, _ctx: &mut PaintContext, _rect: Rect, _state: InteractState) {}

    /// Painted after the control's widget content.
    fn paint_front(&self, _ctx: &mut PaintContext, _rect: Rect, _state: InteractState) {}
}

/// A shared handle to a decorator, as stored by [`add_decor`](crate::add_decor).
pub type DecorRef = Rc<dyn NodeDecor>;

type SurfaceResolve = Box<dyn Fn(InteractState) -> SurfaceStyle>;
type ForegroundFn = Box<dyn Fn(&mut PaintContext, Rect, InteractState)>;

/// A fixed rounded surface painted behind a node.
struct StaticSurface(SurfaceStyle);

impl NodeDecor for StaticSurface {
    fn paint_behind(&self, ctx: &mut PaintContext, rect: Rect, _state: InteractState) {
        paint::surface(ctx, rect, &self.0);
    }
}

/// A surface whose style is recomputed from the interaction state.
struct DynamicSurface(SurfaceResolve);

impl NodeDecor for DynamicSurface {
    fn paint_behind(&self, ctx: &mut PaintContext, rect: Rect, state: InteractState) {
        let style = (self.0)(state);
        paint::surface(ctx, rect, &style);
    }
}

/// Chrome painted in front of a node's content.
struct Foreground(ForegroundFn);

impl NodeDecor for Foreground {
    fn paint_front(&self, ctx: &mut PaintContext, rect: Rect, state: InteractState) {
        (self.0)(ctx, rect, state);
    }
}

/// A fixed-surface decorator (no theme needed).
pub fn surface_decor(style: SurfaceStyle) -> DecorRef {
    Rc::new(StaticSurface(style))
}

/// A dynamic-surface decorator. The resolver receives the interaction state;
/// it captures whatever colors it needs when the component builds.
pub fn dynamic_surface_decor(
    resolve: impl Fn(InteractState) -> SurfaceStyle + 'static,
) -> DecorRef {
    Rc::new(DynamicSurface(Box::new(resolve)))
}

/// A foreground decorator. The closure captures whatever colors it needs when
/// the component builds.
pub fn foreground_decor(
    draw: impl Fn(&mut PaintContext, Rect, InteractState) + 'static,
) -> DecorRef {
    Rc::new(Foreground(Box::new(draw)))
}
