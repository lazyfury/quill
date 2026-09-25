//! `draw_anim` — backend-neutral, time-driven animation for quill.
//!
//! A host measures time and calls [`Animator::update(dt, tree)`](Animator::update);
//! the animator advances its tweens and writes the interpolated values either to
//! external state through a closure or to a [`draw_scene::SceneTree`] node
//! property. [`Animator::is_animating`] is the "needs another frame" signal that
//! lets a host sleep (`ControlFlow::Wait`) while nothing moves and wake while a
//! tween is in flight.
//!
//! ```ignore
//! use draw_anim::{Animator, TweenSpec};
//! use draw_core::Vec2;
//!
//! let mut anim = Animator::new();
//! anim.tween_position(actor, Vec2::new(120.0, 0.0), TweenSpec::new(0.3));
//!
//! // each frame
//! anim.update(dt, &mut tree);
//! if anim.is_animating() { window.request_redraw(); }
//! ```
//!
//! # Scope
//!
//! This crate is deliberately small and pure: no clock, no threads, no backend,
//! no UI dependency. It depends on `draw_core` for value types and on
//! `draw_scene` so node-property tweens can target a `SceneTree`; a value-only
//! user still passes a tree (hosts always have one). Sequencing, animation
//! clips/keyframes and UI-widget animation are later work.

/// Crate name, kept for lightweight smoke checks.
pub const CRATE: &str = "draw_anim";

mod animator;
mod easing;

pub use animator::{Animatable, Animator, Repeat, TweenId, TweenSpec};
pub use easing::Easing;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_identity() {
        assert_eq!(CRATE, "draw_anim");
    }
}
