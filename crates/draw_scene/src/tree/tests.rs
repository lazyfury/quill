use super::*;
use crate::node::{AnchorMode, CanvasLayerData, NodeKind};
use draw_core::{Transform2D, Vec2};
use std::f32::consts::FRAC_PI_2;

const EPS: f32 = 1e-5;

fn approx(a: Vec2, b: Vec2) -> bool {
    (a - b).length() < EPS
}

fn names(tree: &SceneTree) -> Vec<String> {
    tree.iter()
        .map(|id| tree.node(id).name().to_string())
        .collect()
}

#[test]
fn root_and_children() {
    let mut tree = SceneTree::new();
    let root = tree.root();
    assert_eq!(tree.node(root).name(), "root");
    assert_eq!(tree.node(root).kind(), NodeKind::Viewport);
    assert!(tree.node(root).viewport().is_some());
    assert!(!tree.node(root).is_canvas_item());
    assert_eq!(tree.node_count(), 1);

    let a = tree.add_node2d(root, "A");
    let b = tree.add_node(root, "B");
    assert_eq!(tree.parent(a), Some(root));
    assert_eq!(tree.children(root), Some(&[a, b][..]));
    assert!(tree.node(a).is_canvas_item());
    assert!(!tree.node(b).is_canvas_item());
    assert_eq!(tree.node_count(), 3);
}

#[test]
fn world_transform_propagation() {
    let mut tree = SceneTree::new();
    let root = tree.root();
    let a = tree.add_node2d(root, "A");
    let b = tree.add_node2d(a, "B");

    // B local = translate(5, 0)
    assert!(tree.set_position(b, Vec2::new(5.0, 0.0)));
    // A local = rotate(90deg) then translate(10, 0)
    assert!(tree.set_position(a, Vec2::new(10.0, 0.0)));
    assert!(tree.set_rotation(a, FRAC_PI_2));

    tree.update();

    let world_b = tree.world_transform(b).unwrap();
    // rotate (5,0) -> (0,5), then translate by (10,0) -> (10,5)
    assert!(
        approx(world_b.origin, Vec2::new(10.0, 5.0)),
        "{:?}",
        world_b.origin
    );
    assert!(approx(
        tree.world_transform(a).unwrap().origin,
        Vec2::new(10.0, 0.0)
    ));
    assert!(approx(tree.position(b).unwrap(), Vec2::new(5.0, 0.0)));
}

#[test]
fn changing_parent_updates_children() {
    let mut tree = SceneTree::new();
    let root = tree.root();
    let a = tree.add_node2d(root, "A");
    let b = tree.add_node2d(a, "B");
    tree.set_position(a, Vec2::new(1.0, 2.0));
    tree.set_position(b, Vec2::new(3.0, 4.0));
    tree.update();
    assert!(approx(
        tree.world_transform(b).unwrap().origin,
        Vec2::new(4.0, 6.0)
    ));

    tree.set_position(a, Vec2::new(10.0, 20.0));
    tree.update();
    assert!(approx(
        tree.world_transform(b).unwrap().origin,
        Vec2::new(13.0, 24.0)
    ));
}

#[test]
fn visibility_propagates() {
    let mut tree = SceneTree::new();
    let root = tree.root();
    let a = tree.add_node2d(root, "A");
    let b = tree.add_node2d(a, "B");
    tree.update();

    assert_eq!(tree.is_visible(a), Some(true));
    assert_eq!(tree.is_visible_in_tree(b), Some(true));

    tree.set_visible(a, false);
    tree.update();
    assert_eq!(tree.is_visible(a), Some(false));
    assert_eq!(tree.is_visible_in_tree(a), Some(false));
    assert_eq!(tree.is_visible_in_tree(b), Some(false));

    tree.set_visible(a, true);
    tree.update();
    assert_eq!(tree.is_visible_in_tree(b), Some(true));
}

#[test]
fn z_index_orders_traversal() {
    let mut tree = SceneTree::new();
    let root = tree.root();
    let a = tree.add_node2d(root, "A");
    let b = tree.add_node2d(root, "B");
    let c = tree.add_node2d(root, "C");

    // default order is creation order
    assert_eq!(names(&tree), ["root", "A", "B", "C"]);

    tree.set_z_index(c, -1);
    tree.set_z_index(a, 5);
    assert_eq!(names(&tree), ["root", "C", "B", "A"]);
    assert_eq!(tree.z_index(a), Some(5));
    let _ = b;
}

#[test]
fn iter_visible_prunes_hidden_subtrees() {
    let mut tree = SceneTree::new();
    let root = tree.root();
    let a = tree.add_node2d(root, "A");
    let b = tree.add_node2d(a, "B");
    let c = tree.add_node2d(root, "C");
    tree.update();

    let all: Vec<String> = tree
        .iter()
        .map(|id| tree.node(id).name().to_string())
        .collect();
    assert_eq!(all, ["root", "A", "B", "C"]);

    tree.set_visible(a, false);
    let visible: Vec<String> = tree
        .iter_visible()
        .map(|id| tree.node(id).name().to_string())
        .collect();
    assert_eq!(visible, ["root", "C"]);
    let _ = (b, c);
}

#[test]
fn dirty_flags_skip_clean_nodes() {
    let mut tree = SceneTree::new();
    let root = tree.root();
    let a = tree.add_node2d(root, "A");
    let b = tree.add_node2d(a, "B");

    // first update computes world transforms for A and B
    assert_eq!(tree.update(), 2);
    // nothing changed -> no recomputation
    assert_eq!(tree.update(), 0);

    tree.set_position(a, Vec2::new(1.0, 1.0));
    // A recomputes; B recomputes because its parent changed
    assert_eq!(tree.update(), 2);
    assert_eq!(tree.update(), 0);

    tree.set_visible(b, false);
    // only visibility changed, transforms stay clean
    assert_eq!(tree.update(), 0);
}

#[test]
fn needs_update_tracks_pending_dirty_flags() {
    let mut tree = SceneTree::new();
    let a = tree.add_node2d(tree.root(), "A");

    assert!(tree.needs_update(), "fresh nodes are dirty");
    tree.update();
    assert!(!tree.needs_update(), "clean after update");

    tree.set_position(a, Vec2::new(1.0, 0.0));
    assert!(tree.needs_update());
    tree.update();
    assert!(!tree.needs_update());

    tree.set_visible(a, false);
    assert!(tree.needs_update(), "visibility is tracked too");
    tree.update();
    assert!(!tree.needs_update());
}

#[test]
fn remove_frees_whole_subtree() {
    let mut tree = SceneTree::new();
    let root = tree.root();
    let a = tree.add_node2d(root, "A");
    let b = tree.add_node2d(a, "B");
    let c = tree.add_node2d(b, "C");

    assert_eq!(tree.node_count(), 4);
    assert!(tree.remove(a));
    assert_eq!(tree.node_count(), 1);
    assert!(!tree.contains(a));
    assert!(!tree.contains(b));
    assert!(!tree.contains(c));
    assert_eq!(tree.children(root), Some(&[][..]));

    // stale handles are rejected
    assert!(tree.get(a).is_none());
    assert!(!tree.remove(a));
    // root cannot be removed
    assert!(!tree.remove(root));
}

#[test]
fn reparent_moves_subtree_and_rejects_cycles() {
    let mut tree = SceneTree::new();
    let root = tree.root();
    let p1 = tree.add_node2d(root, "P1");
    let p2 = tree.add_node2d(root, "P2");
    let child = tree.add_node2d(p1, "Child");

    assert!(tree.reparent(child, p2));
    assert_eq!(tree.parent(child), Some(p2));
    assert_eq!(tree.children(p1), Some(&[][..]));
    assert_eq!(tree.children(p2), Some(&[child][..]));

    // cannot reparent a node under its own descendant
    assert!(!tree.reparent(p2, child));
    // cannot reparent the root
    assert!(!tree.reparent(root, p1));

    // moved subtree recomputes on next update
    tree.set_position(p2, Vec2::new(7.0, 0.0));
    tree.set_position(child, Vec2::new(1.0, 0.0));
    assert!(tree.update() >= 2);
    assert!(approx(
        tree.world_transform(child).unwrap().origin,
        Vec2::new(8.0, 0.0)
    ));
}

// -- Stage 25.1 (Phase 1): extension slot + canvas layers + cameras --------

#[derive(Debug, PartialEq)]
struct Counter(u32);

#[derive(Debug, PartialEq)]
struct Label(String);

#[test]
fn node_data_slot_round_trips_and_type_checks() {
    let mut tree = SceneTree::new();
    let root = tree.root();
    let a = tree.add_node(root, "A");

    assert!(!tree.node(a).has_data::<Counter>());
    assert_eq!(tree.node(a).data::<Counter>(), None);
    assert_eq!(tree.node_mut(a).data_mut::<Counter>(), None);
    assert_eq!(tree.node_mut(a).take_data::<Counter>(), None);

    tree.node_mut(a).set_data(Counter(3));
    assert!(tree.node(a).has_data::<Counter>());
    assert!(!tree.node(a).has_data::<Label>());
    assert_eq!(tree.node(a).data::<Counter>(), Some(&Counter(3)));
    // Wrong-type downcast is a `None`, not a panic.
    assert_eq!(tree.node(a).data::<Label>(), None);

    tree.node_mut(a).data_mut::<Counter>().unwrap().0 = 9;
    assert_eq!(tree.node(a).data::<Counter>(), Some(&Counter(9)));

    let taken = tree.node_mut(a).take_data::<Counter>();
    assert_eq!(taken, Some(Counter(9)));
    assert!(!tree.node(a).has_data::<Counter>());
}

#[test]
fn node_data_is_independent_per_node() {
    let mut tree = SceneTree::new();
    let root = tree.root();
    let a = tree.add_node(root, "A");
    let b = tree.add_node(root, "B");

    tree.node_mut(a).set_data(Counter(1));
    tree.node_mut(b).set_data(Label("b".into()));

    assert_eq!(tree.node(a).data::<Counter>(), Some(&Counter(1)));
    assert!(!tree.node(a).has_data::<Label>());
    assert_eq!(tree.node(b).data::<Label>().unwrap().0, "b");
    assert!(!tree.node(b).has_data::<Counter>());

    // Different types coexist on one node; only the same type is replaced.
    tree.node_mut(a).set_data(Label("a".into()));
    assert!(tree.node(a).has_data::<Counter>());
    assert!(tree.node(a).has_data::<Label>());
    assert_eq!(tree.node(a).data::<Counter>(), Some(&Counter(1)));

    tree.node_mut(a).clear_data();
    assert!(!tree.node(a).has_data::<Label>());
    assert!(!tree.node(a).has_data::<Counter>());
    assert!(tree.node(b).has_data::<Label>());
}

#[test]
fn add_canvas_layer_and_camera_kinds() {
    let mut tree = SceneTree::new();
    let root = tree.root();

    let layer = tree.add_canvas_layer(root, "Layer");
    assert_eq!(tree.node(layer).kind(), NodeKind::CanvasLayer);
    // A CanvasLayer is not a canvas item; its children are.
    assert!(!tree.node(layer).is_canvas_item());
    assert_eq!(
        tree.node(layer).canvas_layer(),
        Some(&CanvasLayerData::default())
    );
    assert_eq!(tree.node(layer).camera_2d(), None);
    assert_eq!(tree.canvas_layer_data(layer).unwrap().layer, 1);

    let camera = tree.add_camera_2d(root, "Camera");
    assert_eq!(tree.node(camera).kind(), NodeKind::Camera2D);
    // Camera2D implies a canvas item (it has a local transform).
    assert!(tree.node(camera).is_canvas_item());
    assert_eq!(tree.node(camera).camera_2d().unwrap().zoom, Vec2::ONE);
    assert_eq!(tree.node(camera).canvas_layer(), None);

    let plain = tree.add_node(root, "Plain");
    assert_eq!(tree.canvas_layer_data(plain), None);
    assert_eq!(tree.camera_2d_data(plain), None);
}

#[test]
fn canvas_layer_setters() {
    let mut tree = SceneTree::new();
    let root = tree.root();
    let layer = tree.add_canvas_layer(root, "Layer");

    assert!(tree.set_canvas_layer(layer, 7));
    assert!(tree.set_canvas_layer_follow_viewport(layer, true));
    let xform = Transform2D::from_translation(Vec2::new(3.0, 4.0));
    assert!(tree.set_canvas_layer_transform(layer, xform));
    let data = tree.canvas_layer_data(layer).unwrap();
    assert_eq!(data.layer, 7);
    assert!(data.follow_viewport);
    assert_eq!(data.transform, xform);

    // Non-layer nodes reject layer setters.
    let world = tree.add_node2d(root, "World");
    assert!(!tree.set_canvas_layer(world, 2));
    assert!(!tree.set_canvas_layer_transform(world, xform));
    assert!(!tree.set_canvas_layer_follow_viewport(world, true));
}

#[test]
fn camera_setters() {
    let mut tree = SceneTree::new();
    let root = tree.root();
    let camera = tree.add_camera_2d(root, "Camera");

    assert!(tree.set_camera_current(camera, true));
    assert!(tree.set_camera_enabled(camera, false));
    assert!(tree.set_camera_zoom(camera, Vec2::new(2.0, 2.0)));
    assert!(tree.set_camera_offset(camera, Vec2::new(1.0, -1.0)));
    let data = tree.camera_2d_data(camera).unwrap();
    assert!(data.current);
    assert!(!data.enabled);
    assert_eq!(data.zoom, Vec2::new(2.0, 2.0));
    assert_eq!(data.offset, Vec2::new(1.0, -1.0));

    let plain = tree.add_node(root, "Plain");
    assert!(!tree.set_camera_current(plain, true));
    assert!(!tree.set_camera_enabled(plain, true));
    assert!(!tree.set_camera_zoom(plain, Vec2::ONE));
    assert!(!tree.set_camera_offset(plain, Vec2::ZERO));
}

#[test]
fn canvas_layer_of_resolves_nearest_ancestor() {
    let mut tree = SceneTree::new();
    let root = tree.root();
    let world = tree.add_node2d(root, "World");

    // No layer anywhere: default world canvas.
    assert_eq!(tree.canvas_layer_of(world), None);
    assert_eq!(tree.canvas_layer_of(root), None);

    let outer = tree.add_canvas_layer(root, "Outer");
    assert_eq!(tree.canvas_layer_of(outer), None);

    let inner = tree.add_canvas_layer(outer, "Inner");
    tree.set_canvas_layer(outer, 10);
    tree.set_canvas_layer(inner, 20);

    let ui = tree.add_control(inner, "Ui");
    let plain = tree.add_node(inner, "Plain");
    let deep = tree.add_node2d(plain, "Deep");

    // Nearest ancestor wins, even through non-canvas intermediaries.
    assert_eq!(tree.canvas_layer_of(ui).unwrap().0, inner);
    assert_eq!(tree.canvas_layer_of(deep).unwrap().0, inner);
    assert_eq!(tree.canvas_layer_of(deep).unwrap().1.layer, 20);
    assert_eq!(tree.canvas_layer_of(plain).unwrap().0, inner);

    // The layer node itself is not affected by itself.
    assert_eq!(tree.canvas_layer_of(inner).unwrap().0, outer);
    assert_eq!(tree.canvas_layer_of(inner).unwrap().1.layer, 10);
}

#[test]
fn canvas_layer_of_survives_reparent() {
    let mut tree = SceneTree::new();
    let root = tree.root();
    let layer = tree.add_canvas_layer(root, "Layer");
    let world = tree.add_node2d(root, "World");
    let node = tree.add_node2d(world, "Node");

    assert_eq!(tree.canvas_layer_of(node), None);
    assert!(tree.reparent(node, layer));
    assert_eq!(tree.canvas_layer_of(node).unwrap().0, layer);
    assert!(tree.reparent(node, world));
    assert_eq!(tree.canvas_layer_of(node), None);
}

#[test]
fn canvas_layer_default_transform_is_identity() {
    let mut tree = SceneTree::new();
    let root = tree.root();
    let layer = tree.add_canvas_layer(root, "Layer");
    let data = tree.canvas_layer_data(layer).unwrap();
    assert_eq!(data.transform, Transform2D::IDENTITY);
}

// -- Stage 25.2 (Phase 2): viewport + camera + view transforms -------------

fn camera_scene(size: Vec2, camera_pos: Vec2) -> (SceneTree, NodeId) {
    let mut tree = SceneTree::new();
    tree.set_viewport_size(draw_core::Size::new(size.x, size.y));
    let root = tree.root();
    let camera = tree.add_camera_2d(root, "Camera");
    tree.set_camera_current(camera, true);
    tree.set_position(camera, camera_pos);
    tree.update();
    (tree, camera)
}

#[test]
fn no_camera_leaves_identity_transform() {
    let mut tree = SceneTree::new();
    tree.set_viewport_size(draw_core::Size::new(100.0, 100.0));
    tree.update();
    assert_eq!(tree.canvas_transform(), Transform2D::IDENTITY);
    assert!(approx(
        tree.world_to_screen(Vec2::new(3.0, 4.0)),
        Vec2::new(3.0, 4.0)
    ));
    assert!(approx(
        tree.screen_to_world(Vec2::new(3.0, 4.0)),
        Vec2::new(3.0, 4.0)
    ));
}

#[test]
fn current_camera_centers_on_its_position() {
    // 100x100 viewport, camera centered at world (50,50).
    let (mut tree, camera) = camera_scene(Vec2::new(100.0, 100.0), Vec2::new(50.0, 50.0));
    assert!(approx(
        tree.world_to_screen(Vec2::new(50.0, 50.0)),
        Vec2::new(50.0, 50.0)
    ));
    // Camera moves +10 in x: the world point tracks the camera so it stays
    // centered; a fixed world point shifts -10 on screen.
    tree.set_position(camera, Vec2::new(60.0, 50.0));
    tree.update();
    assert!(approx(
        tree.world_to_screen(Vec2::new(60.0, 50.0)),
        Vec2::new(50.0, 50.0)
    ));
    assert!(approx(
        tree.world_to_screen(Vec2::new(50.0, 50.0)),
        Vec2::new(40.0, 50.0)
    ));
}

#[test]
fn camera_zoom_scales_around_center() {
    let mut tree = SceneTree::new();
    tree.set_viewport_size(draw_core::Size::new(100.0, 100.0));
    let root = tree.root();
    let camera = tree.add_camera_2d(root, "Camera");
    tree.set_camera_current(camera, true);
    tree.set_position(camera, Vec2::new(50.0, 50.0));
    tree.set_camera_zoom(camera, Vec2::new(2.0, 2.0));
    tree.update();

    // Center maps to center; 25 world units away maps 50 screen units away.
    assert!(approx(
        tree.world_to_screen(Vec2::new(50.0, 50.0)),
        Vec2::new(50.0, 50.0)
    ));
    assert!(approx(
        tree.world_to_screen(Vec2::new(75.0, 50.0)),
        Vec2::new(100.0, 50.0)
    ));
}

#[test]
fn camera_anchor_fixed_top_left() {
    let mut tree = SceneTree::new();
    tree.set_viewport_size(draw_core::Size::new(100.0, 100.0));
    let root = tree.root();
    let camera = tree.add_camera_2d(root, "Camera");
    tree.set_camera_current(camera, true);
    tree.set_position(camera, Vec2::new(10.0, 20.0));
    tree.set_camera_anchor_mode(camera, AnchorMode::FixedTopLeft);
    tree.update();

    assert!(approx(
        tree.world_to_screen(Vec2::new(10.0, 20.0)),
        Vec2::new(0.0, 0.0)
    ));
    assert!(approx(
        tree.world_to_screen(Vec2::new(20.0, 30.0)),
        Vec2::new(10.0, 10.0)
    ));
}

#[test]
fn camera_offset_shifts_the_view() {
    let mut tree = SceneTree::new();
    tree.set_viewport_size(draw_core::Size::new(100.0, 100.0));
    let root = tree.root();
    let camera = tree.add_camera_2d(root, "Camera");
    tree.set_camera_current(camera, true);
    tree.set_position(camera, Vec2::new(50.0, 50.0));
    tree.set_camera_offset(camera, Vec2::new(5.0, 0.0));
    tree.update();
    // Offset shifts the camera's screen rect: world center lands at 50 - 5.
    assert!(approx(
        tree.world_to_screen(Vec2::new(50.0, 50.0)),
        Vec2::new(45.0, 50.0)
    ));
}

#[test]
fn disabled_or_non_current_cameras_are_ignored() {
    let mut tree = SceneTree::new();
    tree.set_viewport_size(draw_core::Size::new(100.0, 100.0));
    let root = tree.root();
    let camera = tree.add_camera_2d(root, "Camera");
    tree.set_position(camera, Vec2::new(50.0, 50.0));
    tree.set_camera_current(camera, true);
    tree.set_camera_enabled(camera, false);
    tree.update();
    assert_eq!(tree.canvas_transform(), Transform2D::IDENTITY);

    tree.set_camera_enabled(camera, true);
    tree.set_camera_current(camera, false);
    tree.update();
    assert_eq!(tree.canvas_transform(), Transform2D::IDENTITY);
}

#[test]
fn screen_to_world_inverts_world_to_screen() {
    let (tree, _) = camera_scene(Vec2::new(128.0, 96.0), Vec2::new(40.0, -10.0));
    let world = Vec2::new(123.0, 45.0);
    let screen = tree.world_to_screen(world);
    assert!(approx(tree.screen_to_world(screen), world));
    assert_eq!(tree.viewport().size(), draw_core::Size::new(128.0, 96.0));
}

#[test]
fn viewport_transform_composes_camera_and_world() {
    let mut tree = SceneTree::new();
    tree.set_viewport_size(draw_core::Size::new(100.0, 100.0));
    let root = tree.root();
    // Camera at (10,10) with the default drag-center anchor.
    let camera = tree.add_camera_2d(root, "Camera");
    tree.set_camera_current(camera, true);
    tree.set_position(camera, Vec2::new(10.0, 10.0));
    let node = tree.add_node2d(root, "Node");
    tree.set_position(node, Vec2::new(5.0, 5.0));
    tree.update();

    // canvas_transform = translate(50 - 10) = +40; world (5,5) -> screen (45,45).
    assert!(approx(
        tree.viewport_transform(node).unwrap().origin,
        Vec2::new(45.0, 45.0)
    ));
}

#[test]
fn canvas_transforms_are_layer_aware() {
    let mut tree = SceneTree::new();
    tree.set_viewport_size(draw_core::Size::new(100.0, 100.0));
    let root = tree.root();
    // Camera at (0,0) drag-center => canvas_transform = translate(50, 50).
    let camera = tree.add_camera_2d(root, "Camera");
    tree.set_camera_current(camera, true);
    tree.set_position(camera, Vec2::new(0.0, 0.0));

    let world = tree.add_node2d(root, "World");
    let layer = tree.add_canvas_layer(root, "Ui");
    let layer_transform = Transform2D::from_translation(Vec2::new(7.0, -3.0));
    tree.set_canvas_layer_transform(layer, layer_transform);
    let ui = tree.add_node2d(layer, "UiNode");
    tree.set_position(ui, Vec2::new(1.0, 2.0));
    tree.update();

    // Default canvas uses the camera transform.
    assert!(approx(
        tree.canvas_transform_of(world).origin,
        Vec2::new(50.0, 50.0)
    ));
    // A layer ignores the camera unless follow_viewport is set.
    assert_eq!(tree.canvas_transform_of(ui), layer_transform);
    assert!(approx(
        tree.viewport_transform(ui).unwrap().origin,
        Vec2::new(8.0, -1.0)
    ));

    // follow_viewport composes camera * layer after the camera changes.
    tree.set_canvas_layer_follow_viewport(layer, true);
    tree.update();
    let expected = tree.canvas_transform() * layer_transform;
    assert_eq!(tree.canvas_transform_of(ui), expected);
}

#[test]
fn camera_zigzag_transform_updates_each_frame() {
    let (mut tree, camera) = camera_scene(Vec2::new(200.0, 100.0), Vec2::new(100.0, 50.0));
    assert!(approx(
        tree.world_to_screen(Vec2::new(100.0, 50.0)),
        Vec2::new(100.0, 50.0)
    ));
    tree.set_position(camera, Vec2::new(0.0, 0.0));
    tree.update();
    assert!(approx(
        tree.world_to_screen(Vec2::new(0.0, 0.0)),
        Vec2::new(100.0, 50.0)
    ));
    tree.set_position(camera, Vec2::new(200.0, 100.0));
    tree.update();
    assert!(approx(
        tree.world_to_screen(Vec2::new(200.0, 100.0)),
        Vec2::new(100.0, 50.0)
    ));
}
