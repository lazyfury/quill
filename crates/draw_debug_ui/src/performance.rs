use draw_core::{Color, Edges, EventResult, InputEvent, NodeId, Viewport};
use draw_profile::{InspectionReport, Phase, Profiler, Severity};
use draw_render::PaintContext;
use draw_ui::{Label, Panel, Ui, VBox};

/// Which viewport corner the overlay panel is pinned to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Corner {
    TopLeft,
    #[default]
    TopRight,
    BottomLeft,
    BottomRight,
}

/// Appearance and placement of a [`PerformanceOverlay`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OverlayConfig {
    pub corner: Corner,
    /// Panel width in logical pixels.
    pub width: f32,
    /// Gap between the panel and the viewport edge.
    pub margin: f32,
    /// Inner padding of the panel's `VBox`.
    pub padding: f32,
    /// Vertical gap between rows.
    pub separation: f32,
    /// Font size for metric rows.
    pub font_size: f32,
    /// Font size for the title row.
    pub title_font_size: f32,
    /// How many finding rows to display.
    pub max_finding_rows: usize,
    pub background: Color,
    pub border: Color,
    pub text_color: Color,
    pub muted_color: Color,
    pub warn_color: Color,
    pub error_color: Color,
}

impl Default for OverlayConfig {
    fn default() -> Self {
        Self {
            corner: Corner::TopRight,
            width: 320.0,
            margin: 12.0,
            padding: 12.0,
            separation: 4.0,
            font_size: 13.0,
            title_font_size: 15.0,
            max_finding_rows: 4,
            background: Color::new(0.05, 0.06, 0.09, 0.94),
            border: Color::new(0.30, 0.62, 0.98, 0.85),
            text_color: Color::new(0.92, 0.94, 0.98, 1.0),
            muted_color: Color::new(0.66, 0.71, 0.82, 1.0),
            warn_color: Color::new(0.98, 0.72, 0.30, 1.0),
            error_color: Color::new(0.98, 0.42, 0.42, 1.0),
        }
    }
}

impl OverlayConfig {
    /// Number of text rows, including the reserved finding rows.
    pub fn row_count(&self) -> usize {
        // title, fps, frame, profiler, update/layout, paint/render, commands,
        // entities, findings, shortcuts
        10 + self.max_finding_rows
    }

    /// Total panel height implied by the row count.
    pub fn height(&self) -> f32 {
        let rows = self.row_count() as f32;
        let line = self.font_size * 1.4;
        self.padding * 2.0 + rows * line + (rows - 1.0) * self.separation + 4.0
    }

    /// Anchor/offset pair pinning the panel to [`OverlayConfig::corner`].
    fn placement(&self) -> (Edges, Edges) {
        let (w, h, m) = (self.width, self.height(), self.margin);
        match self.corner {
            Corner::TopLeft => (
                Edges::new(0.0, 0.0, 0.0, 0.0),
                Edges::new(m, m, m + w, m + h),
            ),
            Corner::TopRight => (
                Edges::new(1.0, 0.0, 1.0, 0.0),
                Edges::new(-m - w, m, -m, m + h),
            ),
            Corner::BottomLeft => (
                Edges::new(0.0, 1.0, 0.0, 1.0),
                Edges::new(m, -m - h, m + w, -m),
            ),
            Corner::BottomRight => (
                Edges::new(1.0, 1.0, 1.0, 1.0),
                Edges::new(-m - w, -m - h, -m, -m),
            ),
        }
    }
}

/// The last text rendered by the overlay, exposed for tests and hosts.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OverlayText {
    pub title: String,
    pub fps: String,
    pub frame: String,
    /// Profiler state (`on` / `paused`), so toggling it is visible.
    pub profiler: String,
    pub update_layout: String,
    pub paint_render: String,
    pub commands: String,
    pub entities: String,
    pub findings: String,
    pub finding_rows: Vec<String>,
    /// Key legend footer.
    pub shortcuts: String,
}

#[derive(Debug, Clone)]
struct Rows {
    title: NodeId,
    fps: NodeId,
    frame: NodeId,
    profiler: NodeId,
    update_layout: NodeId,
    paint_render: NodeId,
    commands: NodeId,
    entities: NodeId,
    findings: NodeId,
    finding_rows: Vec<NodeId>,
    shortcuts: NodeId,
}

/// A togglable performance panel drawn from a [`Profiler`] and an
/// [`InspectionReport`].
///
/// The overlay owns a private [`Ui`](draw_ui::Ui); hosts keep their own UI and
/// simply call [`update`](PerformanceOverlay::update) + [`paint`](PerformanceOverlay::paint)
/// after painting the application. When closed, `update` and `paint` are no-ops
/// and paint nothing.
pub struct PerformanceOverlay {
    ui: Ui,
    panel: NodeId,
    open: bool,
    config: OverlayConfig,
    rows: Rows,
    text: OverlayText,
}

impl Default for PerformanceOverlay {
    fn default() -> Self {
        Self::new()
    }
}

impl PerformanceOverlay {
    pub fn new() -> Self {
        Self::with_config(OverlayConfig::default())
    }

    pub fn with_config(config: OverlayConfig) -> Self {
        let mut ui = Ui::new();

        let panel = ui.add(
            ui.root(),
            Panel::new()
                .color(config.background)
                .border(Some(config.border)),
        );
        let (anchors, offsets) = config.placement();
        ui.set_anchors(panel.id(), anchors);
        ui.set_offsets(panel.id(), offsets);

        let vbox = ui.add(
            panel.id(),
            VBox::new()
                .separation(config.separation)
                .padding(Edges::all(config.padding)),
        );

        let text_color = config.text_color;
        let muted = config.muted_color;
        let title_size = config.title_font_size;
        let font_size = config.font_size;

        let add = |ui: &mut Ui, text: &str, size: f32, color: Color| -> NodeId {
            ui.add(vbox.id(), Label::new(text).font_size(size).color(color))
                .id()
        };

        let title = add(&mut ui, "Performance", title_size, text_color);
        let fps = add(&mut ui, "FPS --", font_size, text_color);
        let frame = add(&mut ui, "frame -- ms", font_size, text_color);
        let profiler = add(&mut ui, "profiler on  (F5 / o)", font_size, muted);
        let update_layout = add(&mut ui, "update --   layout --", font_size, muted);
        let paint_render = add(&mut ui, "paint --   render --", font_size, muted);
        let commands = add(&mut ui, "commands --", font_size, muted);
        let entities = add(&mut ui, "nodes --   controls --", font_size, muted);
        let findings = add(&mut ui, "findings none", font_size, muted);
        let finding_rows: Vec<NodeId> = (0..config.max_finding_rows)
            .map(|_| add(&mut ui, "(none)", font_size, muted))
            .collect();
        let shortcuts = add(
            &mut ui,
            "F3 / ` / d bounds   F4 / p panel   F5 / o profiler",
            font_size,
            muted,
        );

        Self {
            ui,
            panel: panel.id(),
            open: true,
            config,
            rows: Rows {
                title,
                fps,
                frame,
                profiler,
                update_layout,
                paint_render,
                commands,
                entities,
                findings,
                finding_rows,
                shortcuts,
            },
            text: OverlayText::default(),
        }
    }

    pub fn config(&self) -> &OverlayConfig {
        &self.config
    }

    /// The overlay's own UI tree.
    pub fn ui(&self) -> &Ui {
        &self.ui
    }

    /// The panel control id (root of the overlay's visible content).
    pub fn panel(&self) -> NodeId {
        self.panel
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Shows/hides the overlay and returns the new state.
    pub fn set_open(&mut self, open: bool) -> bool {
        self.open = open;
        self.open
    }

    /// Flips visibility and returns the new state.
    pub fn toggle(&mut self) -> bool {
        self.open = !self.open;
        self.open
    }

    /// The most recently computed text (independent of the UI tree).
    pub fn text(&self) -> &OverlayText {
        &self.text
    }

    /// Recomputes the panel text from `profiler`/`report` and lays it out.
    ///
    /// No-op while closed.
    pub fn update(&mut self, profiler: &Profiler, report: &InspectionReport, viewport: Viewport) {
        if !self.open {
            return;
        }
        let text = build_text(profiler, report, self.config.max_finding_rows);
        self.apply(&text);
        self.text = text;
        self.ui.layout(viewport);
    }

    /// Paints the overlay into `ctx`. No-op while closed.
    pub fn paint(&self, ctx: &mut PaintContext) {
        if self.open {
            self.ui.paint(ctx);
        }
    }

    /// Routes an input event through the overlay. Closed overlays ignore input.
    pub fn handle_input(&mut self, event: &InputEvent) -> EventResult {
        if self.open {
            self.ui.handle_input(event)
        } else {
            EventResult::Ignored
        }
    }

    fn apply(&mut self, text: &OverlayText) {
        self.ui.set_text(self.rows.title, text.title.clone());
        self.ui.set_text(self.rows.fps, text.fps.clone());
        self.ui.set_text(self.rows.frame, text.frame.clone());
        self.ui.set_text(self.rows.profiler, text.profiler.clone());
        self.ui
            .set_text(self.rows.update_layout, text.update_layout.clone());
        self.ui
            .set_text(self.rows.paint_render, text.paint_render.clone());
        self.ui.set_text(self.rows.commands, text.commands.clone());
        self.ui.set_text(self.rows.entities, text.entities.clone());
        self.ui.set_text(self.rows.findings, text.findings.clone());
        for (id, row) in self.rows.finding_rows.iter().zip(&text.finding_rows) {
            self.ui.set_text(*id, row.clone());
        }
        self.ui
            .set_text(self.rows.shortcuts, text.shortcuts.clone());
    }
}

fn build_text(profiler: &Profiler, report: &InspectionReport, max_rows: usize) -> OverlayText {
    let summary = profiler.summary();
    let last = profiler.last();

    let fps = match summary {
        Some(summary) => format!("FPS {:.0}", summary.fps()),
        None => "FPS --".to_string(),
    };

    let frame = match (last, summary) {
        (Some(last), Some(summary)) => format!(
            "frame {:.2} ms  avg {:.2}  max {:.2}",
            last.frame_ms, summary.avg_frame_ms, summary.max_frame_ms
        ),
        _ => "frame -- ms".to_string(),
    };

    let (update_layout, paint_render) = match summary {
        Some(summary) => (
            format!(
                "update {:.2} ms   layout {:.2} ms",
                summary.avg_stages.get(Phase::Update),
                summary.avg_stages.get(Phase::Layout)
            ),
            format!(
                "paint {:.2} ms   render {:.2} ms",
                summary.avg_stages.get(Phase::Paint),
                summary.avg_stages.get(Phase::Render)
            ),
        ),
        None => (
            "update --   layout --".to_string(),
            "paint --   render --".to_string(),
        ),
    };

    let commands = match (last, summary) {
        (Some(last), Some(summary)) => format!(
            "commands {}   max {}",
            last.counters.draw_commands, summary.max_draw_commands
        ),
        (Some(last), None) => format!("commands {}", last.counters.draw_commands),
        _ => "commands --".to_string(),
    };

    let entities = match last {
        Some(last) => format!(
            "nodes {}   controls {}",
            last.counters.scene_nodes, last.counters.controls
        ),
        None => "nodes --   controls --".to_string(),
    };

    let findings = if report.is_clean() {
        "findings none".to_string()
    } else {
        format!(
            "findings {}   warn {}   error {}",
            report.len(),
            report.count_of(Severity::Warning),
            report.count_of(Severity::Error)
        )
    };

    let profiler_state = if profiler.enabled() {
        "profiler on  (F5 / o)".to_string()
    } else {
        "profiler paused  (F5 / o)".to_string()
    };

    let finding_rows = (0..max_rows)
        .map(|i| match report.findings().get(i) {
            Some(finding) => format!("{}: {}", finding.severity.label(), finding.summary()),
            None => "(none)".to_string(),
        })
        .collect();

    OverlayText {
        title: "Performance".to_string(),
        fps,
        frame,
        profiler: profiler_state,
        update_layout,
        paint_render,
        commands,
        entities,
        findings,
        finding_rows,
        shortcuts: "F3 / ` / d bounds   F4 / p panel   F5 / o profiler".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use draw_core::{Size, Vec2};
    use draw_profile::{FindingCode, FrameCounters, StageTimes};
    use draw_render::DrawCommand;
    use draw_ui::Widget;

    fn viewport() -> Viewport {
        Viewport::new(Size::new(1280.0, 720.0))
    }

    fn clean_report() -> InspectionReport {
        InspectionReport::new()
    }

    fn profiler_with(frames: &[(f32, usize)]) -> Profiler {
        let mut profiler = Profiler::new();
        for (i, (ms, commands)) in frames.iter().enumerate() {
            let mut stats = draw_profile::FrameStats::new(i as u64);
            stats.frame_ms = *ms;
            stats.stages = StageTimes::new(ms * 0.1, ms * 0.1, ms * 0.2, ms * 0.3);
            stats.counters = FrameCounters::new(12, 7, *commands, 1);
            profiler.record(stats);
        }
        profiler
    }

    fn label_text(overlay: &PerformanceOverlay, id: NodeId) -> String {
        overlay
            .ui()
            .widget(id)
            .and_then(Widget::text)
            .unwrap_or_default()
            .to_string()
    }

    #[test]
    fn computes_metrics_and_mirrors_them_into_labels() {
        let profiler = profiler_with(&[(8.0, 10), (16.0, 30)]);
        let mut overlay = PerformanceOverlay::new();
        overlay.update(&profiler, &clean_report(), viewport());

        let text = overlay.text();
        assert_eq!(text.title, "Performance");
        assert!(text.fps.starts_with("FPS 83"), "fps was {}", text.fps);
        assert!(
            text.frame.contains("frame 16.00 ms"),
            "frame was {}",
            text.frame
        );
        assert!(text.update_layout.contains("update"));
        assert!(text.commands.contains("commands 30"));
        assert!(text.commands.contains("max 30"));
        assert!(text.entities.contains("nodes 12"));
        assert!(text.entities.contains("controls 7"));
        assert!(text.profiler.starts_with("profiler on"));
        assert!(text.profiler.contains("F5"));
        assert!(text.shortcuts.contains("F3"));
        assert!(text.shortcuts.contains("F4 / p panel"));
        assert_eq!(text.findings, "findings none");
        assert!(text.finding_rows.iter().all(|row| row == "(none)"));

        // the UI labels carry the same strings
        assert_eq!(label_text(&overlay, overlay.rows.fps), overlay.text().fps);
    }

    #[test]
    fn findings_are_summarized_and_listed() {
        let profiler = profiler_with(&[(10.0, 5)]);
        let mut report = InspectionReport::new();
        report.report(
            FindingCode::DegenerateRect,
            "fill rect has zero/negative area",
        );
        report.report(
            FindingCode::DegenerateRect,
            "fill rect has zero/negative area",
        );
        report.report(
            FindingCode::UnmatchedRestore,
            "Restore without matching Save",
        );

        let mut overlay = PerformanceOverlay::new();
        overlay.update(&profiler, &report, viewport());

        let text = overlay.text();
        assert_eq!(text.findings, "findings 2   warn 1   error 1");
        assert_eq!(
            text.finding_rows[0],
            "warning: fill rect has zero/negative area (x2)"
        );
        assert_eq!(text.finding_rows[1], "error: Restore without matching Save");
        assert_eq!(text.finding_rows[2], "(none)");
    }

    #[test]
    fn empty_profiler_uses_placeholders() {
        let overlay_profiler = Profiler::new();
        let mut overlay = PerformanceOverlay::new();
        overlay.update(&overlay_profiler, &clean_report(), viewport());
        let text = overlay.text();
        assert_eq!(text.fps, "FPS --");
        assert_eq!(text.frame, "frame -- ms");
        assert_eq!(text.commands, "commands --");
    }

    #[test]
    fn profiler_state_is_visible_and_follows_the_profiler() {
        let mut profiler = profiler_with(&[(16.0, 4)]);
        let mut overlay = PerformanceOverlay::new();
        overlay.update(&profiler, &clean_report(), viewport());
        assert!(overlay.text().profiler.starts_with("profiler on"));

        // F5 / o toggles this; the panel must change to prove it.
        profiler.set_enabled(false);
        overlay.update(&profiler, &clean_report(), viewport());
        assert!(overlay.text().profiler.starts_with("profiler paused"));

        profiler.set_enabled(true);
        overlay.update(&profiler, &clean_report(), viewport());
        assert!(overlay.text().profiler.starts_with("profiler on"));
    }

    #[test]
    fn closed_overlay_paints_nothing_and_keeps_text() {
        let profiler = profiler_with(&[(16.0, 4)]);
        let mut overlay = PerformanceOverlay::new();
        overlay.update(&profiler, &clean_report(), viewport());
        let before = overlay.text().clone();

        assert!(!overlay.set_open(false));
        overlay.update(&profiler, &clean_report(), viewport());

        let mut ctx = PaintContext::new();
        overlay.paint(&mut ctx);
        assert!(ctx.is_empty(), "closed overlay must not paint");
        assert_eq!(overlay.text(), &before);
    }

    #[test]
    fn open_overlay_paints_panel_and_text() {
        let profiler = profiler_with(&[(16.0, 4)]);
        let mut overlay = PerformanceOverlay::new();
        overlay.update(&profiler, &clean_report(), viewport());

        let mut ctx = PaintContext::new();
        overlay.paint(&mut ctx);
        let list = ctx.into_draw_list();

        assert!(list
            .commands()
            .iter()
            .any(|command| matches!(command, DrawCommand::FillRect { .. })));
        let texts = list
            .commands()
            .iter()
            .filter(|command| matches!(command, DrawCommand::DrawText { .. }))
            .count();
        assert_eq!(texts, overlay.config().row_count());
    }

    #[test]
    fn toggle_flips_state() {
        let mut overlay = PerformanceOverlay::new();
        assert!(overlay.is_open());
        assert!(!overlay.toggle());
        assert!(!overlay.is_open());
        assert!(overlay.toggle());
        assert!(overlay.is_open());
    }

    #[test]
    fn panel_is_pinned_to_configured_corner() {
        let config = OverlayConfig {
            corner: Corner::TopRight,
            width: 300.0,
            margin: 10.0,
            ..OverlayConfig::default()
        };
        let mut overlay = PerformanceOverlay::with_config(config);
        overlay.update(&profiler_with(&[(16.0, 1)]), &clean_report(), viewport());

        let rect = overlay.ui().control(overlay.panel()).unwrap().rect;
        assert_eq!(rect.right(), viewport().logical_size().width - 10.0);
        assert_eq!(rect.left(), viewport().logical_size().width - 310.0);
        assert_eq!(rect.top(), 10.0);

        let mut bottom = PerformanceOverlay::with_config(OverlayConfig {
            corner: Corner::BottomLeft,
            ..config
        });
        bottom.update(&profiler_with(&[(16.0, 1)]), &clean_report(), viewport());
        let rect = bottom.ui().control(bottom.panel()).unwrap().rect;
        assert_eq!(rect.left(), 10.0);
        assert_eq!(rect.bottom(), viewport().logical_size().height - 10.0);
    }

    #[test]
    fn pointer_over_panel_is_consumed() {
        let mut overlay = PerformanceOverlay::new();
        overlay.update(&profiler_with(&[(16.0, 1)]), &clean_report(), viewport());
        let rect = overlay.ui().control(overlay.panel()).unwrap().rect;
        let inside = Vec2::new(rect.center().x, rect.center().y);
        let result = overlay.handle_input(&InputEvent::PointerMove { position: inside });
        assert_eq!(result, EventResult::Handled);
    }
}
