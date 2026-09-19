use draw_components::{Component, Flex, Label, Panel, VBox};
use draw_core::{Color, Edges, EventResult, InputEvent, NodeId, ViewportSize};
use draw_profile::{InspectionReport, Phase, Profiler, Severity};
use draw_render::PaintContext;
use draw_scene::SceneTree;

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
/// The overlay owns a private [`SceneTree`]; hosts keep their own tree and
/// simply call [`update`](PerformanceOverlay::update) + [`paint`](PerformanceOverlay::paint)
/// after painting the application. When closed, `update` and `paint` are no-ops
/// and paint nothing.
pub struct PerformanceOverlay {
    tree: SceneTree,
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
        let mut tree = SceneTree::new();
        let tree_root = tree.root();
        let root = tree.add_child(
            tree_root,
            Flex::column().mouse_filter(draw_ui::MouseFilter::Ignore),
        );

        let (anchors, offsets) = config.placement();
        let panel = tree.add_child(
            root,
            Panel::new()
                .color(config.background)
                .border(Some(config.border))
                .anchors(anchors)
                .offsets(offsets),
        );

        let vbox = tree.add_child(
            panel,
            VBox::new()
                .separation(config.separation)
                .padding(Edges::all(config.padding)),
        );

        let text_color = config.text_color;
        let muted = config.muted_color;
        let title_size = config.title_font_size;
        let font_size = config.font_size;

        let add = |tree: &mut SceneTree, text: &str, size: f32, color: Color| -> NodeId {
            tree.add_child(vbox, Label::new(text).font_size(size).color(color))
        };

        let title = add(&mut tree, "Performance", title_size, text_color);
        let fps = add(&mut tree, "FPS --", font_size, text_color);
        let frame = add(&mut tree, "frame -- ms", font_size, text_color);
        let profiler = add(&mut tree, "profiler on  (F5 / o)", font_size, muted);
        let update_layout = add(&mut tree, "update --   layout --", font_size, muted);
        let paint_render = add(&mut tree, "paint --   render --", font_size, muted);
        let commands = add(&mut tree, "commands --", font_size, muted);
        let entities = add(&mut tree, "nodes --   controls --", font_size, muted);
        let findings = add(&mut tree, "findings none", font_size, muted);
        let finding_rows: Vec<NodeId> = (0..config.max_finding_rows)
            .map(|_| add(&mut tree, "(none)", font_size, muted))
            .collect();
        let shortcuts = add(
            &mut tree,
            "F3 / ` / d bounds   F4 / p panel   F5 / o profiler",
            font_size,
            muted,
        );

        Self {
            tree,
            panel,
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
    pub fn tree(&self) -> &SceneTree {
        &self.tree
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
    pub fn update(
        &mut self,
        profiler: &Profiler,
        report: &InspectionReport,
        viewport: ViewportSize,
    ) {
        if !self.open {
            return;
        }
        let text = build_text(profiler, report, self.config.max_finding_rows);
        self.apply(&text);
        self.text = text;
        draw_ui::layout(&mut self.tree, viewport);
        self.tree.update();
    }

    /// Paints the overlay into `ctx`. No-op while closed.
    pub fn paint(&self, ctx: &mut PaintContext) {
        if self.open {
            draw_ui::paint(&self.tree, ctx);
        }
    }

    /// Routes an input event through the overlay. Closed overlays ignore input.
    pub fn handle_input(&mut self, event: &InputEvent) -> EventResult {
        if self.open {
            draw_ui::handle_input(&mut self.tree, event)
        } else {
            EventResult::Ignored
        }
    }

    fn apply(&mut self, text: &OverlayText) {
        draw_components::set_text(&mut self.tree, self.rows.title, text.title.clone());
        draw_components::set_text(&mut self.tree, self.rows.fps, text.fps.clone());
        draw_components::set_text(&mut self.tree, self.rows.frame, text.frame.clone());
        draw_components::set_text(&mut self.tree, self.rows.profiler, text.profiler.clone());
        draw_components::set_text(
            &mut self.tree,
            self.rows.update_layout,
            text.update_layout.clone(),
        );
        draw_components::set_text(
            &mut self.tree,
            self.rows.paint_render,
            text.paint_render.clone(),
        );
        draw_components::set_text(&mut self.tree, self.rows.commands, text.commands.clone());
        draw_components::set_text(&mut self.tree, self.rows.entities, text.entities.clone());
        draw_components::set_text(&mut self.tree, self.rows.findings, text.findings.clone());
        for (id, row) in self.rows.finding_rows.iter().zip(&text.finding_rows) {
            draw_components::set_text(&mut self.tree, *id, row.clone());
        }
        draw_components::set_text(&mut self.tree, self.rows.shortcuts, text.shortcuts.clone());
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
