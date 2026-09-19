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
