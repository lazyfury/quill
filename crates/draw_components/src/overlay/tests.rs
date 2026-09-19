use std::cell::Cell;
use std::rc::Rc;

use draw_core::{InputEvent, PointerButton, Size, Vec2, Viewport};
use draw_render::{DrawCommand, PaintContext, RenderBackend};

use super::*;

fn host() -> (Ui, NodeId) {
    let mut ui = Ui::new();
    let button = ui.add_button(ui.root(), "Anchor");
    ui.layout(Viewport::new(Size::new(400.0, 300.0)));
    (ui, button)
}

fn commands(overlays: &Overlays) -> Vec<DrawCommand> {
    let mut ctx = PaintContext::new();
    overlays.paint(&mut ctx);
    ctx.into_draw_list().into_commands()
}

#[test]
fn confirm_paints_a_scrim_and_its_text() {
    let (host_ui, _button) = host();
    let viewport = Viewport::new(Size::new(400.0, 300.0));
    let mut overlays = Overlays::new(Theme::dark());
    let id = overlays.confirm("Delete note?", "This cannot be undone.");
    assert!(overlays.is_open(id));
    overlays.layout(&host_ui, viewport);

    let list = commands(&overlays);
    // A scrim fill and the title/message text are emitted.
    assert!(list
        .iter()
        .any(|command| matches!(command, DrawCommand::FillRect { .. })));
    let texts: Vec<&str> = list
        .iter()
        .filter_map(|command| match command {
            DrawCommand::DrawText { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert!(texts.contains(&"Delete note?"));
    assert!(texts.contains(&"This cannot be undone."));

    // A Confirm widget can be painted through a backend end-to-end.
    let mut backend = draw_backend_recording::RecordingBackend::new();
    backend.begin_frame(viewport).unwrap();
    backend.submit(&ctx_list(&overlays)).unwrap();
    backend.end_frame().unwrap();
    assert!(backend.last_frame().is_some());
}

fn ctx_list(overlays: &Overlays) -> draw_render::DrawList {
    let mut ctx = PaintContext::new();
    overlays.paint(&mut ctx);
    ctx.into_draw_list()
}

#[test]
fn escape_dismisses_the_top_overlay() {
    let (host_ui, _button) = host();
    let viewport = Viewport::new(Size::new(400.0, 300.0));
    let mut overlays = Overlays::new(Theme::dark());
    let id = overlays.confirm("Title", "Body");
    overlays.layout(&host_ui, viewport);

    let result = overlays.handle_input(&InputEvent::KeyDown { key: Key::Escape });
    assert!(result.is_handled());
    assert!(!overlays.is_open(id));
}

#[test]
fn outside_click_dismisses_a_popover() {
    let (host_ui, button) = host();
    let viewport = Viewport::new(Size::new(400.0, 300.0));
    let mut overlays = Overlays::new(Theme::dark());
    let id = overlays.popover(button, Placement::Below, |_| {});
    overlays.layout(&host_ui, viewport);

    let result = overlays.handle_input(&InputEvent::PointerDown {
        position: Vec2::new(399.0, 299.0),
        button: PointerButton::Left,
    });
    assert!(result.is_handled());
    assert!(!overlays.is_open(id));
}

#[test]
fn modal_consumes_pointer_moves() {
    let (host_ui, _button) = host();
    let viewport = Viewport::new(Size::new(400.0, 300.0));
    let mut overlays = Overlays::new(Theme::dark());
    overlays.confirm("Title", "Body");
    overlays.layout(&host_ui, viewport);

    let result = overlays.handle_input(&InputEvent::PointerMove {
        position: Vec2::new(5.0, 5.0),
    });
    assert!(result.is_handled());
}

#[test]
fn popover_sits_below_its_target() {
    let (host_ui, button) = host();
    let target = host_ui.control(button).unwrap().rect;
    let viewport = Viewport::new(Size::new(400.0, 300.0));

    let mut overlays = Overlays::new(Theme::dark());
    let id = overlays.popover(button, Placement::Below, |cx| {
        cx.child(Label::new("Menu"));
    });
    overlays.layout(&host_ui, viewport);

    let rect = overlays.rect(id).expect("overlay rect");
    assert!(
        (rect.top() - (target.bottom() + OFFSET)).abs() < 1e-3,
        "overlay top {} vs target bottom {}",
        rect.top(),
        target.bottom()
    );
}

#[test]
fn tips_track_the_hovered_target() {
    let (mut host_ui, button) = host();
    let viewport = Viewport::new(Size::new(400.0, 300.0));
    let mut overlays = Overlays::new(Theme::dark());
    let id = overlays.tips(button, "Helpful hint");

    // Not hovered yet -> the tip is dropped on the next layout.
    overlays.layout(&host_ui, viewport);
    assert!(!overlays.is_open(id));

    // Hover the target, then lay out -> the tip stays.
    let center = host_ui.control(button).unwrap().rect.center();
    host_ui.handle_input(&InputEvent::PointerMove { position: center });
    let id = overlays.tips(button, "Helpful hint");
    overlays.layout(&host_ui, viewport);
    assert!(overlays.is_open(id));

    // Pointer leaves -> the tip closes.
    host_ui.handle_input(&InputEvent::PointerLeave);
    overlays.layout(&host_ui, viewport);
    assert!(!overlays.is_open(id));
}

#[test]
fn message_auto_dismisses_and_fires_on_close() {
    let (host_ui, _button) = host();
    let viewport = Viewport::new(Size::new(400.0, 300.0));
    let mut overlays = Overlays::new(Theme::dark());
    let closed = Rc::new(Cell::new(false));
    let flag = closed.clone();
    let id = overlays.message("Saved");
    overlays.on_close(id, move || flag.set(true));
    overlays.layout(&host_ui, viewport);

    overlays.update(1.0);
    assert!(overlays.is_open(id));
    overlays.update(MESSAGE_DURATION);
    assert!(!overlays.is_open(id));
    assert!(closed.get());
}

#[test]
fn clicking_confirm_fires_the_callback_and_closes() {
    let (host_ui, _button) = host();
    let viewport = Viewport::new(Size::new(400.0, 300.0));
    let mut overlays = Overlays::new(Theme::dark());
    let fired = Rc::new(Cell::new(false));
    let flag = fired.clone();
    let id = overlays.confirm("Title", "Body");
    overlays.on_confirm(id, move || flag.set(true));
    overlays.layout(&host_ui, viewport);

    // `draw_components::Button` is a flex row with a label; find the confirm label and
    // click the row that owns it.
    let label = overlays
        .ui
        .tree()
        .iter_visible()
        .find(|id| overlays.ui.widget(*id).and_then(|w| w.text()) == Some("OK"))
        .expect("confirm label");
    let button = overlays.ui.tree().parent(label).unwrap_or(label);
    let center = overlays.ui.control(button).unwrap().rect.center();
    for event in [
        InputEvent::PointerDown {
            position: center,
            button: PointerButton::Left,
        },
        InputEvent::PointerUp {
            position: center,
            button: PointerButton::Left,
        },
    ] {
        overlays.handle_input(&event);
    }
    assert!(fired.get(), "on_confirm did not fire");
    assert!(!overlays.is_open(id));
}

#[test]
fn close_invokes_on_close_once() {
    let (host_ui, _button) = host();
    let viewport = Viewport::new(Size::new(400.0, 300.0));
    let mut overlays = Overlays::new(Theme::dark());
    let closed = Rc::new(Cell::new(0));
    let counter = closed.clone();
    let id = overlays.confirm("Title", "Body");
    overlays.on_close(id, move || counter.set(counter.get() + 1));
    overlays.layout(&host_ui, viewport);

    overlays.close(id);
    overlays.close(id); // already gone
    assert_eq!(closed.get(), 1);
}
