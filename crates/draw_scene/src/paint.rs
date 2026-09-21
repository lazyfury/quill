use draw_core::{NodeId, Rect, Transform2D, Vec2};
use draw_render::{Paint, PaintContext};

use crate::node::Visual;
use crate::tree::SceneTree;

/// A batch of canvas items sharing one canvas transform, painted as a group.
pub(crate) struct PaintGroup {
    /// `None` for the default world canvas (layer 0), `Some(layer_node)` for a
    /// `CanvasLayer` subtree.
    pub(crate) key: Option<NodeId>,
    pub(crate) layer: i32,
    pub(crate) transform: Transform2D,
    pub(crate) items: Vec<NodeId>,
}

impl SceneTree {
    /// Sets the built-in visual of a canvas item. Returns `false` for non-canvas
    /// nodes or unknown ids.
    pub fn set_visual(&mut self, id: draw_core::NodeId, visual: Visual) -> bool {
        if !self.contains(id) {
            return false;
        }
        let Some(canvas) = self.get_mut(id).and_then(|node| node.canvas.as_mut()) else {
            return false;
        };
        canvas.visual = visual;
        true
    }

    pub fn visual(&self, id: draw_core::NodeId) -> Option<Visual> {
        self.get(id).map(|node| node.visual())
    }

    /// Paints all visible canvas items into `ctx`, batched by canvas layer.
    ///
    /// Groups are emitted in ascending `CanvasLayer::layer` order (the default
    /// world canvas is layer `0`); ties keep tree order. Each item is wrapped
    /// in `Save`/`SetTransform(effective * world)`/`Restore`, where the
    /// effective transform is the item's canvas transform (root viewport
    /// camera, or its `CanvasLayer`'s final transform), so the output is
    /// deterministic for a given scene.
    pub fn paint(&self, ctx: &mut PaintContext) {
        for group in self.paint_groups() {
            for id in group.items {
                let Some(canvas) = self.get(id).and_then(|node| node.canvas()) else {
                    continue;
                };
                ctx.save();
                ctx.set_transform(group.transform * canvas.world_transform());
                match canvas.visual() {
                    Visual::None => {}
                    Visual::Rect { size, color } => {
                        ctx.fill_rect(Rect::from_min_size(Vec2::ZERO, size), Paint::new(color));
                    }
                    Visual::Circle { radius, color } => {
                        ctx.fill_circle(Vec2::ZERO, radius, Paint::new(color));
                    }
                    Visual::Image { texture, size } => {
                        ctx.draw_image(
                            texture,
                            Rect::from_min_size(Vec2::ZERO, size),
                            None,
                            Paint::default(),
                        );
                    }
                }
                ctx.restore();
            }
        }
    }

    /// Collects visible canvas items into layer-ordered groups.
    pub(crate) fn paint_groups(&self) -> Vec<PaintGroup> {
        let mut groups: Vec<PaintGroup> = Vec::new();
        for id in self.iter_visible() {
            let Some(node) = self.get(id) else {
                continue;
            };
            let Some(canvas) = node.canvas() else {
                continue;
            };
            if matches!(canvas.visual(), Visual::None) {
                continue;
            }
            let (key, layer) = match self.canvas_layer_of(id) {
                Some((layer_node, data)) => (Some(layer_node), data.layer),
                None => (None, 0),
            };
            let transform = self.canvas_transform_of(id);
            match groups.iter_mut().find(|group| group.key == key) {
                Some(group) => group.items.push(id),
                None => groups.push(PaintGroup {
                    key,
                    layer,
                    transform,
                    items: vec![id],
                }),
            }
        }
        // Stable: equal layers keep first-encounter (tree) order.
        groups.sort_by_key(|group| group.layer);
        groups
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::Visual;
    use draw_core::{Color, Size, Transform2D};
    use draw_render::DrawCommand;

    fn paint(tree: &SceneTree) -> Vec<DrawCommand> {
        let mut ctx = PaintContext::new();
        tree.paint(&mut ctx);
        ctx.into_draw_list().into_commands()
    }

    fn set_transforms(commands: &[DrawCommand]) -> Vec<Transform2D> {
        commands
            .iter()
            .filter_map(|c| match c {
                DrawCommand::SetTransform(t) => Some(*t),
                _ => None,
            })
            .collect()
    }

    fn rect(color: Color) -> Visual {
        Visual::Rect {
            size: Size::splat(2.0),
            color,
        }
    }

    #[test]
    fn scene_to_draw_list_is_deterministic() {
        let mut tree = SceneTree::new();
        let root = tree.root();
        let a = tree.add_node2d(root, "A");
        tree.set_position(a, Vec2::new(10.0, 20.0));
        tree.set_visual(
            a,
            Visual::Rect {
                size: Size::new(4.0, 5.0),
                color: Color::RED,
            },
        );
        tree.update();

        // One default-canvas group; each item is wrapped in
        // Save + SetTransform + fill + Restore.
        let expected = vec![
            DrawCommand::Save,
            DrawCommand::SetTransform(Transform2D::from_translation(Vec2::new(10.0, 20.0))),
            DrawCommand::FillRect {
                rect: Rect::from_min_size(Vec2::ZERO, Size::new(4.0, 5.0)),
                paint: Paint::new(Color::RED),
            },
            DrawCommand::Restore,
        ];

        assert_eq!(paint(&tree), expected);
        // repeated painting of the same scene is identical
        assert_eq!(paint(&tree), paint(&tree));
    }

    #[test]
    fn an_image_visual_emits_a_draw_image_under_the_node_transform() {
        let mut tree = SceneTree::new();
        let id = tree.add_node2d(tree.root(), "image");
        let texture = draw_render::TextureId::new(7);
        tree.set_visual(
            id,
            Visual::Image {
                texture,
                size: Size::new(4.0, 2.0),
            },
        );
        tree.set_transform(id, Transform2D::from_translation(Vec2::new(10.0, 20.0)));
        tree.update();

        assert_eq!(
            paint(&tree),
            vec![
                DrawCommand::Save,
                DrawCommand::SetTransform(Transform2D::from_translation(Vec2::new(10.0, 20.0))),
                DrawCommand::DrawImage {
                    texture,
                    destination: Rect::from_min_size(Vec2::ZERO, Size::new(4.0, 2.0)),
                    source: None,
                    paint: Paint::default(),
                },
                DrawCommand::Restore,
            ]
        );
    }

    #[test]
    fn nested_transforms_land_in_world_space() {
        let mut tree = SceneTree::new();
        let root = tree.root();
        let a = tree.add_node2d(root, "A");
        let b = tree.add_node2d(a, "B");
        tree.set_position(a, Vec2::new(10.0, 0.0));
        tree.set_rotation(a, std::f32::consts::FRAC_PI_2);
        tree.set_position(b, Vec2::new(5.0, 0.0));
        tree.set_visual(
            b,
            Visual::Circle {
                radius: 2.0,
                color: Color::BLUE,
            },
        );
        tree.update();

        let commands = paint(&tree);
        // Last SetTransform is the item's (the first is the group base).
        let transform = *set_transforms(&commands).last().unwrap();
        assert!((transform.origin - Vec2::new(10.0, 5.0)).length() < 1e-5);
        assert!(commands.iter().any(
            |c| matches!(c, DrawCommand::FillCircle { radius, .. } if (*radius - 2.0).abs() < 1e-5)
        ));
    }

    #[test]
    fn hidden_nodes_are_not_painted() {
        let mut tree = SceneTree::new();
        let root = tree.root();
        let a = tree.add_node2d(root, "A");
        let b = tree.add_node2d(a, "B");
        tree.set_visual(a, rect(Color::RED));
        tree.set_visual(b, rect(Color::GREEN));
        tree.update();
        // 4 commands per visible item.
        assert_eq!(paint(&tree).len(), 2 * 4);

        tree.set_visible(a, false);
        tree.update();
        assert_eq!(paint(&tree).len(), 0);
    }

    #[test]
    fn camera_transform_is_applied_to_world_paint() {
        let mut tree = SceneTree::new();
        tree.set_viewport_size(Size::new(100.0, 100.0));
        let root = tree.root();
        let camera = tree.add_camera_2d(root, "Camera");
        tree.set_camera_current(camera, true);
        tree.set_position(camera, Vec2::new(50.0, 50.0));
        let a = tree.add_node2d(root, "A");
        tree.set_position(a, Vec2::new(50.0, 50.0));
        tree.set_visual(a, rect(Color::RED));
        tree.update();

        let first = paint(&tree);
        let t = *set_transforms(&first).last().unwrap();
        // World (50,50) is the camera center, so it lands at screen (50,50).
        assert!((t.transform_point(Vec2::ZERO) - Vec2::new(50.0, 50.0)).length() < 1e-5);

        // Moving the camera changes the emitted world transform.
        tree.set_position(camera, Vec2::new(0.0, 0.0));
        tree.update();
        let second = paint(&tree);
        let t2 = *set_transforms(&second).last().unwrap();
        assert!((t2.transform_point(Vec2::ZERO) - Vec2::new(100.0, 100.0)).length() < 1e-5);
    }

    #[test]
    fn canvas_layer_ignores_camera() {
        let mut tree = SceneTree::new();
        tree.set_viewport_size(Size::new(100.0, 100.0));
        let root = tree.root();
        let camera = tree.add_camera_2d(root, "Camera");
        tree.set_camera_current(camera, true);
        tree.set_position(camera, Vec2::new(50.0, 50.0));

        let world = tree.add_node2d(root, "World");
        tree.set_position(world, Vec2::new(10.0, 10.0));
        tree.set_visual(world, rect(Color::RED));

        let layer = tree.add_canvas_layer(root, "Ui");
        let ui = tree.add_control(layer, "UiNode");
        tree.set_position(ui, Vec2::new(5.0, 5.0));
        tree.set_visual(ui, rect(Color::BLUE));

        tree.update();
        let before = set_transforms(&paint(&tree));
        // One item transform per canvas: world then UI.
        assert_eq!(before.len(), 2);
        // The UI item transform is layer * world = (5,5); it is not affected
        // by the camera.
        assert!((before[1].origin - Vec2::new(5.0, 5.0)).length() < 1e-5);

        // Move the camera: the world item changes, the UI item does not.
        tree.set_position(camera, Vec2::new(0.0, 0.0));
        tree.update();
        let after = set_transforms(&paint(&tree));
        assert_eq!(after.len(), 2);
        assert_ne!(before[0], after[0]);
        assert_eq!(before[1], after[1]);
    }

    #[test]
    fn layers_paint_in_ascending_order() {
        let mut tree = SceneTree::new();
        let root = tree.root();

        let base = tree.add_node2d(root, "Base");
        tree.set_visual(base, rect(Color::RED));

        let high = tree.add_canvas_layer(root, "High");
        tree.set_canvas_layer(high, 10);
        let high_node = tree.add_node2d(high, "HighNode");
        tree.set_visual(high_node, rect(Color::GREEN));

        let low = tree.add_canvas_layer(root, "Low");
        tree.set_canvas_layer(low, -5);
        let low_node = tree.add_node2d(low, "LowNode");
        tree.set_visual(low_node, rect(Color::BLUE));

        tree.update();
        let fills: Vec<Color> = paint(&tree)
            .iter()
            .filter_map(|c| match c {
                DrawCommand::FillRect { paint, .. } => Some(paint.color),
                _ => None,
            })
            .collect();
        // layer -5, then default 0, then layer 10.
        assert_eq!(fills, vec![Color::BLUE, Color::RED, Color::GREEN]);
    }

    #[test]
    fn follow_viewport_composes_the_camera() {
        let mut tree = SceneTree::new();
        tree.set_viewport_size(Size::new(100.0, 100.0));
        let root = tree.root();
        let camera = tree.add_camera_2d(root, "Camera");
        tree.set_camera_current(camera, true);
        tree.set_position(camera, Vec2::new(0.0, 0.0));

        let layer = tree.add_canvas_layer(root, "Parallax");
        tree.set_canvas_layer(layer, 2);
        tree.set_canvas_layer_follow_viewport(layer, true);
        let node = tree.add_node2d(layer, "Node");
        tree.set_position(node, Vec2::new(5.0, 5.0));
        tree.set_visual(node, rect(Color::RED));
        tree.update();

        // Camera at (0,0) drag-center on a 100x100 viewport maps world ->
        // world + (50,50). `follow_viewport` composes that camera transform
        // before the layer transform, so the item lands where the world canvas
        // would put it; without follow it would stay at layer coordinates.
        let transforms = set_transforms(&paint(&tree));
        let expected_world = tree.world_to_screen(Vec2::new(5.0, 5.0));
        assert!((transforms.last().unwrap().origin - expected_world).length() < 1e-5);
    }
}
