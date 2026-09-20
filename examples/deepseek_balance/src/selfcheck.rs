//! Headless UI self-check: the balance view through the full pipeline.
//!
//! This is the "no vision" checker. The same `BalanceApp` the wgpu host paints
//! is instead painted into a [`RecordingBackend`], which records the frame's
//! viewport and `DrawCommand` sequence — the data a real backend would have
//! turned into pixels. Two layers read that recording:
//!
//! 1. `draw_profile::inspect` audits the frame structurally (degenerate
//!    geometry, `NaN` values, unbalanced `Save`/`Restore`, opacity out of
//!    range, command budget) and returns a severity-ranked report.
//! 2. Semantic checks look *inside* the commands: the title, the refresh
//!    button's label and the fetched numbers must appear as `DrawText`
//!    commands at sane positions, and the panel/window shape must hold.
//!
//! The same checks run two ways:
//!
//! - `deepseek_balance --selfcheck` runs both layouts and prints the report,
//!   and exits non-zero on any Error-severity finding or failed check.
//! - `cargo test` (this module's `#[cfg(test)]`) asserts the same conditions,
//!   so a UI regression fails without opening a window.

use draw_backend_recording::{RecordedFrame, RecordingBackend};
use draw_core::{Color, Rect, Size, Vec2, ViewportSize};
use draw_profile::{inspect, FrameCounters, FrameStats, InspectionReport, Severity};
use draw_render::{CornerRadii, DrawCommand, PaintContext, RenderBackend};
use draw_theme::Theme;

use crate::api::{Balance, BalanceInfo};
use crate::ui::BalanceApp;

/// Window-layout size the self-check renders (matches `ui.rs`'s tests).
const WINDOW_WIDTH: f32 = 520.0;
const WINDOW_HEIGHT: f32 = 460.0;

/// Panel body size plus the popover arrow, mirroring the host's panel window.
const PANEL_BODY_WIDTH: f32 = 300.0;
const PANEL_BODY_HEIGHT: f32 = 420.0;
const ARROW_HEIGHT: f32 = crate::ui::ARROW_HEIGHT;

/// Canned reply the self-check feeds the view instead of hitting the endpoint:
/// two currencies, so both cards are on screen.
fn sample_balance() -> Balance {
    Balance {
        is_available: true,
        balance_infos: vec![
            BalanceInfo {
                currency: "CNY".to_string(),
                total_balance: "110.00".to_string(),
                granted_balance: "10.00".to_string(),
                topped_up_balance: "100.00".to_string(),
            },
            BalanceInfo {
                currency: "USD".to_string(),
                total_balance: "7.00".to_string(),
                granted_balance: "0.00".to_string(),
                topped_up_balance: "7.00".to_string(),
            },
        ],
    }
}

/// Drives one view through the real frame lifecycle and records the result:
/// update -> (canned reply) -> update -> layout -> paint -> backend.
fn record_frame(mut app: BalanceApp, width: f32, height: f32) -> (BalanceApp, RecordedFrame) {
    let viewport = ViewportSize::new(Size::new(width, height));
    app.update(viewport, 0.016);
    // The on-open refresh is the host's to answer; the self-check answers it
    // with canned data, so nothing here ever touches the network.
    if app.take_refresh_request() {
        app.apply_result(Ok(sample_balance()));
    }
    app.update(viewport, 0.016);
    app.layout(viewport);

    let mut backend = RecordingBackend::new();
    backend.begin_frame(viewport).expect("begin frame");
    let mut ctx = PaintContext::new();
    app.paint(&mut ctx);
    backend.submit(&ctx.into_draw_list()).expect("submit frame");
    backend.end_frame().expect("end frame");

    let frame = backend.last_frame().expect("a frame was recorded").clone();
    (app, frame)
}

/// [`inspect`] over a recorded frame, with the counters filled from the frame
/// itself (timing stays zero: a headless frame has no meaningful wall time).
fn inspect_frame(app: &BalanceApp, frame: &RecordedFrame) -> InspectionReport {
    let mut stats = FrameStats::new(0);
    stats.counters = FrameCounters::new(0, app.control_count(), frame.command_count(), 1);
    inspect(&frame.draw_list, &stats)
}

/// `DrawText` commands carrying `needle`, with their positions.
fn text_commands(frame: &RecordedFrame, needle: &str) -> Vec<(String, Vec2)> {
    frame
        .commands()
        .iter()
        .filter_map(|command| match command {
            DrawCommand::DrawText { text, position, .. } if text.contains(needle) => {
                Some((text.clone(), *position))
            }
            _ => None,
        })
        .collect()
}

/// Whether the viewport coordinate is on screen (non-negative, inside the
/// frame). `0.5` of slack for anti-aliased edges.
fn on_screen(point: Vec2, width: f32, height: f32) -> bool {
    point.x >= -0.5 && point.y >= -0.5 && point.x <= width + 0.5 && point.y <= height + 0.5
}

// -- dump -------------------------------------------------------------------
//
// The dump is how the frame gets *read*: the control tree shows the structure
// (with each control's laid-out rect), the command list shows the exact pixels
// a real backend would have produced. Together they replace looking at a
// screenshot.

/// `12.0` -> `12`, `12.5` -> `12.5`: keeps dump lines short and aligned.
fn num(v: f32) -> String {
    if v == v.trunc() && v.is_finite() {
        format!("{}", v as i64)
    } else {
        format!("{v:.1}")
    }
}

fn rect_s(rect: Rect) -> String {
    format!(
        "{}x{} @{},{}",
        num(rect.size.width),
        num(rect.size.height),
        num(rect.origin.x),
        num(rect.origin.y)
    )
}

fn color_s(color: Color) -> String {
    let ch = |v: f32| (v * 255.0).round().clamp(0.0, 255.0) as u8;
    format!(
        "#{:02X}{:02X}{:02X}/{}",
        ch(color.r),
        ch(color.g),
        ch(color.b),
        num(color.a)
    )
}

fn corners_s(corners: CornerRadii) -> String {
    format!("r={}/{}", num(corners.top_left), num(corners.top_right))
}

fn pos_s(point: Vec2) -> String {
    format!("({},{})", num(point.x), num(point.y))
}

/// One compact line per draw command — the same information a real backend
/// would rasterize, in paint order.
fn describe_command(command: &DrawCommand) -> String {
    match command {
        DrawCommand::Save => "Save".to_string(),
        DrawCommand::Restore => "Restore".to_string(),
        DrawCommand::SetTransform(t) => {
            format!("SetTransform origin={}", pos_s(t.origin))
        }
        DrawCommand::SetOpacity(opacity) => format!("SetOpacity {opacity:.2}"),
        DrawCommand::ClipRect(rect) => format!("ClipRect {}", rect_s(*rect)),
        DrawCommand::FillRect { rect, paint } => {
            format!("FillRect {} {}", rect_s(*rect), color_s(paint.color))
        }
        DrawCommand::StrokeRect { rect, width, paint } => format!(
            "StrokeRect {} w={} {}",
            rect_s(*rect),
            num(*width),
            color_s(paint.color)
        ),
        DrawCommand::Line {
            from,
            to,
            width,
            paint,
        } => format!(
            "Line {} -> {} w={} {}",
            pos_s(*from),
            pos_s(*to),
            num(*width),
            color_s(paint.color)
        ),
        DrawCommand::FillCircle {
            center,
            radius,
            paint,
        } => format!(
            "FillCircle {} r={} {}",
            pos_s(*center),
            num(*radius),
            color_s(paint.color)
        ),
        DrawCommand::StrokeCircle {
            center,
            radius,
            width,
            paint,
        } => format!(
            "StrokeCircle {} r={} w={} {}",
            pos_s(*center),
            num(*radius),
            num(*width),
            color_s(paint.color)
        ),
        DrawCommand::FillRoundedRect {
            rect,
            corners,
            paint,
        } => format!(
            "FillRoundedRect {} {} {}",
            rect_s(*rect),
            corners_s(*corners),
            color_s(paint.color)
        ),
        DrawCommand::StrokeRoundedRect {
            rect,
            corners,
            width,
            paint,
        } => format!(
            "StrokeRoundedRect {} {} w={} {}",
            rect_s(*rect),
            corners_s(*corners),
            num(*width),
            color_s(paint.color)
        ),
        DrawCommand::DrawImage { destination, .. } => {
            format!("DrawImage dest={}", rect_s(*destination))
        }
        DrawCommand::DrawText {
            text,
            position,
            font_size,
            align,
            paint,
        } => format!(
            "DrawText {} font={} {:?} {} \"{}\"",
            pos_s(*position),
            num(*font_size),
            align,
            color_s(paint.color),
            text
        ),
    }
}

/// Prints every draw command of the frame, in paint order.
fn dump_frame(frame: &RecordedFrame) {
    let size = frame.viewport.logical_size();
    println!("  viewport: {}x{}", num(size.width), num(size.height));
    println!("  commands ({}):", frame.command_count());
    for (index, command) in frame.commands().iter().enumerate() {
        println!("    [{index:3}] {}", describe_command(command));
    }
}

/// Prints the visible UI tree: structure, widget kind and each control's
/// laid-out rect, so a layout problem is visible without rendering.
fn dump_tree(app: &BalanceApp) {
    let tree = app.tree();
    let total = tree.iter().count();
    println!(
        "  ui tree: {total} nodes, {} controls (hidden subtrees pruned)",
        app.control_count()
    );
    for id in tree.iter_visible() {
        let node = tree.node(id);
        let mut depth = 0;
        let mut parent = node.parent();
        while let Some(pid) = parent {
            depth += 1;
            parent = tree.parent(pid);
        }
        let control = draw_ui::control(tree, id)
            .map(|c| format!(" rect={}", rect_s(c.rect)))
            .unwrap_or_default();
        let widget = match draw_ui::widget(tree, id) {
            Some(draw_ui::Widget::Panel { color, .. }) => {
                format!(" Panel {}", color_s(*color))
            }
            Some(draw_ui::Widget::Flex(_)) => " Flex".to_string(),
            Some(draw_ui::Widget::Grid(_)) => " Grid".to_string(),
            Some(draw_ui::Widget::Label {
                text, font_size, ..
            }) => format!(" Label \"{}\" font={}", text, num(*font_size)),
            Some(draw_ui::Widget::Button(button)) => {
                format!(" Button \"{}\"", button.text)
            }
            None => String::new(),
        };
        println!(
            "    {}{:?} \"{}\"{control}{widget}",
            "  ".repeat(depth),
            node.kind(),
            node.name()
        );
    }
}

/// Semantic checks over a recorded frame. Each failed check comes back as a
/// human-readable line; an empty vector means the frame reads as expected.
fn semantic_checks(app: &BalanceApp, frame: &RecordedFrame, panel: bool) -> Vec<String> {
    let mut failures = Vec::new();
    let width = frame.viewport.logical_size().width;
    let height = frame.viewport.logical_size().height;
    let commands = frame.commands();

    if frame.command_count() == 0 {
        failures.push("the frame recorded no draw commands".to_string());
        return failures;
    }

    // The panel's shape lives in the first commands: the window is one rounded
    // fill; the panel paints the arrow (a scoped rotated fill) first.
    if panel {
        let looks_like_arrow = matches!(
            (commands.first(), commands.get(1), commands.get(3)),
            (
                Some(DrawCommand::Save),
                Some(DrawCommand::SetTransform(_)),
                Some(DrawCommand::Restore)
            )
        );
        if !looks_like_arrow {
            failures.push(format!(
                "panel: expected Save/SetTransform/…/Restore arrow sequence first, found {:?}",
                &commands[..4.min(commands.len())]
            ));
        }
    } else {
        if let Some(DrawCommand::FillRoundedRect { rect, corners, .. }) = commands.first() {
            if *rect != Rect::from_min_size(Vec2::ZERO, Size::new(width, height)) {
                failures.push(format!(
                    "window: the backdrop is not the whole viewport: {rect:?} vs {width}x{height}"
                ));
            }
            if corners.top_left <= 0.0 {
                failures.push("window: the backdrop lost its rounded corners".to_string());
            }
        } else {
            failures.push(format!(
                "window: the first command should be the rounded backdrop, found {:?}",
                commands.first()
            ));
        }
        if commands
            .iter()
            .any(|c| matches!(c, DrawCommand::SetTransform(t) if !t.is_identity()))
        {
            failures.push("window: nothing should be rotated".to_string());
        }
    }

    // Content: the header and the fetched numbers must be drawn, on screen.
    for (what, needle) in [
        ("title", "DeepSeek 余额"),
        ("refresh button", "刷新 ("),
        ("currency", "CNY"),
        ("total", "110.00"),
    ] {
        match text_commands(frame, needle).first() {
            Some((_, position)) if on_screen(*position, width, height) => {}
            Some((_, position)) => failures.push(format!(
                "{what}: `{needle}` is drawn at {position:?}, outside the {width}x{height} viewport"
            )),
            None => failures.push(format!("{what}: `{needle}` is not in the draw list")),
        }
    }

    // The view's own state must agree with what was painted.
    if app.last_balance().is_none() {
        failures.push("the canned balance was never applied".to_string());
    }

    failures
}

/// One self-check run: record, inspect, report. Returns the failure lines
/// (empty = pass); findings are printed as they are seen. With `dump` the
/// whole UI tree and every draw command are printed, so a frame can be read
/// end to end without a screen.
fn run_one(
    name: &str,
    app: BalanceApp,
    width: f32,
    height: f32,
    panel: bool,
    dump: bool,
) -> Vec<String> {
    println!("self-check: {name} ({width}x{height})");
    let (app, frame) = record_frame(app, width, height);
    println!("  frame: {} commands", frame.command_count());

    if dump {
        dump_tree(&app);
        dump_frame(&frame);
    }

    let report = inspect_frame(&app, &frame);
    let mut failures = semantic_checks(&app, &frame, panel);

    if report.is_clean() {
        println!("  inspect: clean");
    } else {
        println!("  inspect: {} finding(s)", report.len());
        for finding in report.findings() {
            println!(
                "    {} {}: {}",
                finding.severity.label(),
                finding.code.label(),
                finding.summary()
            );
        }
        if report.count_of(Severity::Error) > 0 {
            failures.push(format!(
                "{} Error-severity finding(s) from inspect",
                report.count_of(Severity::Error)
            ));
        }
    }

    for line in &failures {
        println!("  FAIL {line}");
    }
    if failures.is_empty() {
        println!("  ok");
    }
    failures
}

/// Runs every self-check layout; returns the process exit code.
pub fn run(dump: bool) -> i32 {
    let mut failed = 0;

    failed += run_one(
        "window",
        BalanceApp::new(
            Theme::dark(),
            "https://api.deepseek.com/user/balance".to_string(),
        ),
        WINDOW_WIDTH,
        WINDOW_HEIGHT,
        false,
        dump,
    )
    .len();

    failed += run_one(
        "panel",
        BalanceApp::new_panel(
            Theme::dark(),
            "https://api.deepseek.com/user/balance".to_string(),
        ),
        PANEL_BODY_WIDTH,
        PANEL_BODY_HEIGHT + ARROW_HEIGHT,
        true,
        dump,
    )
    .len();

    if failed == 0 {
        println!("self-check: all layouts pass");
        0
    } else {
        eprintln!("self-check: {failed} check(s) failed");
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use draw_core::Color;

    fn window_app() -> BalanceApp {
        BalanceApp::new(
            Theme::dark(),
            "https://api.deepseek.com/user/balance".to_string(),
        )
    }

    fn panel_app() -> BalanceApp {
        BalanceApp::new_panel(
            Theme::dark(),
            "https://api.deepseek.com/user/balance".to_string(),
        )
    }

    #[test]
    fn the_window_frame_passes_every_check() {
        let (app, frame) = record_frame(window_app(), WINDOW_WIDTH, WINDOW_HEIGHT);
        let report = inspect_frame(&app, &frame);
        assert!(
            report.max_severity() < Some(Severity::Error),
            "structural findings: {:?}",
            report.findings()
        );
        assert!(
            semantic_checks(&app, &frame, false).is_empty(),
            "semantic checks failed"
        );
    }

    #[test]
    fn the_panel_frame_passes_every_check() {
        let (app, frame) = record_frame(
            panel_app(),
            PANEL_BODY_WIDTH,
            PANEL_BODY_HEIGHT + ARROW_HEIGHT,
        );
        let report = inspect_frame(&app, &frame);
        assert!(
            report.max_severity() < Some(Severity::Error),
            "structural findings: {:?}",
            report.findings()
        );
        assert!(
            semantic_checks(&app, &frame, true).is_empty(),
            "semantic checks failed"
        );
    }

    /// A broken frame must actually trip the inspector — otherwise the checks
    /// above could pass against a detector that never fires.
    #[test]
    fn the_inspector_catches_a_broken_frame() {
        let mut ctx = PaintContext::new();
        ctx.fill_rect(
            Rect::from_min_size(Vec2::new(f32::NAN, 0.0), Size::splat(10.0)),
            Color::RED,
        );
        let list = ctx.into_draw_list();
        let stats = FrameStats::new(0);
        let report = inspect(&list, &stats);
        assert!(report.max_severity() == Some(Severity::Error), "{report:?}");
    }
}
