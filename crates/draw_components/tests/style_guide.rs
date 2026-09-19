//! End-to-end check that the themed component library builds, lays out and
//! paints a representative screen through the headless recording backend.

use draw_backend_recording::RecordingBackend;
use draw_components::{
    Badge, Card, Checkbox, CodeBlock, Divider, EmptyState, Switch, Terminal, Text,
};
use draw_core::{Edges, Size, Viewport};
use draw_render::{DrawCommand, PaintContext, RenderBackend};
use draw_theme::Theme;
use draw_ui::Tone;
use draw_ui::Ui;

fn build(theme: Theme) -> Ui {
    let mut ui = Ui::new();
    ui.set_theme(theme);
    let root = ui.root();

    let card = ui.add(root, Card::new().padding(Edges::all(24.0)).gap(12.0));
    ui.add(card.id(), Text::title("Component Kit"));
    ui.add(
        card.id(),
        Text::small("design tokens + themed components").tone(Tone::Muted),
    );
    ui.add(card.id(), Divider::horizontal());
    ui.add(card.id(), Badge::new("Stable").tone(Tone::Success));
    ui.add(card.id(), Checkbox::new("Enable logs").checked(true));
    ui.add(card.id(), Switch::new().label("Dark mode").on(true));
    ui.add(
        card.id(),
        CodeBlock::new("cargo test --workspace")
            .filename("shell")
            .language("bash"),
    );
    ui.add(
        card.id(),
        Terminal::new()
            .command("cargo test")
            .output("test result: ok. 23 passed; 0 failed"),
    );
    ui.add(
        card.id(),
        EmptyState::new("No items").description("Create one to get started."),
    );

    ui
}

fn render(theme: Theme) -> Vec<DrawCommand> {
    let mut ui = build(theme);
    let viewport = Viewport::new(Size::new(560.0, 1000.0));
    ui.layout(viewport);

    let mut ctx = PaintContext::new();
    ui.paint(&mut ctx);
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
