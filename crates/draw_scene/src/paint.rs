use draw_core::{Rect, Vec2};
use draw_render::{Paint, PaintContext};

use crate::node::Visual;
use crate::tree::SceneTree;

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

    /// Paints all visible canvas items into `ctx` in draw order.
    ///
    /// Each painted item is wrapped in `Save`/`Restore` and preceded by a
    /// `SetTransform` with its world transform, so the output is deterministic
    /// for a given scene.
    pub fn paint(&self, ctx: &mut PaintContext) {
        for id in self.iter_visible() {
            let Some(node) = self.get(id) else {
                continue;
            };
            let Some(canvas) = node.canvas() else {
                continue;
            };
            match canvas.visual() {
                Visual::None => {}
                Visual::Rect { size, color } => {
                    ctx.save();
                    ctx.set_transform(canvas.world_transform());
                    ctx.fill_rect(Rect::from_min_size(Vec2::ZERO, size), Paint::new(color));
                    ctx.restore();
                }
                Visual::Circle { radius, color } => {
                    ctx.save();
                    ctx.set_transform(canvas.world_transform());
                    ctx.fill_circle(Vec2::ZERO, radius, Paint::new(color));
                    ctx.restore();
                }
            }
        }
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
        let transform = commands
            .iter()
            .find_map(|c| match c {
                DrawCommand::SetTransform(t) => Some(*t),
                _ => None,
            })
            .unwrap();
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
        tree.set_visual(
            a,
            Visual::Rect {
                size: Size::splat(1.0),
                color: Color::RED,
            },
        );
        tree.set_visual(
            b,
            Visual::Rect {
                size: Size::splat(1.0),
                color: Color::GREEN,
            },
        );
        tree.update();
        assert_eq!(paint(&tree).len(), 8);

        tree.set_visible(a, false);
        tree.update();
        assert_eq!(paint(&tree).len(), 0);
    }
}
