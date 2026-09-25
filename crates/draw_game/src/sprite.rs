//! `Sprite`: a `Node2D` carrying a sprite visual.

use draw_core::{NodeId, Rect, Size, Vec2};
use draw_render::TextureId;
use draw_scene::{SceneChild, SceneTree, Visual};

/// A 2D sprite component.
///
/// Mounts a [`draw_scene::NodeKind::Node2D`] with a [`Visual::Sprite`] under a
/// parent. It composes with the rest of the scene (`Node2D` transforms, camera,
/// `CanvasLayer`) for free, so `SceneTree::paint` draws it with no extra pass.
///
/// ```ignore
/// let player = tree.add_child(tree.root(), Sprite::new(texture, Size::splat(64.0)));
/// ```
#[derive(Debug, Clone)]
pub struct Sprite {
    name: String,
    texture: TextureId,
    size: Size,
    source: Option<Rect>,
    flip_x: bool,
    flip_y: bool,
    nine: Option<[f32; 4]>,
    position: Vec2,
    z_index: i32,
}

impl Sprite {
    /// A whole-texture sprite of `size` logical pixels, at the origin.
    pub fn new(texture: TextureId, size: Size) -> Self {
        Self {
            name: "Sprite".to_string(),
            texture,
            size,
            source: None,
            flip_x: false,
            flip_y: false,
            nine: None,
            position: Vec2::ZERO,
            z_index: 0,
        }
    }

    /// Samples an atlas sub-rectangle instead of the whole texture.
    pub fn region(mut self, source: Rect) -> Self {
        self.source = Some(source);
        self
    }

    /// Mirrors the sprite horizontally.
    pub fn flip_x(mut self, flip: bool) -> Self {
        self.flip_x = flip;
        self
    }

    /// Mirrors the sprite vertically.
    pub fn flip_y(mut self, flip: bool) -> Self {
        self.flip_y = flip;
        self
    }

    /// Nine-slice insets `[left, top, right, bottom]` in source pixels. Requires
    /// a [`Sprite::region`]; the edges/centre stretch to `size`.
    pub fn nine_slice(mut self, insets: [f32; 4]) -> Self {
        self.nine = Some(insets);
        self
    }

    /// Local position relative to the parent.
    pub fn position(mut self, position: Vec2) -> Self {
        self.position = position;
        self
    }

    /// Paint order within the parent (higher draws later).
    pub fn z_index(mut self, z_index: i32) -> Self {
        self.z_index = z_index;
        self
    }

    /// Node name (for debugging / lookup).
    pub fn named(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    /// The scene visual this sprite mounts.
    pub fn visual(&self) -> Visual {
        Visual::Sprite {
            texture: self.texture,
            size: self.size,
            source: self.source,
            flip_x: self.flip_x,
            flip_y: self.flip_y,
            nine: self.nine,
        }
    }
}

impl SceneChild for Sprite {
    fn attach(self, tree: &mut SceneTree, parent: NodeId) -> NodeId {
        let visual = self.visual();
        let id = tree.add_node2d(parent, self.name);
        tree.set_visual(id, visual);
        tree.set_position(id, self.position);
        tree.set_z_index(id, self.z_index);
        id
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use draw_render::PaintContext;

    #[test]
    fn a_sprite_mounts_a_node2d_with_a_sprite_visual() {
        let mut tree = SceneTree::new();
        let texture = TextureId::new(4);
        let region = Rect::from_min_size(Vec2::new(8.0, 8.0), Size::splat(16.0));
        let id = tree.add_child(
            tree.root(),
            Sprite::new(texture, Size::splat(32.0))
                .region(region)
                .flip_x(true)
                .position(Vec2::new(5.0, 6.0))
                .z_index(3)
                .named("player"),
        );

        assert_eq!(tree.node(id).name(), "player");
        assert_eq!(tree.position(id), Some(Vec2::new(5.0, 6.0)));
        assert_eq!(tree.z_index(id), Some(3));
        assert_eq!(
            tree.visual(id),
            Some(Visual::Sprite {
                texture,
                size: Size::splat(32.0),
                source: Some(region),
                flip_x: true,
                flip_y: false,
                nine: None,
            })
        );
    }

    #[test]
    fn a_mounted_sprite_paints_under_the_scene_pipeline() {
        let mut tree = SceneTree::new();
        let texture = TextureId::new(9);
        tree.add_child(
            tree.root(),
            Sprite::new(texture, Size::splat(24.0)).position(Vec2::new(10.0, 0.0)),
        );
        tree.update();

        let mut ctx = PaintContext::new();
        tree.paint(&mut ctx);
        let list = ctx.into_draw_list();
        assert!(list.commands().iter().any(|command| matches!(
            command,
            draw_render::DrawCommand::DrawImage { texture: t, .. } if *t == texture
        )));
    }
}
