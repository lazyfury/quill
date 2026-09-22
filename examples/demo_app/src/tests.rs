//! `DemoApp` behaviour: navigation, the theme rebuild and the headless pipeline.

use crate::*;
use draw_backend_recording::RecordingBackend;
use draw_core::{Color, PointerButton};
use draw_render::{DrawCommand, RenderBackend};

fn laid_out() -> DemoApp {
    let viewport = ViewportSize::new(Size::new(1200.0, 760.0));
    let mut app = DemoApp::new();
    app.update(viewport, 0.016);
    app.layout(viewport);
    app
}

fn click(app: &mut DemoApp, position: Vec2) {
    app.event(&InputEvent::PointerDown {
        position,
        button: PointerButton::Left,
    });
    app.event(&InputEvent::PointerUp {
        position,
        button: PointerButton::Left,
    });
}

#[test]
fn every_group_has_at_least_one_item() {
    assert_eq!(catalog::ITEMS.len(), catalog::GROUPS.len());
    for (group, items) in catalog::ITEMS.iter().enumerate() {
        assert!(!items.is_empty(), "group {group} has no items");
    }
}

#[test]
fn the_primary_button_toggles_the_theme() {
    let mut app = laid_out();
    let center = app.button_center().expect("primary button laid out");
    click(&mut app, center);
    assert_eq!(app.clicks(), 1);

    let viewport = app.viewport();
    app.update(viewport, 0.016);
    app.layout(viewport);
    assert_eq!(app.theme().mode(), Mode::Light);
    assert_eq!(app.clicks(), 1, "the click count survives the rebuild");
}

#[test]
fn show_group_switches_the_preview() {
    let mut app = laid_out();
    for group in 0..catalog::GROUPS.len() {
        app.show_group(group);
        assert_eq!(app.group(), group);
        assert_eq!(app.router().index(), group);
        app.layout(app.viewport());
        assert!(app.control_count() > 0);
    }
}

#[test]
fn titlebar_inset_pads_the_sidebar() {
    let mut app = laid_out();
    app.set_titlebar_inset(28.0);
    assert_eq!(app.titlebar_inset(), 28.0);
}

#[test]
fn a_short_window_scrolls_the_preview() {
    let viewport = ViewportSize::new(Size::new(1200.0, 300.0));
    let mut app = DemoApp::new();
    app.update(viewport, 0.016);
    app.layout(viewport);
    assert_eq!(app.preview_scroll(), 0.0);

    app.event(&InputEvent::Wheel {
        position: Vec2::new(700.0, 200.0),
        delta: Vec2::new(0.0, 200.0),
    });
    app.update(viewport, 0.016);
    app.layout(viewport);
    assert!(app.preview_scroll() > 0.0, "the preview page should scroll");
}

#[test]
fn the_theme_page_paints_palette_colours() {
    let mut app = laid_out();
    app.show_group(8);
    app.layout(app.viewport());

    let mut ctx = PaintContext::new();
    app.paint(&mut ctx);
    let list = ctx.into_draw_list();
    let filled = |color: Color| {
        list.commands().iter().any(|command| match command {
            DrawCommand::FillRect { paint, .. } | DrawCommand::FillRoundedRect { paint, .. } => {
                paint.color == color
            }
            _ => false,
        })
    };

    assert!(
        filled(app.theme().palette().accent),
        "the palette preview should paint the accent colour"
    );
    assert!(
        !filled(Color::new(0.13, 0.15, 0.20, 1.0)),
        "no swatch should fall back to the default Panel grey"
    );
}

#[test]
fn the_icons_page_paints_glyph_strokes() {
    let mut app = laid_out();
    app.show_group(4);
    app.layout(app.viewport());

    let mut ctx = PaintContext::new();
    app.paint(&mut ctx);
    let list = ctx.into_draw_list();
    assert!(
        list.commands()
            .iter()
            .any(|command| matches!(command, DrawCommand::Line { .. })),
        "the Icons page should stroke glyph lines"
    );
    assert!(
        list.commands()
            .iter()
            .any(|command| matches!(command, DrawCommand::FillCircle { .. })),
        "the Icons page should fill glyph dots"
    );
}

#[test]
fn full_pipeline_records_a_draw_list_headlessly() {
    let app = laid_out();
    let viewport = app.viewport();

    let mut ctx = PaintContext::new();
    app.paint(&mut ctx);
    let list = ctx.into_draw_list();

    let mut backend = RecordingBackend::new();
    backend.begin_frame(viewport).unwrap();
    backend.submit(&list).unwrap();
    backend.end_frame().unwrap();

    assert_eq!(backend.frame_count(), 1);
    let commands = backend.last_frame().expect("frame").commands();
    assert!(commands
        .iter()
        .any(|c| matches!(c, DrawCommand::FillRect { .. })));
    assert!(commands
        .iter()
        .any(|c| matches!(c, DrawCommand::DrawText { .. })));
    assert!(commands
        .iter()
        .any(|c| matches!(c, DrawCommand::FillRoundedRect { .. })));
}
