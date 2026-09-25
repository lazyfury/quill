//! Headless verification for `game_demo`: builds the world, drives synthetic
//! input through the real pipeline into a `RecordingBackend`, and asserts the
//! outcome. No window and no screenshot (see `AGENTS.md`).

use std::rc::Rc;

use draw_backend_recording::RecordingBackend;
use draw_core::{InputEvent, Key, Size, Vec2, ViewportSize};
use draw_render::{DrawCommand, PaintContext};
use draw_ui::FixedWidthTextMeasurer;

use crate::{Game, TARGET};

/// Runs the self-check, returning a description of the first failure.
pub fn run_selfcheck() -> Result<(), String> {
    let mut game = Game::new();
    game.set_text_measurer(Rc::new(FixedWidthTextMeasurer::default()));
    let mut backend = RecordingBackend::new();
    game.init(&mut backend)
        .map_err(|error| format!("init: {error:?}"))?;

    let viewport = ViewportSize::new(Size::new(640.0, 480.0));
    game.layout(viewport);

    // A coin directly in the player's path, then walk right into it.
    game.add_pickup_at(Vec2::new(48.0, 0.0));
    game.event(&InputEvent::KeyDown {
        key: Key::ArrowRight,
    });
    let start = game.player_position();

    for _ in 0..120 {
        game.layout(viewport);
        game.advance(1.0 / 60.0, &mut backend)
            .map_err(|error| format!("advance: {error:?}"))?;
        let mut ctx = PaintContext::new();
        game.paint(&mut ctx);
        let _ = ctx.into_draw_list();
    }
    game.event(&InputEvent::KeyUp {
        key: Key::ArrowRight,
    });

    if game.player_position().x <= start.x {
        return Err("the player did not move right".into());
    }
    if game.score() == 0 {
        return Err("the player never collected a coin".into());
    }
    if backend.render_target(TARGET).is_none() {
        return Err("the game view never created its render target".into());
    }
    if backend.target_frame_count(TARGET) == 0 {
        return Err("the game view never rendered into its target".into());
    }

    let mut ctx = PaintContext::new();
    game.paint(&mut ctx);
    let list = ctx.into_draw_list();
    if !list.commands().iter().any(|command| {
        matches!(
            command,
            DrawCommand::DrawImage { texture, .. } if *texture == TARGET.texture()
        )
    }) {
        return Err("the HUD did not composite the game view target".into());
    }

    println!(
        "game_demo selfcheck ok: score={}, target_frames={}, player_x={:.1}",
        game.score(),
        backend.target_frame_count(TARGET),
        game.player_position().x
    );
    Ok(())
}
