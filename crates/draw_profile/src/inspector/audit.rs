//! The inspection logic: walking a `DrawList` and the frame counters and
//! recording findings.

use super::finding::{FindingCode, Severity};
use super::report::{InspectionConfig, InspectionReport};
use crate::stats::FrameStats;
use draw_core::{Rect, Transform2D, Vec2};
use draw_render::{DrawCommand, DrawList};

/// Inspects `list` and `stats` with the default [`InspectionConfig`].
pub fn inspect(list: &DrawList, stats: &FrameStats) -> InspectionReport {
    inspect_with(list, stats, &InspectionConfig::default())
}

/// Inspects `list` and `stats` against explicit thresholds.
pub fn inspect_with(
    list: &DrawList,
    stats: &FrameStats,
    config: &InspectionConfig,
) -> InspectionReport {
    let mut report = InspectionReport::new();
    inspect_frame(stats, config, &mut report);
    inspect_draw_list(list, config, &mut report);
    report
}

/// Inspects only the `DrawList` (geometry, commands, save/restore balance).
pub fn inspect_draw_list(
    list: &DrawList,
    config: &InspectionConfig,
    report: &mut InspectionReport,
) {
    if list.is_empty() {
        report.report(FindingCode::EmptyDrawList, "draw list has no commands");
        return;
    }

    check_budget(
        list.len(),
        config.max_draw_commands,
        FindingCode::CommandBudgetExceeded,
        "commands",
        report,
    );

    let mut save_depth: usize = 0;
    for command in list.commands() {
        match command {
            DrawCommand::Save => save_depth += 1,
            DrawCommand::Restore => {
                if save_depth == 0 {
                    report.report(
                        FindingCode::UnmatchedRestore,
                        "Restore without matching Save",
                    );
                } else {
                    save_depth -= 1;
                }
            }
            DrawCommand::SetTransform(transform) => check_transform(*transform, report),
            DrawCommand::SetOpacity(opacity) => {
                if !opacity.is_finite() || !(0.0..=1.0).contains(opacity) {
                    report.report(
                        FindingCode::OpacityOutOfRange,
                        format!("opacity {opacity} is outside 0.0..=1.0"),
                    );
                }
            }
            DrawCommand::ClipRect(rect) => {
                if check_rect(*rect, "clip rect", report) && is_degenerate(*rect) {
                    report.report(
                        FindingCode::DegenerateClip,
                        "clip rect has zero/negative area",
                    );
                }
            }
            DrawCommand::FillRect { rect, .. } => {
                if check_rect(*rect, "fill rect", report) && is_degenerate(*rect) {
                    report.report(
                        FindingCode::DegenerateRect,
                        "fill rect has zero/negative area",
                    );
                }
            }
            DrawCommand::StrokeRect { rect, width, .. } => {
                if check_rect(*rect, "stroke rect", report) && is_degenerate(*rect) {
                    report.report(
                        FindingCode::DegenerateRect,
                        "stroke rect has zero/negative area",
                    );
                }
                check_stroke_width(*width, "stroke rect", report);
            }
            DrawCommand::FillCircle { center, radius, .. } => {
                check_vec(*center, "circle center", report);
                check_radius(*radius, "fill circle", report);
            }
            DrawCommand::StrokeCircle {
                center,
                radius,
                width,
                ..
            } => {
                check_vec(*center, "circle center", report);
                check_radius(*radius, "stroke circle", report);
                check_stroke_width(*width, "stroke circle", report);
            }
            DrawCommand::FillRoundedRect { rect, radius, .. } => {
                if check_rect(*rect, "fill rounded rect", report) && is_degenerate(*rect) {
                    report.report(
                        FindingCode::DegenerateRect,
                        "fill rounded rect has zero/negative area",
                    );
                }
                check_radius(*radius, "fill rounded rect", report);
            }
            DrawCommand::StrokeRoundedRect {
                rect,
                radius,
                width,
                ..
            } => {
                if check_rect(*rect, "stroke rounded rect", report) && is_degenerate(*rect) {
                    report.report(
                        FindingCode::DegenerateRect,
                        "stroke rounded rect has zero/negative area",
                    );
                }
                check_radius(*radius, "stroke rounded rect", report);
                check_stroke_width(*width, "stroke rounded rect", report);
            }
            DrawCommand::DrawImage {
                destination,
                source,
                ..
            } => {
                if check_rect(*destination, "image destination", report)
                    && is_degenerate(*destination)
                {
                    report.report(
                        FindingCode::DegenerateRect,
                        "image destination has zero/negative area",
                    );
                }
                if let Some(source) = source {
                    if check_rect(*source, "image source", report) && is_degenerate(*source) {
                        report.report(
                            FindingCode::DegenerateRect,
                            "image source has zero/negative area",
                        );
                    }
                }
            }
            DrawCommand::DrawText {
                text,
                position,
                font_size,
                ..
            } => {
                check_vec(*position, "text position", report);
                if !font_size.is_finite() || *font_size <= 0.0 {
                    report.report(
                        FindingCode::NonFiniteGeometry,
                        format!("text font size {font_size} is not a positive number"),
                    );
                }
                if text.trim().is_empty() {
                    report.report(FindingCode::EmptyText, "text command has no visible glyphs");
                }
            }
        }
    }

    if save_depth != 0 {
        report.report(
            FindingCode::UnbalancedSaveRestore,
            format!("{save_depth} Save(s) without matching Restore"),
        );
    }
}

/// Inspects the frame's timing and structural counters.
pub fn inspect_frame(stats: &FrameStats, config: &InspectionConfig, report: &mut InspectionReport) {
    if stats.frame_ms.is_finite() && stats.frame_ms > config.max_frame_ms {
        let severity = if stats.frame_ms > config.max_frame_ms * 2.0 {
            Severity::Error
        } else {
            Severity::Warning
        };
        report.record(
            FindingCode::FrameTimeBudgetExceeded,
            severity,
            format!(
                "frame {:.1}ms exceeds {:.1}ms budget",
                stats.frame_ms, config.max_frame_ms
            ),
        );
    }

    check_budget(
        stats.counters.entities(),
        config.max_entities,
        FindingCode::EntityBudgetExceeded,
        "scene+ui entities",
        report,
    );
}

fn check_budget(
    value: usize,
    budget: usize,
    code: FindingCode,
    what: &str,
    report: &mut InspectionReport,
) {
    if value <= budget {
        return;
    }
    let severity = if code.escalates() && value > budget * 2 {
        Severity::Error
    } else {
        Severity::Warning
    };
    report.record(
        code,
        severity,
        format!("{value} {what} exceeds budget of {budget}"),
    );
}

fn check_transform(transform: Transform2D, report: &mut InspectionReport) {
    if !vec_finite(transform.x_axis)
        || !vec_finite(transform.y_axis)
        || !vec_finite(transform.origin)
    {
        report.report(
            FindingCode::NonFiniteGeometry,
            "transform contains non-finite values",
        );
    }
}

fn check_rect(rect: Rect, what: &str, report: &mut InspectionReport) -> bool {
    if !vec_finite(rect.origin) || !rect.size.width.is_finite() || !rect.size.height.is_finite() {
        report.report(
            FindingCode::NonFiniteGeometry,
            format!("{what} contains non-finite values"),
        );
        return false;
    }
    true
}

fn check_vec(value: Vec2, what: &str, report: &mut InspectionReport) {
    if !vec_finite(value) {
        report.report(
            FindingCode::NonFiniteGeometry,
            format!("{what} contains non-finite values"),
        );
    }
}

fn check_radius(radius: f32, what: &str, report: &mut InspectionReport) {
    if !radius.is_finite() {
        report.report(
            FindingCode::NonFiniteGeometry,
            format!("{what} radius is not finite"),
        );
    } else if radius <= 0.0 {
        report.report(
            FindingCode::DegenerateCircle,
            format!("{what} radius {radius} is not positive"),
        );
    }
}

fn check_stroke_width(width: f32, what: &str, report: &mut InspectionReport) {
    if !width.is_finite() {
        report.report(
            FindingCode::NonFiniteGeometry,
            format!("{what} stroke width is not finite"),
        );
    } else if width <= 0.0 {
        report.report(
            FindingCode::DegenerateStroke,
            format!("{what} stroke width {width} is not positive"),
        );
    }
}

fn vec_finite(value: Vec2) -> bool {
    value.x.is_finite() && value.y.is_finite()
}

fn is_degenerate(rect: Rect) -> bool {
    rect.size.width <= 0.0 || rect.size.height <= 0.0
}
