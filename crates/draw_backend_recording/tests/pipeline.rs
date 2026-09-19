//! End-to-end headless pipeline test: `Scene -> DrawList -> RecordingBackend`.

use draw_backend_recording::{CommandAsserts, RecordingBackend};
use draw_core::{Color, Size, Transform2D, Vec2, Viewport};
use draw_render::{DrawCommand, Paint, PaintContext, RenderBackend};
use draw_scene::{SceneTree, Visual};

fn build_scene() -> SceneTree {
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
    tree
}

#[test]
fn scene_to_recording_backend_pipeline() {
    let tree = build_scene();

    let mut ctx = PaintContext::new();
    ctx.set_opacity(0.75);
    tree.paint(&mut ctx);
    let list = ctx.into_draw_list();

    // B world = A_local(T(10,0) * R(90deg)) * B_local(T(5,0))
    let expected_transform = Transform2D::from_scale_rotation_origin(
        Vec2::splat(1.0),
        std::f32::consts::FRAC_PI_2,
        Vec2::new(10.0, 0.0),
    ) * Transform2D::from_translation(Vec2::new(5.0, 0.0));

    let mut backend = RecordingBackend::new();
    let viewport = Viewport::new(Size::new(800.0, 600.0));
    backend.begin_frame(viewport).unwrap();
    backend.submit(&list).unwrap();
    backend.end_frame().unwrap();

    assert_eq!(backend.frame_count(), 1);
    let frame = backend.last_frame().unwrap();
    assert_eq!(frame.viewport, viewport);

    frame.commands().assert_sequence(&[
        DrawCommand::SetOpacity(0.75),
        DrawCommand::Save,
        DrawCommand::SetTransform(expected_transform),
        DrawCommand::FillCircle {
            center: Vec2::ZERO,
            radius: 2.0,
            paint: Paint::new(Color::BLUE),
        },
        DrawCommand::Restore,
    ]);
    frame.commands().assert_last_opacity(0.75);
    frame.commands().assert_last_transform(expected_transform);
}

#[test]
fn visibility_gates_recording_across_frames() {
    let mut tree = build_scene();
    let mut backend = RecordingBackend::new();
    let viewport = Viewport::new(Size::new(100.0, 100.0));

    // frame 0: visible, 1 circle emitted
    let mut ctx = PaintContext::new();
    tree.paint(&mut ctx);
    backend.begin_frame(viewport).unwrap();
    backend.submit(&ctx.into_draw_list()).unwrap();
    backend.end_frame().unwrap();
    assert_eq!(backend.frame(0).unwrap().command_count(), 4);

    // frame 1: hide the child, nothing is painted
    let child = tree.iter().find(|id| tree.node(*id).name() == "B").unwrap();
    tree.set_visible(child, false);
    tree.update();
    let mut ctx = PaintContext::new();
    tree.paint(&mut ctx);
    backend.begin_frame(viewport).unwrap();
    backend.submit(&ctx.into_draw_list()).unwrap();
    backend.end_frame().unwrap();
    assert_eq!(backend.frame(1).unwrap().command_count(), 0);
}
