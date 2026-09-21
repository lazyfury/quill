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
use crate::badge::{self, BadgeApp};
use crate::go::{GoUsage, Usage, UsageWindow};
use crate::ui::{BalanceApp, Tab};

/// Window-layout size the self-check renders (matches `ui.rs`'s tests), tall
/// enough for the usual one-currency body plus the footer's refresh button.
const WINDOW_WIDTH: f32 = 520.0;
const WINDOW_HEIGHT: f32 = 460.0;

/// Panel body size plus the popover arrow, mirroring the host's panel window.
const PANEL_BODY_WIDTH: f32 = 300.0;
const PANEL_BODY_HEIGHT: f32 = 470.0;
const ARROW_HEIGHT: f32 = crate::ui::ARROW_HEIGHT;

/// Canned reply the self-check feeds the view instead of hitting the endpoint.
///
/// One currency, which is the usual reply and the size the panel is built for;
/// the two-card layout is covered by `ui.rs`'s tests (visibility and values,
/// which need no frame).
fn sample_balance() -> Balance {
    Balance {
        is_available: true,
        balance_infos: vec![BalanceInfo {
            currency: "CNY".to_string(),
            total_balance: "110.00".to_string(),
            granted_balance: "10.00".to_string(),
            topped_up_balance: "100.00".to_string(),
        }],
    }
}

/// Canned OpenCode Go reply: all three windows healthy, no reset stamps so the
/// rows are stable to assert against.
fn sample_go() -> GoUsage {
    let window = |percent| UsageWindow {
        status: "ok".to_string(),
        percent,
        resets_at: None,
    };
    GoUsage {
        usage: Usage {
            rolling: Some(window(12.0)),
            weekly: Some(window(40.0)),
            monthly: Some(window(100.0)),
        },
    }
}

/// Drives one view through the real frame lifecycle and records the result:
/// update -> (canned reply) -> update -> layout -> paint -> backend.
///
/// The on-open refresh queries both sources, so both canned replies are applied
/// — the button only leaves `Busy` when the last one lands. `tab` selects the
/// page to record after the replies.
fn record_frame(
    mut app: BalanceApp,
    width: f32,
    height: f32,
    tab: Tab,
) -> (BalanceApp, RecordedFrame) {
    let viewport = ViewportSize::new(Size::new(width, height));
    app.update(viewport, 0.016);
    // The on-open refresh is the host's to answer; the self-check answers it
    // with canned data, so nothing here ever touches the network.
    if app.take_refresh_request() {
        app.apply_result(Ok(sample_balance()));
        app.apply_go_result(Ok(sample_go()));
    }
    app.select_tab(tab);
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
fn dump_tree(tree: &draw_scene::SceneTree, controls: usize) {
    let total = tree.iter().count();
    println!("  ui tree: {total} nodes, {controls} controls (hidden subtrees pruned)");
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

/// Semantic checks for the OpenCode Go page: the tab, the three window rows and
/// the remainders the canned reply implies.
fn go_semantic_checks(app: &BalanceApp, frame: &RecordedFrame) -> Vec<String> {
    let mut failures = Vec::new();
    let width = frame.viewport.logical_size().width;
    let height = frame.viewport.logical_size().height;

    if frame.command_count() == 0 {
        failures.push("the Go frame recorded no draw commands".to_string());
        return failures;
    }

    for (what, needle) in [
        ("tab", "OpenCode Go"),
        ("rolling row", "5 小时"),
        ("rolling value", "剩余 88%"),
        ("weekly value", "剩余 60%"),
        ("monthly value", "已用尽"),
        ("refresh button", "刷新 ("),
    ] {
        match text_commands(frame, needle).first() {
            Some((_, position)) if on_screen(*position, width, height) => {}
            Some((_, position)) => failures.push(format!(
                "go {what}: `{needle}` is drawn at {position:?}, outside the {width}x{height} viewport"
            )),
            None => failures.push(format!("go {what}: `{needle}` is not in the draw list")),
        }
    }

    if app.last_go().is_none() {
        failures.push("the canned Go usage was never applied".to_string());
    }

    failures
}

/// Counts an inspection report: prints each finding and turns every Error into
/// a failure line. Shared by the main view's layouts and the badge window.
fn report_findings(report: &InspectionReport, failures: &mut Vec<String>) {
    if report.is_clean() {
        println!("  inspect: clean");
        return;
    }
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

/// What, if anything, a self-check run prints for the frame.
///
/// Split so a run can ask for just the UI tree or just the draw commands: the
/// two together are the most context any command here produces, and usually
/// only one of them is being read.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Dump {
    None,
    All,
    Tree,
    Commands,
}

impl Dump {
    fn wants_tree(self) -> bool {
        matches!(self, Self::All | Self::Tree)
    }

    fn wants_commands(self) -> bool {
        matches!(self, Self::All | Self::Commands)
    }
}

/// One self-check run: record, inspect, report. Returns the failure lines
/// (empty = pass); findings are printed as they are seen. `dump` selects which
/// parts of the frame are printed, so a frame can be read without a screen.
fn run_one(
    name: &str,
    app: BalanceApp,
    width: f32,
    height: f32,
    panel: bool,
    tab: Tab,
    dump: Dump,
) -> Vec<String> {
    println!("self-check: {name} ({width}x{height})");
    let (app, frame) = record_frame(app, width, height, tab);
    println!("  frame: {} commands", frame.command_count());

    if dump.wants_tree() {
        dump_tree(app.tree(), app.control_count());
    }
    if dump.wants_commands() {
        dump_frame(&frame);
    }

    let report = inspect_frame(&app, &frame);
    let mut failures = match tab {
        Tab::DeepSeek => semantic_checks(&app, &frame, panel),
        Tab::Go => go_semantic_checks(&app, &frame),
    };
    report_findings(&report, &mut failures);

    for line in &failures {
        println!("  FAIL {line}");
    }
    if failures.is_empty() {
        println!("  ok");
    }
    failures
}

// -- the badge window -------------------------------------------------------

/// Records one badge frame. The same lifecycle as [`record_frame`], minus the
/// view state: the badge has no data source of its own, so "the host applied a
/// reply" and "paint a frame" are the whole story.
fn record_badge_frame(mut app: BadgeApp) -> (BadgeApp, RecordedFrame) {
    let viewport = ViewportSize::new(Size::new(badge::BADGE_WIDTH, badge::BADGE_HEIGHT));
    // One fetch, two windows: this is the same canned reply [`record_frame`]
    // feeds the main view, handed over exactly as the host would.
    let state = badge::State::from_result(&Ok(sample_balance()));
    app.show(badge::TITLE, badge::BALANCE_PREFIX, &state);
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

/// Semantic checks for the badge, which is its own surface and so gets its own
/// list instead of joining [`semantic_checks`].
fn badge_checks(frame: &RecordedFrame) -> Vec<String> {
    let mut failures = Vec::new();
    let size = frame.viewport.logical_size();
    let commands = frame.commands();

    if frame.command_count() == 0 {
        failures.push("the badge frame recorded no draw commands".to_string());
        return failures;
    }

    // The badge is a transparent overlay, so text is the *only* thing it may
    // paint. A fill or a border here would cover the desktop — and would bring
    // the window-server shadow back with it, since AppKit traces the shadow
    // from the window's alpha.
    if let Some(other) = commands
        .iter()
        .find(|command| !matches!(command, DrawCommand::DrawText { .. }))
    {
        failures.push(format!(
            "badge: a transparent overlay may only paint text, found {other:?}"
        ));
    }

    // Mirroring is the badge's whole job, so the frame has to carry the *value*
    // the host handed it — not merely some text. The canned reply is CNY
    // 110.00, so that is its headline and that is what should be on screen.
    let headline = sample_balance().headline();
    let expected = badge::State::Ready(headline).line(badge::BALANCE_PREFIX);
    let needles: [(&str, String); 2] = [("title", badge::TITLE.to_string()), ("balance", expected)];
    for (what, needle) in needles {
        match text_commands(frame, &needle).first() {
            Some((_, position)) if on_screen(*position, size.width, size.height) => {}
            Some((_, position)) => failures.push(format!(
                "badge {what}: `{needle}` is drawn at {position:?}, outside the {size:?} surface"
            )),
            None => failures.push(format!("badge {what}: `{needle}` is not in the draw list")),
        }
    }

    failures
}

/// The badge window's self-check: the second window gets the same treatment as
/// the first, so both are verified without a screen.
fn run_badge(dump: Dump) -> Vec<String> {
    let (width, height) = (badge::BADGE_WIDTH, badge::BADGE_HEIGHT);
    println!("self-check: badge ({width}x{height})");
    let (app, frame) = record_badge_frame(BadgeApp::new(Theme::dark()));
    println!(
        "  frame: {} commands, 余额行 {:?}",
        frame.command_count(),
        app.balance_text().unwrap_or("（无）")
    );

    if dump.wants_tree() {
        dump_tree(app.tree(), draw_ui::control_count(app.tree()));
    }
    if dump.wants_commands() {
        dump_frame(&frame);
    }

    let mut stats = FrameStats::new(0);
    stats.counters = FrameCounters::new(
        0,
        draw_ui::control_count(app.tree()),
        frame.command_count(),
        1,
    );
    let report = inspect(&frame.draw_list, &stats);
    let mut failures = badge_checks(&frame);
    report_findings(&report, &mut failures);

    for line in &failures {
        println!("  FAIL {line}");
    }
    if failures.is_empty() {
        println!("  ok");
    }
    failures
}

/// Runs every self-check layout; returns the process exit code.
pub fn run(dump: Dump) -> i32 {
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
        Tab::DeepSeek,
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
        Tab::DeepSeek,
        dump,
    )
    .len();

    // The Go tab is a second page in the same tree, so it gets its own recorded
    // frame in both shapes — otherwise a Go-only layout regression would never
    // be seen without a screen.
    failed += run_one(
        "window/go",
        BalanceApp::new(
            Theme::dark(),
            "https://api.deepseek.com/user/balance".to_string(),
        ),
        WINDOW_WIDTH,
        WINDOW_HEIGHT,
        false,
        Tab::Go,
        dump,
    )
    .len();

    failed += run_one(
        "panel/go",
        BalanceApp::new_panel(
            Theme::dark(),
            "https://api.deepseek.com/user/balance".to_string(),
        ),
        PANEL_BODY_WIDTH,
        PANEL_BODY_HEIGHT + ARROW_HEIGHT,
        true,
        Tab::Go,
        dump,
    )
    .len();

    failed += run_badge(dump).len();

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

    /// Both surfaces and both tabs get the same treatment: every recorded frame
    /// must pass the structural audit and its own semantic checks.
    #[test]
    fn every_app_frame_passes_its_checks() {
        let cases = [
            (
                "window",
                window_app(),
                WINDOW_WIDTH,
                WINDOW_HEIGHT,
                Tab::DeepSeek,
                false,
            ),
            (
                "panel",
                panel_app(),
                PANEL_BODY_WIDTH,
                PANEL_BODY_HEIGHT + ARROW_HEIGHT,
                Tab::DeepSeek,
                true,
            ),
            (
                "panel/go",
                panel_app(),
                PANEL_BODY_WIDTH,
                PANEL_BODY_HEIGHT + ARROW_HEIGHT,
                Tab::Go,
                true,
            ),
        ];

        for (name, app, width, height, tab, panel) in cases {
            let (app, frame) = record_frame(app, width, height, tab);
            let report = inspect_frame(&app, &frame);
            assert!(
                report.max_severity() < Some(Severity::Error),
                "{name}: structural findings: {:?}",
                report.findings()
            );
            let failures = match tab {
                Tab::DeepSeek => semantic_checks(&app, &frame, panel),
                Tab::Go => go_semantic_checks(&app, &frame),
            };
            assert!(failures.is_empty(), "{name}: {failures:?}");
        }
    }

    /// The badge window is a second surface with its own view, so it gets its
    /// own frame check: nothing but text on it, and both lines land on screen.
    #[test]
    fn the_badge_frame_passes_every_check() {
        let (app, frame) = record_badge_frame(BadgeApp::new(Theme::dark()));
        assert!(
            badge_checks(&frame).is_empty(),
            "badge semantic checks failed"
        );

        let mut stats = FrameStats::new(0);
        stats.counters = FrameCounters::new(
            0,
            draw_ui::control_count(app.tree()),
            frame.command_count(),
            1,
        );
        let report = inspect(&frame.draw_list, &stats);
        assert!(
            report.max_severity() < Some(Severity::Error),
            "structural findings: {:?}",
            report.findings()
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

    /// Same story for the badge's one structural rule: an opaque command has to
    /// be caught, or "only text" could pass without ever looking at anything.
    #[test]
    fn a_backdrop_on_the_badge_trips_the_check() {
        let viewport = ViewportSize::new(Size::new(badge::BADGE_WIDTH, badge::BADGE_HEIGHT));
        let mut backend = RecordingBackend::new();
        backend.begin_frame(viewport).expect("begin frame");
        let mut ctx = PaintContext::new();
        ctx.fill_rect(
            Rect::from_min_size(
                Vec2::ZERO,
                Size::new(badge::BADGE_WIDTH, badge::BADGE_HEIGHT),
            ),
            Color::WHITE,
        );
        backend.submit(&ctx.into_draw_list()).expect("submit frame");
        backend.end_frame().expect("end frame");
        let frame = backend.last_frame().expect("a frame was recorded").clone();

        let failures = badge_checks(&frame);
        assert!(
            failures.iter().any(|line| line.contains("only paint text")),
            "a fill must be reported, got {failures:?}"
        );
    }
}
