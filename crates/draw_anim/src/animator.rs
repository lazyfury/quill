//! The [`Animator`]: a small time-driven tween manager.
//!
//! An `Animator` owns a set of active tweens and advances them by a host-supplied
//! `dt`. Each tween interpolates a value over a [`TweenSpec`] (duration, delay,
//! [`Easing`], repeat) and writes it either through a caller closure (external
//! state such as an `Rc<Cell<_>>`) or to a node property of the [`SceneTree`].
//!
//! It is deliberately backend- and clock-neutral: the host measures time and
//! calls [`Animator::update`]. [`Animator::is_animating`] is the signal a host
//! uses to keep requesting frames while motion is in flight.

use draw_core::{Color, NodeId, Vec2};
use draw_scene::SceneTree;

use crate::easing::Easing;

/// Stable handle for a running tween, returned by the `tween_*` methods.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TweenId(u64);

impl TweenId {
    /// The underlying monotonic id.
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// How many times a tween plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Repeat {
    /// Play once, then complete (the default).
    #[default]
    Once,
    /// Play `n` times, then complete. `Times(0)` completes immediately.
    Times(u32),
    /// Play forever; never completes on its own.
    Forever,
    /// Alternate direction forever (forward, backward, forward, ...).
    PingPong,
}

/// Duration, delay, curve and repeat policy for one tween.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TweenSpec {
    /// Length of a single iteration, in seconds.
    pub duration: f32,
    /// Seconds to wait before the first application.
    pub delay: f32,
    /// Curve applied to normalized iteration time.
    pub easing: Easing,
    /// Repeat policy.
    pub repeat: Repeat,
}

impl TweenSpec {
    /// A once-off tween of `duration` seconds with linear easing.
    pub fn new(duration: f32) -> Self {
        Self {
            duration: duration.max(0.0),
            delay: 0.0,
            easing: Easing::Linear,
            repeat: Repeat::Once,
        }
    }

    /// Waits `seconds` before starting.
    pub fn delay(mut self, seconds: f32) -> Self {
        self.delay = seconds.max(0.0);
        self
    }

    /// Overrides the easing curve.
    pub fn easing(mut self, easing: Easing) -> Self {
        self.easing = easing;
        self
    }

    /// Overrides the repeat policy.
    pub fn repeat(mut self, repeat: Repeat) -> Self {
        self.repeat = repeat;
        self
    }
}

impl Default for TweenSpec {
    fn default() -> Self {
        Self::new(0.25)
    }
}

/// A value a [`Animator::tween`] can interpolate.
pub trait Animatable: Copy + 'static {
    /// Interpolates from `a` to `b` at normalized `t` (`0.0..=1.0`).
    fn interpolate(a: Self, b: Self, t: f32) -> Self;
}

impl Animatable for f32 {
    fn interpolate(a: Self, b: Self, t: f32) -> Self {
        a + (b - a) * t
    }
}

impl Animatable for Vec2 {
    fn interpolate(a: Self, b: Self, t: f32) -> Self {
        a.lerp(b, t)
    }
}

impl Animatable for Color {
    fn interpolate(a: Self, b: Self, t: f32) -> Self {
        a.lerp(b, t)
    }
}

/// One active tween.
struct Active {
    id: TweenId,
    elapsed: f32,
    spec: TweenSpec,
    binding: Binding,
    on_complete: Option<Box<dyn FnMut()>>,
    finished: bool,
}

impl Active {
    fn apply(&mut self, progress: f32, tree: &mut SceneTree) {
        let eased = self.spec.easing.ease(progress);
        match &mut self.binding {
            Binding::Value(apply) => apply(eased),
            Binding::Node(binding) => binding.apply(eased, tree),
        }
    }

    fn finish(&mut self, completed: &mut Vec<Box<dyn FnMut()>>) {
        self.finished = true;
        if let Some(callback) = self.on_complete.take() {
            completed.push(callback);
        }
    }
}

/// What a tween writes each frame.
enum Binding {
    /// A closure receiving the eased value directly (already interpolated).
    Value(Box<dyn FnMut(f32)>),
    /// A node property, read once on first application as the "from" value.
    Node(NodeBinding),
}

/// A `SceneTree` property target.
enum NodeBinding {
    Position {
        id: NodeId,
        from: Option<Vec2>,
        to: Vec2,
    },
    Rotation {
        id: NodeId,
        from: Option<f32>,
        to: f32,
    },
    Scale {
        id: NodeId,
        from: Option<Vec2>,
        to: Vec2,
    },
    ZIndex {
        id: NodeId,
        from: Option<i32>,
        to: i32,
    },
}

impl NodeBinding {
    fn apply(&mut self, eased: f32, tree: &mut SceneTree) {
        match self {
            NodeBinding::Position { id, from, to } => {
                let start = *from.get_or_insert_with(|| tree.position(*id).unwrap_or(Vec2::ZERO));
                tree.set_position(*id, start.lerp(*to, eased));
            }
            NodeBinding::Rotation { id, from, to } => {
                let start = *from.get_or_insert_with(|| tree.rotation(*id).unwrap_or(0.0));
                tree.set_rotation(*id, start + (*to - start) * eased);
            }
            NodeBinding::Scale { id, from, to } => {
                let start = *from.get_or_insert_with(|| tree.scale(*id).unwrap_or(Vec2::ONE));
                tree.set_scale(*id, start.lerp(*to, eased));
            }
            NodeBinding::ZIndex { id, from, to } => {
                let start = *from.get_or_insert_with(|| tree.z_index(*id).unwrap_or(0));
                let value = start as f32 + (*to - start) as f32 * eased;
                tree.set_z_index(*id, value.round() as i32);
            }
        }
    }
}

/// Owns and advances active tweens.
///
/// ```ignore
/// let mut anim = Animator::new();
/// anim.tween_position(node, Vec2::new(120.0, 0.0), TweenSpec::new(0.3));
///
/// // each frame:
/// anim.update(dt, &mut tree);
/// if anim.is_animating() { /* request another frame */ }
/// ```
pub struct Animator {
    active: Vec<Active>,
    next_id: u64,
}

impl Default for Animator {
    fn default() -> Self {
        Self::new()
    }
}

impl Animator {
    /// Creates an empty animator.
    pub fn new() -> Self {
        Self {
            active: Vec::new(),
            next_id: 1,
        }
    }
    /// Whether any tween is still running. Hosts use this as a "needs another
    /// frame" signal.
    pub fn is_animating(&self) -> bool {
        self.active.iter().any(|tween| !tween.finished)
    }

    /// Number of running tweens.
    pub fn active_count(&self) -> usize {
        self.active.iter().filter(|tween| !tween.finished).count()
    }

    /// Removes every tween (running or finishing) without firing callbacks.
    pub fn clear(&mut self) {
        self.active.clear();
    }

    /// Cancels a single tween. Returns whether it was present.
    pub fn kill(&mut self, id: TweenId) -> bool {
        let before = self.active.len();
        self.active.retain(|tween| tween.id != id);
        self.active.len() != before
    }

    /// Attaches a completion callback to a running tween. Returns whether the
    /// tween was found.
    pub fn on_complete(&mut self, id: TweenId, callback: impl FnMut() + 'static) -> bool {
        if let Some(tween) = self.active.iter_mut().find(|tween| tween.id == id) {
            tween.on_complete = Some(Box::new(callback));
            true
        } else {
            false
        }
    }

    /// Advances every tween by `dt` and writes node-property targets into `tree`.
    pub fn update(&mut self, dt: f32, tree: &mut SceneTree) {
        if self.active.is_empty() {
            return;
        }
        let dt = dt.max(0.0);
        let mut completed: Vec<Box<dyn FnMut()>> = Vec::new();

        for tween in &mut self.active {
            if tween.finished {
                continue;
            }
            tween.elapsed += dt;
            if tween.elapsed < tween.spec.delay {
                continue;
            }
            let local = tween.elapsed - tween.spec.delay;
            let duration = tween.spec.duration.max(0.0);

            if duration <= f32::EPSILON {
                tween.apply(1.0, tree);
                tween.finish(&mut completed);
                continue;
            }

            let (progress, done) = match tween.spec.repeat {
                Repeat::Once => {
                    let progress = (local / duration).clamp(0.0, 1.0);
                    (progress, local >= duration)
                }
                Repeat::Times(count) => {
                    let total = duration * count as f32;
                    if local >= total {
                        (1.0, true)
                    } else {
                        (iteration_progress(local, duration), false)
                    }
                }
                Repeat::Forever => (iteration_progress(local, duration), false),
                Repeat::PingPong => {
                    let iteration = (local / duration).floor();
                    let within = (local % duration) / duration;
                    let progress = if (iteration as i64) % 2 == 0 {
                        within
                    } else {
                        1.0 - within
                    };
                    (progress, false)
                }
            };

            tween.apply(progress, tree);
            if done {
                tween.finish(&mut completed);
            }
        }

        for mut callback in completed {
            callback();
        }
        self.active.retain(|tween| !tween.finished);
    }

    /// Tweens a generic value, delivering each interpolated value to `apply`.
    pub fn tween<T: Animatable>(
        &mut self,
        from: T,
        to: T,
        spec: TweenSpec,
        mut apply: impl FnMut(T) + 'static,
    ) -> TweenId {
        let binding = Binding::Value(Box::new(move |t| apply(T::interpolate(from, to, t))));
        self.push(binding, spec)
    }

    /// Tweens a node's local position from its current value to `to`.
    pub fn tween_position(&mut self, id: NodeId, to: Vec2, spec: TweenSpec) -> TweenId {
        self.push(
            Binding::Node(NodeBinding::Position { id, from: None, to }),
            spec,
        )
    }

    /// Tweens a node's local rotation (radians) from its current value to `to`.
    pub fn tween_rotation(&mut self, id: NodeId, to: f32, spec: TweenSpec) -> TweenId {
        self.push(
            Binding::Node(NodeBinding::Rotation { id, from: None, to }),
            spec,
        )
    }

    /// Tweens a node's local scale from its current value to `to`.
    pub fn tween_scale(&mut self, id: NodeId, to: Vec2, spec: TweenSpec) -> TweenId {
        self.push(
            Binding::Node(NodeBinding::Scale { id, from: None, to }),
            spec,
        )
    }

    /// Tweens a node's z-index (rounded) from its current value to `to`.
    pub fn tween_z_index(&mut self, id: NodeId, to: i32, spec: TweenSpec) -> TweenId {
        self.push(
            Binding::Node(NodeBinding::ZIndex { id, from: None, to }),
            spec,
        )
    }

    fn push(&mut self, binding: Binding, spec: TweenSpec) -> TweenId {
        let id = TweenId(self.next_id);
        self.next_id += 1;
        self.active.push(Active {
            id,
            elapsed: 0.0,
            spec,
            binding,
            on_complete: None,
            finished: false,
        });
        id
    }
}

/// Progress within the current iteration for looping tweens.
fn iteration_progress(local: f32, duration: f32) -> f32 {
    (local % duration) / duration
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use super::*;

    fn tree_with_node() -> (SceneTree, NodeId) {
        let mut tree = SceneTree::new();
        let node = tree.add_node2d(tree.root(), "actor");
        (tree, node)
    }

    #[test]
    fn linear_value_tween_interpolates_and_completes() {
        let mut tree = SceneTree::new();
        let value = Rc::new(Cell::new(0.0));
        let sink = value.clone();
        let mut anim = Animator::new();
        anim.tween(0.0_f32, 10.0, TweenSpec::new(1.0), move |v| sink.set(v));

        assert!(anim.is_animating());
        anim.update(0.5, &mut tree);
        assert!((value.get() - 5.0).abs() < 1e-4);
        assert!(anim.is_animating());

        anim.update(0.5, &mut tree);
        assert!((value.get() - 10.0).abs() < 1e-4);
        assert!(!anim.is_animating());
        assert_eq!(anim.active_count(), 0);
    }

    #[test]
    fn delay_defers_the_first_application() {
        let mut tree = SceneTree::new();
        let value = Rc::new(Cell::new(-1.0));
        let sink = value.clone();
        let mut anim = Animator::new();
        anim.tween(0.0_f32, 4.0, TweenSpec::new(1.0).delay(1.0), move |v| {
            sink.set(v)
        });

        anim.update(0.5, &mut tree);
        assert_eq!(value.get(), -1.0, "still waiting out the delay");

        anim.update(0.5, &mut tree);
        assert_eq!(value.get(), 0.0, "delay elapsed, at the start of the curve");

        anim.update(0.5, &mut tree);
        assert!((value.get() - 2.0).abs() < 1e-4);
    }

    #[test]
    fn zero_duration_applies_the_final_value_immediately() {
        let mut tree = SceneTree::new();
        let value = Rc::new(Cell::new(0.0));
        let sink = value.clone();
        let mut anim = Animator::new();
        anim.tween(0.0_f32, 1.0, TweenSpec::new(0.0), move |v| sink.set(v));

        anim.update(0.016, &mut tree);
        assert_eq!(value.get(), 1.0);
        assert!(!anim.is_animating());
    }

    #[test]
    fn times_plays_exactly_n_iterations() {
        let mut tree = SceneTree::new();
        let value = Rc::new(Cell::new(0.0));
        let sink = value.clone();
        let mut anim = Animator::new();
        anim.tween(
            0.0_f32,
            1.0,
            TweenSpec::new(1.0).repeat(Repeat::Times(3)),
            move |v| sink.set(v),
        );

        anim.update(1.0, &mut tree);
        assert!(anim.is_animating(), "second iteration still pending");
        anim.update(1.0, &mut tree);
        assert!(anim.is_animating(), "third iteration still pending");
        anim.update(1.0, &mut tree);
        assert!(!anim.is_animating());
        assert!((value.get() - 1.0).abs() < 1e-4);
    }

    #[test]
    fn forever_never_completes() {
        let mut tree = SceneTree::new();
        let value = Rc::new(Cell::new(0.0));
        let sink = value.clone();
        let mut anim = Animator::new();
        anim.tween(
            0.0_f32,
            1.0,
            TweenSpec::new(1.0).repeat(Repeat::Forever),
            move |v| sink.set(v),
        );

        for _ in 0..10 {
            anim.update(1.0, &mut tree);
        }
        assert!(anim.is_animating());
        assert_eq!(anim.active_count(), 1);
    }

    #[test]
    fn ping_pong_reverses_direction_each_iteration() {
        let mut tree = SceneTree::new();
        let value = Rc::new(Cell::new(0.0));
        let sink = value.clone();
        let mut anim = Animator::new();
        anim.tween(
            0.0_f32,
            1.0,
            TweenSpec::new(1.0).repeat(Repeat::PingPong),
            move |v| sink.set(v),
        );

        anim.update(0.5, &mut tree);
        assert!((value.get() - 0.5).abs() < 1e-4, "forward half");
        anim.update(1.0, &mut tree);
        assert!((value.get() - 0.5).abs() < 1e-4, "backward half");
    }

    #[test]
    fn node_position_tween_captures_the_current_value_as_from() {
        let (mut tree, node) = tree_with_node();
        tree.set_position(node, Vec2::new(10.0, 0.0));
        let mut anim = Animator::new();
        anim.tween_position(node, Vec2::new(30.0, 0.0), TweenSpec::new(1.0));

        anim.update(0.5, &mut tree);
        let p = tree.position(node).unwrap();
        assert!((p.x - 20.0).abs() < 1e-4, "halfway from captured 10 to 30");
        anim.update(0.5, &mut tree);
        assert_eq!(tree.position(node).unwrap(), Vec2::new(30.0, 0.0));
    }

    #[test]
    fn node_rotation_and_scale_tweens_write_the_tree() {
        let (mut tree, node) = tree_with_node();
        let mut anim = Animator::new();
        anim.tween_rotation(node, std::f32::consts::PI, TweenSpec::new(1.0));
        anim.tween_scale(node, Vec2::new(2.0, 2.0), TweenSpec::new(1.0));

        anim.update(0.5, &mut tree);
        assert!((tree.rotation(node).unwrap() - std::f32::consts::FRAC_PI_2).abs() < 1e-4);
        assert_eq!(tree.scale(node).unwrap(), Vec2::new(1.5, 1.5));
    }

    #[test]
    fn z_index_tween_rounds_to_integers() {
        let (mut tree, node) = tree_with_node();
        let mut anim = Animator::new();
        anim.tween_z_index(node, 10, TweenSpec::new(1.0));

        anim.update(0.95, &mut tree);
        assert_eq!(tree.z_index(node).unwrap(), 10);
    }

    #[test]
    fn kill_stops_a_running_tween() {
        let mut anim = Animator::new();
        let id = anim.tween(0.0_f32, 1.0, TweenSpec::new(1.0), |_| {});
        assert!(anim.kill(id));
        assert!(!anim.is_animating());
        assert!(!anim.kill(id), "already gone");
    }

    #[test]
    fn on_complete_fires_exactly_once() {
        let mut tree = SceneTree::new();
        let hits = Rc::new(Cell::new(0));
        let counter = hits.clone();
        let mut anim = Animator::new();
        let id = anim.tween(0.0_f32, 1.0, TweenSpec::new(1.0), |_| {});
        assert!(anim.on_complete(id, move || counter.set(counter.get() + 1)));

        anim.update(1.0, &mut tree);
        anim.update(1.0, &mut tree);
        assert_eq!(hits.get(), 1);
    }

    #[test]
    fn easing_is_applied_to_the_progress() {
        let mut tree = SceneTree::new();
        let value = Rc::new(Cell::new(0.0));
        let sink = value.clone();
        let mut anim = Animator::new();
        anim.tween(
            0.0_f32,
            1.0,
            TweenSpec::new(1.0).easing(Easing::QuadIn),
            move |v| sink.set(v),
        );

        anim.update(0.5, &mut tree);
        assert!((value.get() - 0.25).abs() < 1e-4, "0.5^2");
    }
}
