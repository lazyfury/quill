//! Host-owned runner for [`SpriteFrames`] animations.
//!
//! Frame stepping is discrete, not interpolated, so it is a small dedicated
//! runner rather than a `draw_anim` tween: the host calls
//! [`SpriteAnimations::update`] in the same frame step as its `draw_anim`
//! `Animator` (that one drives transforms / colours, this one drives the atlas
//! region). [`SpriteAnimations::is_animating`] is part of a host's `needs_frame`
//! signal, like `Animator::is_animating`.

use draw_core::{NodeId, Rect};
use draw_scene::{SceneTree, Visual};

use crate::frames::SpriteFrames;

/// One playing sprite-sheet animation.
struct Playing {
    node: NodeId,
    frames: SpriteFrames,
    elapsed: f32,
    finished: bool,
}

/// Runs sprite-sheet animations over a [`SceneTree`].
#[derive(Default)]
pub struct SpriteAnimations {
    playing: Vec<Playing>,
}

impl SpriteAnimations {
    pub fn new() -> Self {
        Self::default()
    }

    /// Starts (or restarts) an animation on `node` and applies its first frame.
    ///
    /// `node` must already carry a [`Visual::Sprite`]; otherwise the animation
    /// runs but writes nothing until one is set.
    pub fn play(&mut self, tree: &mut SceneTree, node: NodeId, frames: SpriteFrames) {
        self.playing.retain(|playing| playing.node != node);
        if let Some(region) = frames.region_at(0.0) {
            apply_region(tree, node, region);
        }
        self.playing.push(Playing {
            node,
            frames,
            elapsed: 0.0,
            finished: false,
        });
    }

    /// Stops the animation on `node`. Returns whether one was running.
    pub fn stop(&mut self, node: NodeId) -> bool {
        let before = self.playing.len();
        self.playing.retain(|playing| playing.node != node);
        self.playing.len() != before
    }

    /// Stops every animation.
    pub fn stop_all(&mut self) {
        self.playing.clear();
    }

    /// Whether any animation is still running (a non-looping one stops at its
    /// last frame).
    pub fn is_animating(&self) -> bool {
        self.playing.iter().any(|playing| !playing.finished)
    }

    /// The current frame index for `node`, if it is animating.
    pub fn current_frame(&self, node: NodeId) -> Option<usize> {
        self.playing
            .iter()
            .find(|playing| playing.node == node)
            .map(|playing| playing.frames.index_at(playing.elapsed))
    }

    /// Advances every animation by `dt` and writes the current region.
    pub fn update(&mut self, dt: f32, tree: &mut SceneTree) {
        let dt = dt.max(0.0);
        for playing in &mut self.playing {
            if playing.finished {
                continue;
            }
            playing.elapsed += dt;
            let duration = playing.frames.duration();
            if duration <= 0.0 || playing.frames.is_empty() {
                playing.finished = true;
                continue;
            }
            if !playing.frames.is_looping() && playing.elapsed >= duration {
                playing.elapsed = duration;
                playing.finished = true;
            }
            if let Some(region) = playing.frames.region_at(playing.elapsed) {
                apply_region(tree, playing.node, region);
            }
        }
        // A node removed from the tree stops animating.
        self.playing.retain(|playing| tree.contains(playing.node));
    }
}

/// Replaces the `source` region of a node's `Visual::Sprite`, if it has one.
fn apply_region(tree: &mut SceneTree, node: NodeId, region: Rect) {
    let Some(Visual::Sprite {
        texture,
        size,
        flip_x,
        flip_y,
        nine,
        ..
    }) = tree.visual(node)
    else {
        return;
    };
    tree.set_visual(
        node,
        Visual::Sprite {
            texture,
            size,
            source: Some(region),
            flip_x,
            flip_y,
            nine,
        },
    );
}

#[cfg(test)]
mod tests {
    use draw_core::{Size, Vec2};
    use draw_render::TextureId;

    use super::*;
    use crate::Sprite;

    fn sheet() -> SpriteFrames {
        SpriteFrames::from_grid(
            Rect::from_min_size(Vec2::ZERO, Size::new(32.0, 8.0)),
            4,
            1,
            4,
        )
        .fps(4.0)
    }

    fn sprite_tree() -> (SceneTree, NodeId) {
        let mut tree = SceneTree::new();
        let node = tree.add_child(
            tree.root(),
            Sprite::new(TextureId::new(1), Size::splat(8.0)),
        );
        (tree, node)
    }

    fn source(tree: &SceneTree, node: NodeId) -> Option<Rect> {
        match tree.visual(node) {
            Some(Visual::Sprite { source, .. }) => source,
            _ => None,
        }
    }

    #[test]
    fn playing_applies_the_first_frame_then_advances() {
        let (mut tree, node) = sprite_tree();
        let mut animations = SpriteAnimations::new();
        animations.play(&mut tree, node, sheet());
        assert_eq!(
            source(&tree, node),
            Some(Rect::from_min_size(Vec2::ZERO, Size::new(8.0, 8.0)))
        );
        assert!(animations.is_animating());

        animations.update(0.3, &mut tree);
        assert_eq!(animations.current_frame(node), Some(1));
        assert_eq!(
            source(&tree, node),
            Some(Rect::from_min_size(
                Vec2::new(8.0, 0.0),
                Size::new(8.0, 8.0)
            ))
        );
    }

    #[test]
    fn a_non_looping_animation_finishes_on_its_last_frame() {
        let (mut tree, node) = sprite_tree();
        let mut animations = SpriteAnimations::new();
        animations.play(&mut tree, node, sheet().looping(false));

        animations.update(10.0, &mut tree);
        assert!(!animations.is_animating());
        assert_eq!(animations.current_frame(node), Some(3));
        assert_eq!(
            source(&tree, node),
            Some(Rect::from_min_size(
                Vec2::new(24.0, 0.0),
                Size::new(8.0, 8.0)
            ))
        );
    }

    #[test]
    fn stop_ends_a_running_animation() {
        let (mut tree, node) = sprite_tree();
        let mut animations = SpriteAnimations::new();
        animations.play(&mut tree, node, sheet());
        assert!(animations.stop(node));
        assert!(!animations.is_animating());
        assert!(!animations.stop(node));
    }

    #[test]
    fn removing_the_node_drops_its_animation() {
        let (mut tree, node) = sprite_tree();
        let mut animations = SpriteAnimations::new();
        animations.play(&mut tree, node, sheet());
        assert!(tree.remove(node));

        animations.update(0.1, &mut tree);
        assert!(!animations.is_animating());
        assert_eq!(animations.current_frame(node), None);
    }
}
