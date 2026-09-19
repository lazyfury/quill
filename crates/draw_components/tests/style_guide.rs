//! End-to-end check that the themed component library builds, lays out and
//! paints a representative screen through the headless recording backend.

use draw_backend_recording::RecordingBackend;
use draw_components::{
    Badge, Card, Checkbox, CodeBlock, Divider, EmptyState, Switch, Terminal, Text,
};
use draw_core::{Edges, Size, ViewportSize};
use draw_render::{DrawCommand, PaintContext, RenderBackend};
use draw_scene::SceneTree;
use draw_theme::{Theme, Tone};

fn build(theme: Theme) -> SceneTree {
    let mut tree = SceneTree::new();
    draw_ui::set_theme(&mut tree, theme);
    let tree_root = tree.root();
    let root = draw_app::add_flex(&mut tree, tree_root, draw_ui::FlexStyle::column());
    draw_app::update_control(&mut tree, root, |data| {
        data.mouse_filter = draw_ui::MouseFilter::Ignore
    });

    let card = draw_app::add(
        &mut tree,
        root,
        Card::new().padding(Edges::all(24.0)).gap(12.0),
    );
    draw_app::add(&mut tree, card.id(), Text::title("Component Kit"));
    draw_app::add(
        &mut tree,
        card.id(),
        Text::small("design tokens + themed components").tone(Tone::Muted),
    );
    draw_app::add(&mut tree, card.id(), Divider::horizontal());
    draw_app::add(
        &mut tree,
        card.id(),
        Badge::new("Stable").tone(Tone::Success),
    );
    draw_app::add(
        &mut tree,
        card.id(),
        Checkbox::new("Enable logs").checked(true),
    );
    draw_app::add(
        &mut tree,
        card.id(),
        Switch::new().label("Dark mode").on(true),
    );
    draw_app::add(
        &mut tree,
        card.id(),
        CodeBlock::new("cargo test --workspace")
            .filename("shell")
            .language("bash"),
    );
    draw_app::add(
        &mut tree,
        card.id(),
        Terminal::new()
            .command("cargo test")
            .output("test result: ok. 23 passed; 0 failed"),
    );
    draw_app::add(
        &mut tree,
        card.id(),
        EmptyState::new("No items").description("Create one to get started."),
    );

    tree
}

fn render(theme: Theme) -> Vec<DrawCommand> {
    let mut tree = build(theme);
    let viewport = ViewportSize::new(Size::new(560.0, 1000.0));
    draw_ui::layout(&mut tree, viewport);
    tree.update();

    let mut ctx = PaintContext::new();
    draw_ui::paint(&tree, &mut ctx);
    let list = ctx.into_draw_list();

    let mut backend = RecordingBackend::new();
    backend.begin_frame(viewport).unwrap();
    backend.submit(&list).unwrap();
    backend.end_frame().unwrap();
    backend
        .last_frame()
        .expect("recorded frame")
        .commands()
        .to_vec()
}

#[test]
fn style_guide_renders_surfaces_borders_text_and_indicators() {
    let commands = render(Theme::dark());

    assert!(commands
        .iter()
        .any(|c| matches!(c, DrawCommand::FillRoundedRect { .. })));
    assert!(commands
        .iter()
        .any(|c| matches!(c, DrawCommand::FillCircle { .. })));
    assert!(commands
        .iter()
        .any(|c| matches!(c, DrawCommand::DrawText { .. })));

    let texts: Vec<&str> = commands
        .iter()
        .filter_map(|c| match c {
            DrawCommand::DrawText { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert!(texts.iter().any(|t| t.contains("Component Kit")));
    assert!(texts.iter().any(|t| t.contains("test result: ok")));
}

#[test]
fn light_and_dark_render_the_same_structure() {
    let light = render(Theme::light());
    let dark = render(Theme::dark());
    assert_eq!(light.len(), dark.len());
}
