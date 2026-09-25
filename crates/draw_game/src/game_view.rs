//! `GameView`: an embedded game viewport hosted in a `draw_ui` tree.
//!
//! A `GameView` owns a whole 2D world (a sub-[`SceneTree`]), its animation
//! runners and collision, renders that world into its own **offscreen render
//! target**, and composites the result into a `Control` in the host UI. Moving
//! the camera or animating the world therefore changes only the target's
//! contents; the surrounding UI stays cacheable, and the game keeps its own
//! cadence via [`GameView::needs_frame`].
//!
//! Gated behind `draw_game`'s optional `ui` feature, so a UI-less game never
//! compiles the UI crates.
//!
//! ```ignore
//! let mut view = GameView::new(RenderTargetId::from_raw(1));
//! view.set_scale_factor(backend.scale_factor());
//! view.mount(&mut ui_tree, ui_tree.root());
//!
//! // each frame, after `draw_ui::layout` and before `draw_ui::paint`:
//! view.set_viewport(control_size);
//! view.update(dt, &mut backend)?;
//! if view.needs_frame() { /* request another frame */ }
//! ```

use draw_anim::Animator;
use draw_core::{Color, NodeId, Size, ViewportSize};
use draw_render::{Paint, PaintContext, RenderBackend, RenderTargetId};
use draw_scene::SceneTree;
use draw_ui::{add_decor, foreground_decor, Control, ControlData, Widget};

use crate::clock::FixedTimestep;
use crate::{Areas, SpriteAnimations, Timers};

/// An embedded game viewport: sub-tree + runners + offscreen target + UI control.
pub struct GameView {
    world: SceneTree,
    anim: Animator,
    sprites: SpriteAnimations,
    timers: Timers,
    areas: Areas,
    target: RenderTargetId,
    control: Option<NodeId>,
    viewport: ViewportSize,
    scale: f32,
    /// Device size the target was last created at.
    created: Option<(u32, u32)>,
    /// Optional fixed-step clock; when set, `update` runs `physics_process`.
    clock: Option<FixedTimestep>,
    alpha: f32,
}

impl GameView {
    /// A game view rendering into `target` (create it with
    /// [`RenderBackend::create_render_target`](draw_render::RenderBackend::create_render_target)
    /// on the first update).
    pub fn new(target: RenderTargetId) -> Self {
        Self {
            world: SceneTree::new(),
            anim: Animator::new(),
            sprites: SpriteAnimations::new(),
            timers: Timers::new(),
            areas: Areas::new(),
            target,
            control: None,
            viewport: ViewportSize::new(Size::ZERO),
            scale: 1.0,
            created: None,
            clock: None,
            alpha: 0.0,
        }
    }

    /// Mounts the compositing `Control` under `parent` and returns its id.
    ///
    /// The control fills its parent and paints the render target as a
    /// foreground image, so it is laid out and composited by the normal
    /// `draw_ui` pipeline.
    pub fn mount(&mut self, host: &mut SceneTree, parent: NodeId) -> NodeId {
        let id = host.add_control(parent, "GameView");
        host.set_data(
            id,
            Control::new(
                ControlData::fill_parent(),
                Widget::Panel {
                    color: Color::TRANSPARENT,
                    border: None,
                },
            ),
        );
        let texture = self.target.texture();
        add_decor(
            host,
            id,
            foreground_decor(move |ctx, rect, _state| {
                ctx.draw_image(texture, rect, None, Paint::default());
            }),
        );
        self.control = Some(id);
        id
    }

    /// The compositing control, once [`GameView::mount`] has run.
    pub fn control(&self) -> Option<NodeId> {
        self.control
    }

    /// The offscreen target this view renders into.
    pub fn target(&self) -> RenderTargetId {
        self.target
    }

    pub fn world(&self) -> &SceneTree {
        &self.world
    }

    pub fn world_mut(&mut self) -> &mut SceneTree {
        &mut self.world
    }

    pub fn animator(&mut self) -> &mut Animator {
        &mut self.anim
    }

    pub fn sprite_animations(&mut self) -> &mut SpriteAnimations {
        &mut self.sprites
    }

    pub fn timers(&mut self) -> &mut Timers {
        &mut self.timers
    }

    pub fn areas(&mut self) -> &mut Areas {
        &mut self.areas
    }

    /// The viewport's logical size (the control's rect in the UI).
    pub fn viewport(&self) -> ViewportSize {
        self.viewport
    }

    /// Sets the viewport's logical size; the target is recreated at the next
    /// update if the device size changed.
    pub fn set_viewport(&mut self, viewport: ViewportSize) {
        self.viewport = viewport;
    }

    pub fn scale_factor(&self) -> f32 {
        self.scale
    }

    /// Device-pixel ratio; the target is `logical * scale` device pixels.
    pub fn set_scale_factor(&mut self, scale: f32) {
        self.scale = scale.max(0.0);
    }

    /// Runs the world at a fixed `hz` (via `physics_process`) inside
    /// [`GameView::update`]; `process(dt)` still runs once per real frame.
    pub fn set_fixed_step(&mut self, hz: f32) {
        self.clock = Some(FixedTimestep::from_hz(hz));
    }

    /// Disables the fixed step (only the variable `process(dt)` runs).
    pub fn clear_fixed_step(&mut self) {
        self.clock = None;
    }

    /// The interpolation alpha from the last [`GameView::update`] (the fraction
    /// of the next fixed step already elapsed; `0.0` without a fixed step).
    pub fn interpolation_alpha(&self) -> f32 {
        self.alpha
    }

    /// Whether another frame is needed (a running runner or a stale world).
    pub fn needs_frame(&self) -> bool {
        self.anim.is_animating()
            || self.sprites.is_animating()
            || self.timers.is_animating()
            || self.world.needs_update()
    }

    /// Advances the world and renders it into the target.
    ///
    /// Call after the host's `draw_ui::layout` (so the viewport size is known)
    /// and before `draw_ui::paint` (which composites the target).
    pub fn update<B: RenderBackend>(&mut self, dt: f32, backend: &mut B) -> Result<(), B::Error> {
        self.world.set_viewport_size(self.viewport.logical_size());

        if let Some(clock) = self.clock.as_mut() {
            let tick = clock.advance(dt);
            let step = clock.step();
            for _ in 0..tick.steps {
                self.world.physics_process(step);
            }
            self.alpha = tick.alpha;
        } else {
            self.alpha = 0.0;
        }

        self.anim.update(dt, &mut self.world);
        self.sprites.update(dt, &mut self.world);
        self.timers.update(dt);
        self.world.process(dt);
        self.world.update();
        self.areas.update(&self.world);

        let device = self.device_size();
        if self.created != Some(device) {
            backend.create_render_target(self.target, device.0, device.1)?;
            self.created = Some(device);
        }

        let mut ctx = PaintContext::new();
        self.world.paint(&mut ctx);
        backend.render_to_target(self.target, &ctx.into_draw_list())
    }

    fn device_size(&self) -> (u32, u32) {
        let scale = if self.scale > 0.0 { self.scale } else { 1.0 };
        let size = self.viewport.logical_size();
        (
            (size.width * scale).round().max(1.0) as u32,
            (size.height * scale).round().max(1.0) as u32,
        )
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use draw_backend_recording::RecordingBackend;
    use draw_core::Vec2;
    use draw_render::DrawCommand;
    use draw_scene::Visual;
    use draw_ui;

    use super::*;

    fn viewport(width: f32, height: f32) -> ViewportSize {
        ViewportSize::new(Size::new(width, height))
    }

    #[test]
    fn update_renders_the_world_and_the_ui_composites_it() {
        let mut host = SceneTree::new();
        let target = RenderTargetId::from_raw(120);
        let mut view = GameView::new(target);
        view.set_scale_factor(1.0);
        view.set_viewport(viewport(64.0, 48.0));

        let root = view.world().root();
        let background = view.world_mut().add_node2d(root, "background");
        view.world_mut().set_visual(
            background,
            Visual::Rect {
                size: Size::new(64.0, 48.0),
                color: Color::RED,
            },
        );

        let host_root = host.root();
        let control = view.mount(&mut host, host_root);
        assert!(draw_ui::control(&host, control).is_some());

        let mut backend = RecordingBackend::new();
        view.update(0.016, &mut backend).unwrap();
        let registered = backend.render_target(target).expect("target created");
        assert_eq!((registered.width, registered.height), (64, 48));
        assert_eq!(backend.target_frame_count(target), 1);

        draw_ui::layout(&mut host, viewport(200.0, 100.0));
        let mut ctx = PaintContext::new();
        draw_ui::paint(&host, &mut ctx);
        let list = ctx.into_draw_list();
        assert!(
            list.commands().iter().any(|command| matches!(
                command,
                DrawCommand::DrawImage { texture, .. } if *texture == target.texture()
            )),
            "the UI should composite the render target"
        );
    }

    #[test]
    fn the_target_tracks_logical_size_times_scale() {
        let mut view = GameView::new(RenderTargetId::from_raw(121));
        view.set_scale_factor(2.0);
        view.set_viewport(viewport(30.0, 20.0));

        let mut backend = RecordingBackend::new();
        view.update(0.016, &mut backend).unwrap();
        let registered = backend
            .render_target(RenderTargetId::from_raw(121))
            .unwrap();
        assert_eq!((registered.width, registered.height), (60, 40));

        // A viewport change recreates the target at the new size.
        view.set_viewport(viewport(10.0, 10.0));
        view.update(0.016, &mut backend).unwrap();
        let resized = backend
            .render_target(RenderTargetId::from_raw(121))
            .unwrap();
        assert_eq!((resized.width, resized.height), (20, 20));
    }

    #[test]
    fn needs_frame_tracks_world_dirtiness() {
        let mut view = GameView::new(RenderTargetId::from_raw(122));
        assert!(!view.needs_frame(), "a fresh world is clean");

        let root = view.world().root();
        let node = view.world_mut().add_node2d(root, "actor");
        view.world_mut().set_position(node, Vec2::new(1.0, 0.0));
        assert!(view.needs_frame());

        let mut backend = RecordingBackend::new();
        view.set_scale_factor(1.0);
        view.set_viewport(viewport(16.0, 16.0));
        view.update(0.016, &mut backend).unwrap();
        assert!(!view.needs_frame(), "clean after update");
    }

    #[test]
    fn a_fixed_step_runs_physics_at_the_configured_rate() {
        let mut view = GameView::new(RenderTargetId::from_raw(123));
        view.set_scale_factor(1.0);
        view.set_viewport(viewport(16.0, 16.0));
        view.set_fixed_step(50.0); // 20 ms per step

        let steps = Rc::new(Cell::new(0));
        let counter = steps.clone();
        let root = view.world().root();
        let node = view.world_mut().add_node2d(root, "physics");
        view.world_mut()
            .set_physics_process(node, move |_| counter.set(counter.get() + 1));

        let mut backend = RecordingBackend::new();
        view.update(0.05, &mut backend).unwrap();
        assert_eq!(steps.get(), 2, "50 ms is two 20 ms steps");
        assert!((view.interpolation_alpha() - 0.5).abs() < 1e-4);

        view.update(0.01, &mut backend).unwrap();
        assert_eq!(steps.get(), 3, "the leftover 10 ms plus 10 ms");
        assert!((view.interpolation_alpha() - 0.0).abs() < 1e-4);
    }
}
