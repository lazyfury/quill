//! The demo content: a `Node2D` scene plus a composed `Ui`.
//!
//! Deliberately identical in spirit to `demos/component_demo` and
//! `demos/web_demo`: the same `SceneTree` + `Ui` + component code that the
//! Canvas backend renders in a browser is painted here into the wgpu backend.

use std::cell::Cell;
use std::rc::Rc;

use draw_core::{Color, Edges, EventResult, InputEvent, NodeId, Rect, Size, Vec2, Viewport};
use draw_render::{Paint, PaintContext, TextAlign};
use draw_scene::{SceneTree, Visual};
use draw_ui::{Button, Label, Panel, Ui, VBox};

const BACKGROUND: Color = Color::new(0.09, 0.10, 0.13, 1.0);
const ACCENT: Color = Color::new(0.30, 0.62, 0.98, 1.0);
const WARN: Color = Color::new(0.98, 0.66, 0.25, 1.0);
const TEXT: Color = Color::new(0.92, 0.94, 0.98, 1.0);

/// Application state shared by the window runner.
pub struct Demo {
    ui: Ui,
    status: NodeId,
    clicks: Rc<Cell<u32>>,
    scene: SceneTree,
    rotating: NodeId,
    viewport: Viewport,
    time: f32,
}

impl Demo {
    pub fn new() -> Self {
        // --- UI: compose components ---------------------------------------
        let mut ui = Ui::new();

        let panel = ui.add(ui.root(), Panel::new());
        ui.set_anchors(panel.id(), Edges::new(1.0, 0.0, 1.0, 0.0));
        ui.set_offsets(panel.id(), Edges::new(-360.0, 40.0, -40.0, 320.0));

        let vbox = ui.add(panel.id(), VBox::new().separation(12.0));
        ui.add(vbox.id(), Label::new("Hello wgpu"));
        ui.add(
            vbox.id(),
            Label::new("quill — native wgpu backend")
                .font_size(14.0)
                .color(Color::new(0.70, 0.75, 0.85, 1.0)),
        );

        let clicks = Rc::new(Cell::new(0));
        let counter = clicks.clone();
        let _button = ui.add(
            vbox.id(),
            Button::new("Click me").on_click(move || counter.set(counter.get() + 1)),
        );
        let status = ui.add(vbox.id(), Label::new("Status: Clicked 0 times"));

        // --- Scene: a rotated Node2D with a child -------------------------
        let mut scene = SceneTree::new();
        let root = scene.root();
        let rotating = scene.add_node2d(root, "Rotating");
        scene.set_visual(
            rotating,
            Visual::Rect {
                size: Size::new(120.0, 80.0),
                color: ACCENT,
            },
        );
        let child = scene.add_node2d(rotating, "Child");
        scene.set_position(child, Vec2::new(80.0, 0.0));
        scene.set_visual(
            child,
            Visual::Circle {
                radius: 16.0,
                color: WARN,
            },
        );
        scene.update();

        Self {
            ui,
            status: status.id(),
            clicks,
            scene,
            rotating,
            viewport: Viewport::new(Size::new(900.0, 620.0)),
            time: 0.0,
        }
    }

    /// Advances the animation and updates text for the new viewport.
    ///
    /// UI layout is deliberately *not* performed here: the host times it as a
    /// separate pipeline phase via [`Demo::layout`].
    pub fn update(&mut self, viewport: Viewport, dt: f32) {
        self.viewport = viewport;
        self.time += dt;

        let size = viewport.logical_size();
        self.scene.set_position(
            self.rotating,
            Vec2::new(size.width * 0.30, size.height * 0.42),
        );
        self.scene.set_rotation(self.rotating, self.time);
        self.scene.update();

        self.ui.set_text(
            self.status,
            format!("Status: Clicked {} times", self.clicks.get()),
        );
    }

    /// Resolves UI layout for `viewport` (the timed *layout* pipeline phase).
    pub fn layout(&mut self, viewport: Viewport) {
        self.ui.layout(viewport);
    }

    /// Scene nodes in the built-in demo scene.
    pub fn scene_node_count(&self) -> usize {
        self.scene.node_count()
    }

    /// Controls in the demo UI.
    pub fn control_count(&self) -> usize {
        self.ui.control_count()
    }

    /// The demo's UI tree, used by the component debug overlay.
    pub fn ui(&self) -> &Ui {
        &self.ui
    }

    /// Emits this frame's `DrawList` into `ctx`.
    pub fn paint(&self, ctx: &mut PaintContext) {
        let size = self.viewport.logical_size();
        ctx.fill_rect(Rect::from_min_size(Vec2::ZERO, size), BACKGROUND);

        self.scene.paint(ctx);
        self.ui.paint(ctx);

        ctx.draw_text(
            "Scene / Node2D Demo",
            Vec2::new(40.0, 60.0),
            18.0,
            TextAlign::Left,
            Paint::new(TEXT.with_alpha(0.85)),
        );
        ctx.draw_text(
            "quill — wgpu Demo",
            Vec2::new(size.width * 0.5, size.height - 32.0),
            20.0,
            TextAlign::Center,
            Paint::new(TEXT),
        );
    }

    /// Routes a backend-neutral input event through the UI.
    pub fn event(&mut self, event: &InputEvent) -> EventResult {
        self.ui.handle_input(event)
    }
}
