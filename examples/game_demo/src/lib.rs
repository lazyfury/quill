//! `game_demo` — a small top-down collect game built on the quill game layer.
//!
//! The world lives in a [`GameView`] embedded in a `draw_ui` HUD: a player
//! sprite (animated from an embedded PNG atlas) moves with the arrow keys /
//! WASD, a `Camera2D` follows, and coins spawn on a timer; touching a coin fires
//! an `Area` `on_enter` that scores. Movement runs at a fixed step via
//! [`FixedTimestep`], while the view renders its own offscreen target that the
//! HUD composites.
//!
//! Hosts call the pipeline in order: [`Game::layout`] -> [`Game::advance`] ->
//! [`Game::paint`], and use [`Game::needs_frame`] to sleep when idle.

mod assets;
mod selfcheck;

pub use selfcheck::run_selfcheck;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use draw_anim::{Easing, TweenSpec};
use draw_components::{Component, Flex, NodeRef, Panel, Text};
use draw_core::{Color, Edges, InputEvent, Key, NodeId, Size, Vec2, ViewportSize};
use draw_game::{
    upload_texture, Area, CollisionShape, FixedTimestep, GameView, Sprite, SpriteFrames,
};
use draw_render::{PaintContext, RenderBackend, RenderTargetId, TextureId};
use draw_scene::{SceneChild, SceneTree, Visual};
use draw_theme::{default_theme, space, Mode, SurfaceLevel, Theme};
use draw_ui::{self, TextMeasurer};

/// Crate name, kept for lightweight smoke checks.
pub const CRATE: &str = "game_demo";

/// The render target the `GameView` renders into.
///
/// A render target shares the `TextureId` space (see [`RenderTargetId`]), so it
/// must use a number disjoint from the uploaded textures below.
///
/// [`RenderTargetId`]: draw_render::RenderTargetId
pub const TARGET: RenderTargetId = RenderTargetId::from_raw(100);

const PLAYER_TEXTURE: TextureId = TextureId::new(1);
const COIN_TEXTURE: TextureId = TextureId::new(2);

const ARENA: f32 = 520.0;
const PLAYER_SPEED: f32 = 150.0;
const PLAYER_RADIUS: f32 = 9.0;
const COIN_RADIUS: f32 = 9.0;
const COIN_LIMIT: u32 = 8;
const SPAWN_INTERVAL: f32 = 0.7;
const HUD_HEIGHT: f32 = 34.0;

#[derive(Default, Clone, Copy)]
struct Keys {
    left: bool,
    right: bool,
    up: bool,
    down: bool,
}

/// The demo's whole state: the UI/HUD tree and the embedded game view.
pub struct Game {
    theme: &'static dyn Theme,
    ui: SceneTree,
    view: GameView,
    score: Rc<Cell<u32>>,
    keys: Rc<RefCell<Keys>>,
    clock: FixedTimestep,
    collect_marks: Rc<RefCell<Vec<NodeId>>>,
    pickups: Vec<NodeId>,
    player: Option<NodeId>,
    camera: Option<NodeId>,
    view_control: Option<NodeId>,
    score_label: Option<NodeId>,
    player_frames: Option<SpriteFrames>,
    painting_walk: bool,
    spawn_accum: f32,
    spawned: u32,
    viewport: ViewportSize,
    painted_generation: Cell<u64>,
}

impl Game {
    /// Builds the state (no backend needed yet). Call [`Game::init`] with a
    /// backend before the first frame.
    pub fn new() -> Self {
        Self::with_theme(default_theme(Mode::Dark))
    }

    /// Builds the state with an explicit theme.
    pub fn with_theme(theme: &'static dyn Theme) -> Self {
        Self {
            theme,
            ui: SceneTree::new(),
            view: GameView::new(TARGET),
            score: Rc::new(Cell::new(0)),
            keys: Rc::new(RefCell::new(Keys::default())),
            clock: FixedTimestep::from_hz(120.0).max_steps(8),
            collect_marks: Rc::new(RefCell::new(Vec::new())),
            pickups: Vec::new(),
            player: None,
            camera: None,
            view_control: None,
            score_label: None,
            player_frames: None,
            painting_walk: false,
            spawn_accum: 0.0,
            spawned: 0,
            viewport: ViewportSize::new(Size::new(640.0, 480.0)),
            painted_generation: Cell::new(u64::MAX),
        }
    }

    /// Decodes/upload the art, builds the world and mounts the HUD.
    pub fn init<B: RenderBackend>(&mut self, backend: &mut B) -> Result<(), B::Error> {
        let player_image =
            draw_assets::decode_png(assets::PLAYER_SHEET).expect("player sheet decodes");
        let coin_image = draw_assets::decode_png(assets::COIN).expect("coin decodes");
        upload_texture(backend, PLAYER_TEXTURE, &player_image)?;
        upload_texture(backend, COIN_TEXTURE, &coin_image)?;
        self.player_frames = Some(
            SpriteFrames::from_grid(
                draw_core::Rect::from_min_size(Vec2::ZERO, Size::new(64.0, 16.0)),
                4,
                1,
                4,
            )
            .fps(9.0),
        );

        let root = self.view.world().root();
        let arena = self.view.world_mut().add_node2d(root, "arena");
        self.view.world_mut().set_visual(
            arena,
            Visual::Rect {
                size: Size::splat(ARENA),
                color: self.theme.surface(SurfaceLevel::Base),
            },
        );
        self.view
            .world_mut()
            .set_position(arena, Vec2::splat(-ARENA * 0.5));

        let camera = self.view.world_mut().add_camera_2d(root, "camera");
        self.view.world_mut().set_camera_current(camera, true);
        self.camera = Some(camera);

        let player = self.view.world_mut().add_child(
            root,
            Sprite::new(PLAYER_TEXTURE, Size::splat(16.0)).named("player"),
        );
        self.player = Some(player);
        self.view.world_mut().set_scale(player, Vec2::ZERO);
        self.view
            .areas()
            .add(Area::new(player, CollisionShape::circle(PLAYER_RADIUS)));
        self.view.animator().tween_scale(
            player,
            Vec2::splat(1.0),
            TweenSpec::new(0.45).easing(Easing::BackOut),
        );

        for index in 0..4 {
            self.add_pickup_at(pickup_position(index as f32));
        }

        self.build_ui();
        Ok(())
    }

    /// Installs `measurer` for the HUD.
    pub fn set_text_measurer(&mut self, measurer: Rc<dyn TextMeasurer>) {
        draw_ui::set_text_measurer(&mut self.ui, measurer);
    }

    /// Device-pixel ratio for the game view's offscreen target.
    pub fn set_scale_factor(&mut self, scale: f32) {
        self.view.set_scale_factor(scale);
    }

    /// Resolves the HUD layout for `viewport`.
    pub fn layout(&mut self, viewport: ViewportSize) {
        self.viewport = viewport;
        draw_ui::layout(&mut self.ui, viewport);
        self.ui.update();
    }

    /// Advances the fixed-step world and renders the view for `dt` seconds.
    pub fn advance<B: RenderBackend>(&mut self, dt: f32, backend: &mut B) -> Result<(), B::Error> {
        let tick = self.clock.advance(dt);
        for _ in 0..tick.steps {
            self.physics_step(self.clock.step());
        }

        self.spawn_accum += dt;
        if self.spawned < COIN_LIMIT && self.spawn_accum >= SPAWN_INTERVAL {
            self.spawn_accum = 0.0;
            let position = pickup_position(self.spawned as f32 + 4.0);
            self.add_pickup_at(position);
        }

        self.sync_walk_animation();

        let control_size = self
            .view_control
            .and_then(|id| draw_ui::control(&self.ui, id))
            .map_or(self.viewport.logical_size(), |control| control.rect.size);
        self.view.set_viewport(ViewportSize::new(control_size));
        self.view.update(dt, backend)?;

        let collected = std::mem::take(&mut *self.collect_marks.borrow_mut());
        for id in collected {
            self.view.world_mut().remove(id);
            self.pickups.retain(|pickup| *pickup != id);
        }

        if let Some(label) = self.score_label {
            draw_components::set_text(&mut self.ui, label, format!("Score: {}", self.score.get()));
        }
        Ok(())
    }

    /// Routes an input event to the key state.
    pub fn event(&mut self, event: &InputEvent) {
        let (key, pressed) = match event {
            InputEvent::KeyDown { key } => (*key, true),
            InputEvent::KeyUp { key } => (*key, false),
            _ => return,
        };
        let mut keys = self.keys.borrow_mut();
        match key {
            Key::ArrowLeft | Key::Character('a') => keys.left = pressed,
            Key::ArrowRight | Key::Character('d') => keys.right = pressed,
            Key::ArrowUp | Key::Character('w') => keys.up = pressed,
            Key::ArrowDown | Key::Character('s') => keys.down = pressed,
            _ => {}
        }
    }

    /// Paints the HUD (and composites the game view) into `ctx`.
    pub fn paint(&self, ctx: &mut PaintContext) {
        draw_ui::paint(&self.ui, ctx);
        self.painted_generation
            .set(draw_ui::paint_generation(&self.ui));
    }

    /// Whether another frame is needed (view animating, HUD dirty, unpainted).
    pub fn needs_frame(&self) -> bool {
        self.view.needs_frame()
            || draw_ui::needs_layout(&self.ui)
            || self.ui.needs_update()
            || self.painted_generation.get() != draw_ui::paint_generation(&self.ui)
    }

    /// Current score.
    pub fn score(&self) -> u32 {
        self.score.get()
    }

    /// The player's world position.
    pub fn player_position(&self) -> Vec2 {
        self.player
            .and_then(|id| self.view.world().position(id))
            .unwrap_or(Vec2::ZERO)
    }

    /// Adds a coin at `position` (also used by `--selfcheck`).
    pub fn add_pickup_at(&mut self, position: Vec2) -> NodeId {
        let root = self.view.world().root();
        let id = self.view.world_mut().add_child(
            root,
            Sprite::new(COIN_TEXTURE, Size::splat(16.0))
                .named("coin")
                .position(position),
        );
        let score = self.score.clone();
        let marks = self.collect_marks.clone();
        let player = self.player;
        self.view
            .areas()
            .add(
                Area::new(id, CollisionShape::circle(COIN_RADIUS)).on_enter(move |other| {
                    if Some(other) == player {
                        score.set(score.get() + 1);
                        marks.borrow_mut().push(id);
                    }
                }),
            );
        self.pickups.push(id);
        self.spawned += 1;
        id
    }

    /// The UI tree (HUD + the mounted game view control).
    pub fn tree(&self) -> &SceneTree {
        &self.ui
    }

    fn build_ui(&mut self) {
        let theme = self.theme;
        let view_slot = NodeRef::new();
        let score_slot = NodeRef::new();

        let tree = Flex::column()
            .child(
                Panel::new()
                    .color(Color::TRANSPARENT)
                    .flat()
                    .grow(1.0)
                    .clip(true)
                    .child(
                        Panel::new()
                            .color(Color::TRANSPARENT)
                            .flat()
                            .grow(1.0)
                            .ref_(&view_slot),
                    ),
            )
            .child(
                Flex::row()
                    .min_size(0.0, HUD_HEIGHT)
                    .padding(Edges::new(space::MD, space::XS, space::MD, space::XS))
                    .child(Text::subheading("Score: 0", theme).ref_(&score_slot)),
            )
            .into_tree();

        self.ui = tree;
        let parent = view_slot.get().expect("view slot mounted");
        self.view_control = Some(self.view.mount(&mut self.ui, parent));
        self.score_label = score_slot.get();
    }

    fn physics_step(&mut self, step: f32) {
        let keys = *self.keys.borrow();
        let mut direction = Vec2::ZERO;
        if keys.left {
            direction.x -= 1.0;
        }
        if keys.right {
            direction.x += 1.0;
        }
        if keys.up {
            direction.y -= 1.0;
        }
        if keys.down {
            direction.y += 1.0;
        }
        let direction = direction.normalize_or_zero();

        let limit = ARENA * 0.5 - 8.0;
        if let Some(player) = self.player {
            let current = self.view.world().position(player).unwrap_or(Vec2::ZERO);
            let next = current + direction * (PLAYER_SPEED * step);
            let next = Vec2::new(next.x.clamp(-limit, limit), next.y.clamp(-limit, limit));
            self.view.world_mut().set_position(player, next);
            if let Some(camera) = self.camera {
                self.view.world_mut().set_position(camera, next);
            }
        }
    }

    fn sync_walk_animation(&mut self) {
        let moving = {
            let keys = *self.keys.borrow();
            keys.left || keys.right || keys.up || keys.down
        };
        let Some(player) = self.player else {
            return;
        };
        if moving && !self.painting_walk {
            if let Some(frames) = self.player_frames.clone() {
                self.view.play_animation(player, frames);
                self.painting_walk = true;
            }
        } else if !moving && self.painting_walk {
            self.view.stop_animation(player);
            self.painting_walk = false;
        }
    }
}

impl Default for Game {
    fn default() -> Self {
        Self::new()
    }
}

/// A deterministic spawn position for the `index`-th coin.
fn pickup_position(index: f32) -> Vec2 {
    let half = ARENA * 0.5 - 40.0;
    Vec2::new(
        ((index * 137.0) % ARENA) - half,
        ((index * 271.0) % ARENA) - half,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use draw_backend_recording::RecordingBackend;
    use draw_ui::FixedWidthTextMeasurer;

    #[test]
    fn crate_identity() {
        assert_eq!(CRATE, "game_demo");
    }

    #[test]
    fn a_frame_renders_the_view_and_composites_the_hud() {
        let mut game = Game::new();
        game.set_text_measurer(Rc::new(FixedWidthTextMeasurer::default()));
        let mut backend = RecordingBackend::new();
        game.init(&mut backend).unwrap();

        let viewport = ViewportSize::new(Size::new(640.0, 480.0));
        game.layout(viewport);
        game.advance(1.0 / 60.0, &mut backend).unwrap();

        assert!(backend.render_target(TARGET).is_some());
        assert_eq!(backend.target_frame_count(TARGET), 1);

        let mut ctx = PaintContext::new();
        game.paint(&mut ctx);
        let list = ctx.into_draw_list();
        assert!(
            list.commands().iter().any(|command| matches!(
                command,
                draw_render::DrawCommand::DrawImage { texture, .. }
                    if *texture == TARGET.texture()
            )),
            "the HUD should composite the game view target"
        );
    }

    #[test]
    fn walking_into_a_coin_scores() {
        let mut game = Game::new();
        let mut backend = RecordingBackend::new();
        game.init(&mut backend).unwrap();
        game.layout(ViewportSize::new(Size::new(640.0, 480.0)));

        game.add_pickup_at(Vec2::new(48.0, 0.0));
        game.event(&InputEvent::KeyDown {
            key: Key::ArrowRight,
        });
        for _ in 0..120 {
            game.advance(1.0 / 60.0, &mut backend).unwrap();
        }
        game.event(&InputEvent::KeyUp {
            key: Key::ArrowRight,
        });

        assert!(game.player_position().x > 0.0, "the player moved right");
        assert!(game.score() >= 1, "the player collected a coin");
    }
}
