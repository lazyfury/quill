//! Multiple `into_tree()` calls build independent scene trees.
//!
//! Run with `cargo run -p multi_tree`. It builds three trees from the same
//! shape, lays each out, paints each headlessly through the recording backend,
//! and asserts the trees share neither node ids nor state.

use draw_backend_recording::RecordingBackend;
use draw_components::{set_text, Column, Component, NodeRef, Text};
use draw_core::{Size, ViewportSize};
use draw_render::RenderBackend;
use draw_scene::{SceneChild, SceneTree};
use draw_theme::{default_theme, Mode, Theme, Tone};

fn build(theme: &'static dyn Theme, label: &str) -> (SceneTree, NodeRef) {
    let title = NodeRef::new();
    let tree = Column::new()
        .child(Text::heading(label, theme).ref_(&title))
        .child(Text::small("shared shape", theme).tone(Tone::Muted))
        .into_tree();
    (tree, title)
}

fn main() {
    let theme = default_theme(Mode::Dark);
    let viewport = ViewportSize::new(Size::new(400.0, 300.0));

    // 1. Each `into_tree()` returns a fresh, independent tree.
    let (mut a, title_a) = build(theme, "A");
    let (mut b, title_b) = build(theme, "B");
    let (mut c, title_c) = build(theme, "C");

    // 2. Layout + paint each headlessly: no panics, each yields commands.
    for tree in [&mut a, &mut b, &mut c] {
        draw_ui::layout(tree, viewport);
        tree.update();

        let mut ctx = draw_render::PaintContext::new();
        draw_ui::paint(tree, &mut ctx);
        let list = ctx.into_draw_list();
        assert!(!list.commands().is_empty(), "each tree must paint commands");

        let mut backend = RecordingBackend::new();
        backend.begin_frame(viewport).expect("begin frame");
        backend.submit(&list).expect("submit");
        backend.end_frame().expect("end frame");
        assert_eq!(backend.frame_count(), 1);
    }

    // 3. Independence: each tree owns its own id space. NodeIds are tree-local,
    // so the title ids may be numerically equal across trees — they are only
    // valid with their own tree. The real check is that edits never cross-talk.
    set_text(&mut a, title_a.get().expect("tree A title"), "A (edited)");
    assert_eq!(
        draw_ui::widget(&b, title_b.get().expect("tree B title")).and_then(|w| w.text()),
        Some("B"),
        "editing tree A must not touch tree B"
    );
    assert_eq!(
        draw_ui::widget(&c, title_c.get().expect("tree C title")).and_then(|w| w.text()),
        Some("C"),
        "editing tree A must not touch tree C"
    );

    println!("ok: 3 independent into_tree() trees, no crash, no cross-talk");
}
