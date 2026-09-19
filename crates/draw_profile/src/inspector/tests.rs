use super::*;
use draw_core::{Color, Size, Transform2D};
use draw_render::{Paint, TextAlign};

fn rect(w: f32, h: f32) -> Rect {
    Rect::from_min_size(Vec2::ZERO, Size::new(w, h))
}

fn stats(frame_ms: f32, entities: usize) -> FrameStats {
    let mut stats = FrameStats::new(0);
    stats.frame_ms = frame_ms;
    stats.counters.scene_nodes = entities;
    stats
}

fn clean_stats() -> FrameStats {
    stats(8.0, 10)
}

#[test]
fn clean_list_produces_no_findings() {
    let list = DrawList::from(vec![
        DrawCommand::FillRect {
            rect: rect(10.0, 10.0),
            paint: Paint::solid(Color::RED),
        },
        DrawCommand::Save,
        DrawCommand::SetOpacity(0.5),
        DrawCommand::FillCircle {
            center: Vec2::new(5.0, 5.0),
            radius: 3.0,
            paint: Paint::solid(Color::BLUE),
        },
        DrawCommand::Restore,
        DrawCommand::DrawText {
            text: "hi".into(),
            position: Vec2::new(1.0, 2.0),
            font_size: 12.0,
            align: TextAlign::Left,
            paint: Paint::solid(Color::WHITE),
        },
    ]);
    let report = inspect(&list, &clean_stats());
    assert!(report.is_clean(), "unexpected: {:?}", report.findings());
    assert_eq!(report.max_severity(), None);
    assert_eq!(report.total(), 0);
}

#[test]
fn empty_list_is_info() {
    let report = inspect(&DrawList::new(), &clean_stats());
    assert!(report.has(FindingCode::EmptyDrawList));
    assert_eq!(
        report.find(FindingCode::EmptyDrawList).unwrap().severity,
        Severity::Info
    );
    assert_eq!(report.max_severity(), Some(Severity::Info));
}

#[test]
fn unbalanced_save_restore_is_error() {
    let list = DrawList::from(vec![DrawCommand::Save, DrawCommand::Save]);
    let report = inspect(&list, &clean_stats());
    let finding = report.find(FindingCode::UnbalancedSaveRestore).unwrap();
    assert_eq!(finding.severity, Severity::Error);
    assert_eq!(finding.count, 1);
    assert_eq!(report.max_severity(), Some(Severity::Error));
}

#[test]
fn unmatched_restore_is_error() {
    let list = DrawList::from(vec![DrawCommand::Restore]);
    let report = inspect(&list, &clean_stats());
    assert!(report.has(FindingCode::UnmatchedRestore));
}

#[test]
fn degenerate_geometry_is_aggregated() {
    let list = DrawList::from(vec![
        DrawCommand::FillRect {
            rect: rect(0.0, 10.0),
            paint: Paint::solid(Color::RED),
        },
        DrawCommand::StrokeRect {
            rect: rect(10.0, -1.0),
            paint: Paint::solid(Color::RED),
            width: 1.0,
        },
        DrawCommand::FillCircle {
            center: Vec2::ZERO,
            radius: -2.0,
            paint: Paint::solid(Color::RED),
        },
        DrawCommand::StrokeRect {
            rect: rect(5.0, 5.0),
            paint: Paint::solid(Color::RED),
            width: 0.0,
        },
    ]);
    let report = inspect(&list, &clean_stats());
    assert_eq!(report.find(FindingCode::DegenerateRect).unwrap().count, 2);
    assert_eq!(report.find(FindingCode::DegenerateCircle).unwrap().count, 1);
    assert_eq!(report.find(FindingCode::DegenerateStroke).unwrap().count, 1);
    assert_eq!(report.max_severity(), Some(Severity::Warning));
}

#[test]
fn non_finite_geometry_is_error() {
    let list = DrawList::from(vec![DrawCommand::FillRect {
        rect: rect(f32::NAN, 10.0),
        paint: Paint::solid(Color::RED),
    }]);
    let report = inspect(&list, &clean_stats());
    assert_eq!(
        report
            .find(FindingCode::NonFiniteGeometry)
            .unwrap()
            .severity,
        Severity::Error
    );
    // a non-finite rect is not *also* reported as degenerate
    assert!(!report.has(FindingCode::DegenerateRect));
}

#[test]
fn opacity_out_of_range_is_warned() {
    let list = DrawList::from(vec![
        DrawCommand::SetOpacity(1.5),
        DrawCommand::SetOpacity(-0.1),
        DrawCommand::SetOpacity(f32::NAN),
    ]);
    let report = inspect(&list, &clean_stats());
    assert_eq!(
        report.find(FindingCode::OpacityOutOfRange).unwrap().count,
        3
    );
}

#[test]
fn empty_text_is_info() {
    let list = DrawList::from(vec![DrawCommand::DrawText {
        text: "   ".into(),
        position: Vec2::ZERO,
        font_size: 12.0,
        align: TextAlign::Left,
        paint: Paint::solid(Color::WHITE),
    }]);
    let report = inspect(&list, &clean_stats());
    assert!(report.has(FindingCode::EmptyText));
    assert_eq!(report.max_severity(), Some(Severity::Info));
}

#[test]
fn command_budget_escalates_from_warning_to_error() {
    let config = InspectionConfig {
        max_draw_commands: 2,
        ..InspectionConfig::default()
    };
    let list = |n: usize| {
        DrawList::from(
            (0..n)
                .map(|_| DrawCommand::FillRect {
                    rect: rect(1.0, 1.0),
                    paint: Paint::solid(Color::RED),
                })
                .collect::<Vec<_>>(),
        )
    };

    let warning = inspect_with(&list(3), &clean_stats(), &config);
    assert_eq!(
        warning
            .find(FindingCode::CommandBudgetExceeded)
            .unwrap()
            .severity,
        Severity::Warning
    );

    let error = inspect_with(&list(5), &clean_stats(), &config);
    assert_eq!(
        error
            .find(FindingCode::CommandBudgetExceeded)
            .unwrap()
            .severity,
        Severity::Error
    );
}

#[test]
fn frame_time_and_entity_budgets() {
    let list = DrawList::from(vec![DrawCommand::FillRect {
        rect: rect(1.0, 1.0),
        paint: Paint::solid(Color::RED),
    }]);
    let config = InspectionConfig {
        max_frame_ms: 10.0,
        max_entities: 5,
        ..InspectionConfig::default()
    };

    let report = inspect_with(&list, &stats(25.0, 100), &config);
    assert_eq!(
        report
            .find(FindingCode::FrameTimeBudgetExceeded)
            .unwrap()
            .severity,
        Severity::Error
    );
    assert!(report.has(FindingCode::EntityBudgetExceeded));
}

#[test]
fn finding_summary_includes_count() {
    let mut report = InspectionReport::new();
    report.report(FindingCode::DegenerateRect, "bad rect");
    report.report(FindingCode::DegenerateRect, "bad rect");
    let finding = report.find(FindingCode::DegenerateRect).unwrap();
    assert_eq!(finding.count, 2);
    assert_eq!(finding.summary(), "bad rect (x2)");
}

#[test]
fn severity_ordering_and_counts() {
    let mut report = InspectionReport::new();
    report.report(FindingCode::EmptyText, "x");
    report.report(FindingCode::DegenerateRect, "y");
    report.report(FindingCode::UnmatchedRestore, "z");
    assert_eq!(report.count_of(Severity::Info), 1);
    assert_eq!(report.count_of(Severity::Warning), 1);
    assert_eq!(report.count_of(Severity::Error), 1);
    assert_eq!(report.total(), 3);
    assert_eq!(report.max_severity(), Some(Severity::Error));
}

#[test]
fn transform_and_clip_are_checked() {
    let list = DrawList::from(vec![
        DrawCommand::SetTransform(Transform2D::from_translation(Vec2::new(f32::INFINITY, 0.0))),
        DrawCommand::ClipRect(rect(0.0, 0.0)),
    ]);
    let report = inspect(&list, &clean_stats());
    assert!(report.has(FindingCode::NonFiniteGeometry));
    assert!(report.has(FindingCode::DegenerateClip));
}
