use super::*;
use crate::node::NodeKind;
use draw_core::Vec2;
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
    assert_eq!(tree.node(root).kind(), NodeKind::Node);
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
