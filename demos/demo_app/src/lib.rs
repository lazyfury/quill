//! Shared, backend-neutral demo application.
//!
//! Both the native `wgpu` demo and the browser (`wasm`) demos build and drive
//! this exact [`DemoApp`]: a rotated `Node2D` scene plus a UI that exercises the
//! layout engine (flex, grid, text wrapping, `flex_grow`).
//!
//! The app owns no window, backend or browser API. Hosts drive it through:
//!
//! ```text
//! Input -> DemoApp::event -> DemoApp::update -> DemoApp::layout
//!                                                   -> DemoApp::paint -> DrawList
//! ```
//!
//! Hosts may inject a text measurer (`DemoApp::ui_mut().set_text_measurer`); the
//! `wgpu` demo supplies one backed by the real font the backend loaded, while the
//! Canvas demo keeps the proportional default.

use std::cell::Cell;
use std::rc::Rc;

use draw_core::{Color, Edges, EventResult, InputEvent, NodeId, Rect, Size, Vec2, Viewport};
use draw_render::{Paint, PaintContext, TextAlign};
use draw_scene::{SceneTree, Visual};
use draw_ui::{Button, Flex, Grid, Label, Panel, Track, Ui};

const BACKGROUND: Color = Color::new(0.09, 0.10, 0.13, 1.0);
const ACCENT: Color = Color::new(0.30, 0.62, 0.98, 1.0);
const WARN: Color = Color::new(0.98, 0.66, 0.25, 1.0);
const TEXT: Color = Color::new(0.92, 0.94, 0.98, 1.0);
const MUTED: Color = Color::new(0.70, 0.75, 0.85, 1.0);

/// Application state shared by every demo host.
pub struct DemoApp {
    ui: Ui,
    scene: SceneTree,
    rotating: NodeId,
    panel: NodeId,
    column: NodeId,
    paragraph: NodeId,
    button: NodeId,
    reset_button: NodeId,
    grid: NodeId,
    cells: Vec<NodeId>,
    status: NodeId,
    clicks: Rc<Cell<u32>>,
    viewport: Viewport,
    time: f32,
}

impl Default for DemoApp {
    fn default() -> Self {
        Self::new()
    }
}

impl DemoApp {
    /// Builds the demo scene and UI. Text uses the default proportional
    /// measurer; inject a backend-matching one via [`DemoApp::ui_mut`] when
    /// needed (for example the wgpu bitmap font).
    pub fn new() -> Self {
        let mut ui = Ui::new();

        // A fixed-size card on the left with a flex column inside.
        let panel = ui.add(ui.root(), Panel::new());
        ui.set_anchors(panel.id(), Edges::new(0.0, 0.0, 0.0, 0.0));
        ui.set_offsets(panel.id(), Edges::new(40.0, 40.0, 400.0, 400.0));

        let column = ui.add(panel.id(), Flex::column().gap(12.0));

        ui.add(column.id(), Label::new("Hello quill").font_size(22.0));
        ui.add(
            column.id(),
            Label::new("flex + grid + wrapping showcase")
                .font_size(13.0)
                .color(MUTED),
        );
        // Soft-wraps to the available width, grows its height, and is clipped
        // to two lines with an ellipsis.
        let paragraph = ui.add(
            column.id(),
            Label::new(
                "This paragraph wraps to the available width. Flex measures the \
                 wrapped height, so the column grows instead of clipping it.",
            )
            .font_size(13.0)
            .max_lines(2)
            .ellipsis(true),
        );

        // A row of equal-width buttons: each grows to share leftover space.
        let row = ui.add(column.id(), Flex::row().gap(8.0).padding(Edges::ZERO));
        let clicks = Rc::new(Cell::new(0));
        let counter = clicks.clone();
        let button = ui.add(
            row.id(),
            Button::new("Click me").on_click(move || counter.set(counter.get() + 1)),
        );
        ui.set_flex_grow(button.id(), 1.0);
        let reset = clicks.clone();
        let reset_button = ui.add(
            row.id(),
            Button::new("Reset").on_click(move || reset.set(0)),
        );
        ui.set_flex_grow(reset_button.id(), 1.0);

        // A 2x2 grid with `Fr` tracks and auto rows.
        let grid = ui.add(
            column.id(),
            Grid::new(vec![Track::Fr(1.0), Track::Fr(2.0)])
                .gap(6.0)
                .padding(Edges::all(8.0)),
        );
        let mut cells = Vec::new();
        for (title, value) in [
            ("Flex", "grow / shrink"),
            ("Grid", "fr tracks"),
            ("Text", "wrap + ellipsis"),
            ("Order", "placement"),
        ] {
            let cell = ui.add(grid.id(), Flex::column().gap(2.0).padding(Edges::ZERO));
            ui.add(cell.id(), Label::new(title).font_size(12.0).color(ACCENT));
            ui.add(cell.id(), Label::new(value).font_size(11.0).color(MUTED));
            cells.push(cell.id());
        }

        let status = ui.add(
            column.id(),
            Label::new("Status: Clicked 0 times").font_size(13.0),
        );

        // --- Scene: a rotated Node2D with a circular child ----------------
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
            scene,
            rotating,
            panel: panel.id(),
            column: column.id(),
            paragraph: paragraph.id(),
            button: button.id(),
            reset_button: reset_button.id(),
            grid: grid.id(),
            cells,
            status: status.id(),
            clicks,
            viewport: Viewport::new(Size::new(900.0, 620.0)),
            time: 0.0,
        }
    }

    pub fn ui(&self) -> &Ui {
        &self.ui
    }

    /// Mutable UI access, e.g. to inject a text measurer or build overlays.
    pub fn ui_mut(&mut self) -> &mut Ui {
        &mut self.ui
    }

    pub fn scene(&self) -> &SceneTree {
        &self.scene
    }

    pub fn viewport(&self) -> Viewport {
        self.viewport
    }

    pub fn clicks(&self) -> u32 {
        self.clicks.get()
    }

    /// Center of the primary button in logical viewport coordinates.
    pub fn button_center(&self) -> Option<Vec2> {
        self.ui
            .control(self.button)
            .map(|control| control.rect.center())
    }

    pub fn panel(&self) -> NodeId {
        self.panel
    }

    pub fn column(&self) -> NodeId {
        self.column
    }

    pub fn paragraph(&self) -> NodeId {
        self.paragraph
    }

    pub fn button(&self) -> NodeId {
        self.button
    }

    pub fn reset_button(&self) -> NodeId {
        self.reset_button
    }

    pub fn grid(&self) -> NodeId {
        self.grid
    }

    pub fn grid_cells(&self) -> &[NodeId] {
        &self.cells
    }

    pub fn status(&self) -> NodeId {
        self.status
    }

    /// Advances the scene animation and refreshes state-driven text.
    ///
    /// UI layout is intentionally separate ([`DemoApp::layout`]) so hosts can
    /// time the layout phase.
    pub fn update(&mut self, viewport: Viewport, dt: f32) {
        self.viewport = viewport;
        self.time += dt;

        let size = viewport.logical_size();
        self.scene.set_position(
            self.rotating,
            Vec2::new(size.width * 0.72, size.height * 0.42),
        );
        self.scene.set_rotation(self.rotating, self.time);
        self.scene.update();

        self.ui.set_text(
            self.status,
            format!("Status: Clicked {} times", self.clicks.get()),
        );
    }

    /// Resolves UI layout for `viewport`.
    pub fn layout(&mut self, viewport: Viewport) {
        self.viewport = viewport;
        self.ui.layout(viewport);
    }

    /// Emits this frame's `DrawList` into `ctx`.
    pub fn paint(&self, ctx: &mut PaintContext) {
        let size = self.viewport.logical_size();
        ctx.fill_rect(Rect::from_min_size(Vec2::ZERO, size), BACKGROUND);

        self.scene.paint(ctx);
        self.ui.paint(ctx);

        ctx.draw_text(
            "Scene / Node2D + Layout Demo",
            Vec2::new(40.0, 60.0),
            18.0,
            TextAlign::Left,
            Paint::new(TEXT.with_alpha(0.85)),
        );
        ctx.draw_text(
            "quill — shared DemoApp",
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

    /// Scene nodes in the built-in demo scene.
    pub fn scene_node_count(&self) -> usize {
        self.scene.node_count()
    }

    /// Controls in the demo UI.
    pub fn control_count(&self) -> usize {
        self.ui.control_count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use draw_backend_recording::RecordingBackend;
    use draw_core::{PointerButton, Size};
    use draw_render::{DrawCommand, RenderBackend};

    fn laid_out(width: f32, height: f32) -> DemoApp {
        let viewport = Viewport::new(Size::new(width, height));
        let mut app = DemoApp::new();
        app.update(viewport, 0.016);
        app.layout(viewport);
        app
    }

    fn rect(app: &DemoApp, id: NodeId) -> Rect {
        app.ui().control(id).expect("control").rect
    }

    #[test]
    fn panel_and_children_stay_inside_viewport() {
        let app = laid_out(900.0, 620.0);
        let viewport = Rect::from_min_size(Vec2::ZERO, Size::new(900.0, 620.0));
        let panel = rect(&app, app.panel());
        assert!(
            viewport.contains_rect(panel),
            "panel {panel:?} escaped viewport"
        );
        assert_eq!(panel.size, Size::new(360.0, 360.0));

        for id in [
            app.column(),
            app.paragraph(),
            app.button(),
            app.reset_button(),
            app.grid(),
            app.status(),
        ] {
            assert!(
                panel.contains_rect(rect(&app, id)),
                "control {id:?} escaped the panel"
            );
        }
        for cell in app.grid_cells() {
            assert!(panel.contains_rect(rect(&app, *cell)));
        }
    }

    #[test]
    fn wrapping_label_is_clipped_to_two_lines() {
        let app = laid_out(900.0, 620.0);
        let line_h = draw_ui::layout::line_height(13.0);
        let paragraph = rect(&app, app.paragraph());
        assert!(paragraph.size.height > line_h, "paragraph did not wrap");
        assert!(
            paragraph.size.height <= line_h * 2.0 + 1e-3,
            "paragraph exceeded max_lines: {paragraph:?}"
        );
    }

    #[test]
    fn grow_buttons_share_the_row() {
        let app = laid_out(900.0, 620.0);
        let left = rect(&app, app.button());
        let right = rect(&app, app.reset_button());

        // Both are on the same row and together fill its content width.
        assert!((left.top() - right.top()).abs() < 1e-3);
        assert!(left.left() < right.left());
        let column = rect(&app, app.column());
        let row_width = column.size.width - 32.0; // column padding
        assert!(
            (left.size.width + right.size.width + 8.0 - row_width).abs() < 1e-3,
            "buttons did not fill the row: {} + {} + 8 != {row_width}",
            left.size.width,
            right.size.width
        );
        assert!(left.size.width > 0.0 && right.size.width > 0.0);
    }

    #[test]
    fn grid_cells_tile_without_overlap() {
        let app = laid_out(900.0, 620.0);
        let grid = rect(&app, app.grid());
        let cells = app.grid_cells();
        assert_eq!(cells.len(), 4);

        let rects: Vec<Rect> = cells.iter().map(|id| rect(&app, *id)).collect();
        for cell in &rects {
            assert!(
                grid.contains_rect(*cell) || grid.intersects(*cell),
                "cell {cell:?} outside grid {grid:?}"
            );
        }
        // No two cells overlap.
        for i in 0..rects.len() {
            for j in (i + 1)..rects.len() {
                assert!(
                    !rects[i].intersects(rects[j]),
                    "cells {i} and {j} overlap: {:?} / {:?}",
                    rects[i],
                    rects[j]
                );
            }
        }
        // Rows are stacked: cells 0/1 above cells 2/3.
        assert!(rects[0].bottom() <= rects[2].top() + 1e-3);
        assert!(rects[1].bottom() <= rects[3].top() + 1e-3);
    }

    #[test]
    fn click_updates_state_and_status_text() {
        let mut app = laid_out(900.0, 620.0);
        let center = app.button_center().expect("button rect");
        app.event(&InputEvent::PointerDown {
            position: center,
            button: PointerButton::Left,
        });
        app.event(&InputEvent::PointerUp {
            position: center,
            button: PointerButton::Left,
        });
        assert_eq!(app.clicks(), 1);

        let viewport = app.viewport();
        app.update(viewport, 0.016);
        app.layout(viewport);
        let status = app
            .ui()
            .widget(app.status())
            .and_then(|widget| widget.text())
            .expect("status text");
        assert!(status.contains('1'), "status was {status:?}");
    }

    #[test]
    fn resize_relayouts_and_keeps_panel_anchored() {
        let mut app = DemoApp::new();
        let narrow = Viewport::new(Size::new(700.0, 500.0));
        app.update(narrow, 0.016);
        app.layout(narrow);
        let panel = rect(&app, app.panel());
        assert_eq!(
            panel,
            Rect::from_min_size(Vec2::new(40.0, 40.0), Size::new(360.0, 360.0))
        );

        let wide = Viewport::new(Size::new(1100.0, 800.0));
        app.layout(wide);
        // Left-anchored panel keeps its offsets.
        assert_eq!(rect(&app, app.panel()), panel);
    }

    #[test]
    fn full_pipeline_records_a_draw_list_headlessly() {
        let app = laid_out(900.0, 620.0);
        let viewport = app.viewport();

        let mut ctx = PaintContext::new();
        app.paint(&mut ctx);
        let list = ctx.into_draw_list();

        let mut backend = RecordingBackend::new();
        backend.begin_frame(viewport).unwrap();
        backend.submit(&list).unwrap();
        backend.end_frame().unwrap();

        assert_eq!(backend.frame_count(), 1);
        let frame = backend.last_frame().expect("frame");
        assert_eq!(frame.viewport, viewport);

        let commands = frame.commands();
        assert!(commands
            .iter()
            .any(|command| matches!(command, DrawCommand::FillRect { .. })));
        assert!(commands
            .iter()
            .any(|command| matches!(command, DrawCommand::DrawText { .. })));
        assert!(commands
            .iter()
            .any(|command| matches!(command, DrawCommand::StrokeRect { .. })));
    }
}
