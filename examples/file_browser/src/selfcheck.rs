//! 无头自检：同一个视图走完整条管线，然后**读命令**，不看画面。
//!
//! 窗口宿主画进 wgpu 的那一棵 `Browser`，这里画进
//! [`RecordingBackend`] —— 录下这一帧的 viewport 和完整的 `DrawCommand`
//! 序列，也就是真实后端本来要变成像素的那份数据。两层检查读它：
//!
//! 1. `draw_profile::inspect` 做结构体检（退化几何、`NaN`、
//!    `Save`/`Restore` 配不上、opacity 越界、命令预算），按严重度排序。
//! 2. 语义检查看命令**里面**：路径栏、状态行、行文字都得出现，而且**只出现
//!    视口那几行**（这一条就是虚拟化的证据 —— 清单里放着 5 000 行，画出来
//!    的却只有二十几行的文字）。
//!
//! 同一套检查跑两种方式：
//!
//! - `file_browser --selfcheck` 打印报告，有 Error 级发现或非零失败就退出 1。
//! - `cargo test --manifest-path examples/file_browser/Cargo.toml` 断言同样
//!   的条件，不开窗就能抓到 UI 回归。

use draw_backend_recording::{RecordedFrame, RecordingBackend};
use draw_core::{Size, Vec2, ViewportSize};
use draw_profile::{inspect, FrameCounters, FrameStats, InspectionReport, Severity};
use draw_render::{DrawCommand, PaintContext, RenderBackend};
use draw_theme::Theme;

use crate::scan::{Entry, Listing};
use crate::ui::{Browser, ROW_HEIGHT};

/// 自检用的窗口尺寸（跟宿主的初始窗口一致）。
const WIDTH: f32 = 900.0;
const HEIGHT: f32 = 620.0;

/// 清单里放这么多行，但画出来的只有视口那二十几行 —— 数字差就是这份自检
/// 要证明的事。
const ROWS: usize = 5_000;

/// 一份不碰磁盘的清单：每 10 行一个目录。
fn fixture(count: usize) -> Listing {
    let entries = (0..count)
        .map(|index| Entry {
            name: format!("entry_{index:05}"),
            is_dir: index % 10 == 0,
            size: (index as u64) * 1_537,
            modified: Some(1_789_886_988 + index as u64 * 61),
        })
        .collect();
    Listing::fixture("/Users/suke/Documents/quill/examples", entries)
}

/// 走一遍真实的帧生命周期：建视图 -> 收清单 -> 排布 -> 绘制 -> 录下来。
fn record(mut app: Browser) -> (Browser, RecordedFrame) {
    let viewport = ViewportSize::new(Size::new(WIDTH, HEIGHT));
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

fn browser(rows: usize) -> Browser {
    let mut app = Browser::new(Theme::dark(), std::path::PathBuf::from("/tmp"), false);
    app.apply_listing(fixture(rows));
    app.layout(ViewportSize::new(Size::new(WIDTH, HEIGHT)));
    app
}

/// 体检：这一帧有没有结构性问题（无头帧没有有意义的墙钟时间，所以计时留 0）。
fn inspect_frame(app: &Browser, frame: &RecordedFrame) -> InspectionReport {
    let mut stats = FrameStats::new(0);
    stats.counters = FrameCounters::new(0, app.control_count(), frame.command_count(), 1);
    inspect(&frame.draw_list, &stats)
}

/// 所有 `DrawText` 的文字。
fn texts(frame: &RecordedFrame) -> Vec<String> {
    frame
        .commands()
        .iter()
        .filter_map(|command| match command {
            DrawCommand::DrawText { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

/// 文字的绘制位置（基线原点）。
fn text_positions(frame: &RecordedFrame) -> Vec<Vec2> {
    frame
        .commands()
        .iter()
        .filter_map(|command| match command {
            DrawCommand::DrawText { position, .. } => Some(*position),
            _ => None,
        })
        .collect()
}

fn contains(frame: &RecordedFrame, needle: &str) -> bool {
    texts(frame).iter().any(|text| text.contains(needle))
}

/// 位置在窗口内（留 0.5 的抗锯齿余量）。
fn on_screen(point: Vec2) -> bool {
    point.x >= -0.5 && point.y >= -0.5 && point.x <= WIDTH + 0.5 && point.y <= HEIGHT + 0.5
}

/// 一帧里有几对 Save/ClipRect —— 列表容器裁剪自己的证据。
fn clip_count(frame: &RecordedFrame) -> usize {
    frame
        .commands()
        .iter()
        .filter(|command| matches!(command, DrawCommand::ClipRect(_)))
        .count()
}

// -- 报告 ------------------------------------------------------------------

/// 跑全套检查，返回（失败数，报告文本）。
pub fn check() -> (usize, String) {
    let mut out = String::new();
    let mut failures = 0usize;
    /// 记一次失败：报告里写一行，退出码加一。
    fn fail(out: &mut String, failures: &mut usize, message: String) {
        out.push_str(&format!("  FAIL  {message}\n"));
        *failures += 1;
    }

    let app = browser(ROWS);
    let (app, frame) = record(app);

    out.push_str("file_browser 无头自检\n");
    out.push_str(&format!(
        "  视图：{:.0}x{:.0}  清单 {} 行  行高 {:.0}\n",
        WIDTH, HEIGHT, ROWS, ROW_HEIGHT
    ));
    out.push_str(&format!(
        "  控件 {}  行池 {}  视口行 {:?}  命令 {}\n",
        app.control_count(),
        app.pool_size(),
        app.visible_range(),
        frame.command_count()
    ));

    // -- 结构体检 --
    let report = inspect_frame(&app, &frame);
    let errors = report.count_of(Severity::Error);
    let warnings = report.count_of(Severity::Warning);
    out.push_str(&format!("  体检：{errors} error / {warnings} warning\n"));
    for finding in report.findings() {
        out.push_str(&format!(
            "    [{:?}] {}\n",
            finding.severity,
            finding.summary()
        ));
    }
    if errors > 0 {
        fail(&mut out, &mut failures, format!("体检有 {errors} 项 Error"));
    }

    // 收下清单之后就不该还在"读取中"，而且新清单默认选中第一行。
    if !app.is_loading() && app.selected_index() == Some(0) {
        out.push_str("  ok    收下清单后不再是读取中，且选中第一行\n");
    } else {
        fail(
            &mut out,
            &mut failures,
            format!(
                "收下清单后状态不对：loading={} selected={:?}",
                app.is_loading(),
                app.selected_index()
            ),
        );
    }

    // -- 语义：该出现的都出现了 --
    for needle in [
        "examples",
        "5000 项",
        "entry_00000/",
        "entry_00001",
        "↑↓ 选择",
    ] {
        if contains(&frame, needle) {
            out.push_str(&format!("  ok    文字含 `{needle}`\n"));
        } else {
            fail(&mut out, &mut failures, format!("文字里找不到 `{needle}`"));
        }
    }

    // -- 虚拟化：画出来的行数只跟视口有关 --
    let drawn_rows = texts(&frame)
        .iter()
        .filter(|text| text.starts_with("entry_"))
        .count();
    let expected = (HEIGHT / ROW_HEIGHT).ceil() as usize + 2;
    if drawn_rows <= expected {
        out.push_str(&format!(
            "  ok    画了 {drawn_rows} 行（<= {expected}），清单有 {ROWS} 行\n"
        ));
    } else {
        fail(
            &mut out,
            &mut failures,
            format!("画了 {drawn_rows} 行，超过视口上限 {expected} —— 行池没起作用"),
        );
    }
    if !contains(&frame, "entry_04999") {
        out.push_str("  ok    最后一行的文字根本没被画出来\n");
    } else {
        fail(
            &mut out,
            &mut failures,
            "视口外的行也被画了 —— 虚拟化失效".to_string(),
        );
    }

    // -- 命令数不随数据量走 --
    let huge = browser(200_000);
    let (_, huge_frame) = record(huge);
    if huge_frame.command_count() == frame.command_count() {
        out.push_str(&format!(
            "  ok    5 000 行和 200 000 行命令数一样（{}）\n",
            frame.command_count()
        ));
    } else {
        fail(
            &mut out,
            &mut failures,
            format!(
                "命令数跟着数据量走了：{} vs {}",
                frame.command_count(),
                huge_frame.command_count()
            ),
        );
    }

    // -- 裁剪 --
    if clip_count(&frame) > 0 {
        out.push_str(&format!(
            "  ok    列表容器发了 {} 条 clip\n",
            clip_count(&frame)
        ));
    } else {
        fail(&mut out, &mut failures, "列表容器没有裁剪自己".to_string());
    }

    // -- 几何：文字都在窗口里 --
    let outside = text_positions(&frame)
        .into_iter()
        .filter(|point| !on_screen(*point))
        .count();
    if outside == 0 {
        out.push_str("  ok    所有文字都在窗口内\n");
    } else {
        fail(
            &mut out,
            &mut failures,
            format!("有 {outside} 条文字画到了窗口外"),
        );
    }

    // -- 滚动：走滚轮那条路径（Wheel -> 最近祖先的滚动回调）--
    let mut scrolled = browser(ROWS);
    let viewport = ViewportSize::new(Size::new(WIDTH, HEIGHT));
    let before = scrolled.offset();
    scrolled.event(&draw_core::InputEvent::Wheel {
        position: Vec2::new(WIDTH / 2.0, HEIGHT / 2.0),
        delta: Vec2::new(0.0, 10.0 * ROW_HEIGHT),
    });
    scrolled.layout(viewport);
    let after = scrolled.offset();
    if after > before {
        out.push_str(&format!(
            "  ok    滚轮滚了 {:.0}px（{before:.0} -> {after:.0}）\n",
            after - before
        ));
    } else {
        fail(
            &mut out,
            &mut failures,
            format!("滚轮没滚动：{before} -> {after}"),
        );
    }
    if scrolled.visible_range().start > 0 {
        out.push_str(&format!(
            "  ok    视口行跟着挪到 {:?}\n",
            scrolled.visible_range()
        ));
    } else {
        fail(&mut out, &mut failures, "滚动后视口行区间没变".to_string());
    }

    if failures == 0 {
        out.push_str("\n自检通过\n");
    } else {
        out.push_str(&format!("\n自检失败：{failures} 项\n"));
    }
    (failures, out)
}

/// `--dump` 的入口：把这一帧的每条命令打出来。
///
/// 这是"读"一帧的方式 —— 命令里带着 rect、颜色和文字，所以布局和内容都
/// 能直接读出来，不需要截图。
pub fn dump_commands() -> String {
    let mut out = String::from("\n绘制命令\n");
    let (_, frame) = record(browser(ROWS));
    for (index, command) in frame.commands().iter().enumerate() {
        out.push_str(&format!("  {index:4}  {}\n", summarize(command)));
    }
    out
}

/// 一条命令的一行摘要。
fn summarize(command: &DrawCommand) -> String {
    match command {
        DrawCommand::Save => "Save".to_string(),
        DrawCommand::Restore => "Restore".to_string(),
        DrawCommand::SetTransform(_) => "SetTransform".to_string(),
        DrawCommand::SetOpacity(value) => format!("SetOpacity {value:.2}"),
        DrawCommand::ClipRect(rect) => format!("ClipRect {}", rect_text(*rect)),
        DrawCommand::FillRect { rect, .. } => format!("FillRect {}", rect_text(*rect)),
        DrawCommand::StrokeRect { rect, .. } => format!("StrokeRect {}", rect_text(*rect)),
        DrawCommand::Line { .. } => "Line".to_string(),
        DrawCommand::FillCircle { .. } => "FillCircle".to_string(),
        DrawCommand::StrokeCircle { .. } => "StrokeCircle".to_string(),
        DrawCommand::FillRoundedRect { rect, .. } => {
            format!("FillRoundedRect {}", rect_text(*rect))
        }
        DrawCommand::StrokeRoundedRect { rect, .. } => {
            format!("StrokeRoundedRect {}", rect_text(*rect))
        }
        DrawCommand::DrawImage { destination, .. } => {
            format!("DrawImage {}", rect_text(*destination))
        }
        DrawCommand::DrawText { text, position, .. } => {
            format!("DrawText {:>6.1},{:>6.1}  {text}", position.x, position.y)
        }
    }
}

/// `x,y w x h` —— 读布局时最关心的四个数。
fn rect_text(rect: draw_core::Rect) -> String {
    format!(
        "{:>6.1},{:>6.1} {:>6.1}x{:>6.1}",
        rect.origin.x, rect.origin.y, rect.size.width, rect.size.height
    )
}

/// `--selfcheck` / `--dump` 的入口：打印报告，失败退出 1。
pub fn run(dump: bool) -> i32 {
    let (failures, report) = check();
    print!("{report}");
    if dump {
        print!("{}", dump_commands());
    }
    if failures == 0 {
        0
    } else {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `cargo test` 里跑同一套断言 —— UI 回归不必开窗就能抓到。
    #[test]
    fn the_headless_check_passes() {
        let (failures, report) = check();
        assert_eq!(failures, 0, "自检报告：\n{report}");
    }

    #[test]
    fn the_frame_is_structurally_clean() {
        let app = browser(ROWS);
        let (app, frame) = record(app);
        let report = inspect_frame(&app, &frame);
        assert_eq!(
            report.count_of(Severity::Error),
            0,
            "Error 级发现：{:?}",
            report
                .findings()
                .iter()
                .map(|finding| finding.summary())
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn the_header_and_status_are_on_screen() {
        let (_, frame) = record(browser(ROWS));
        assert!(contains(&frame, "examples"), "路径栏");
        assert!(contains(&frame, "5000 项"), "状态行");
    }

    /// 这就是虚拟化的证据：清单 5 000 行，画出来的只有二十几行。
    #[test]
    fn only_the_viewports_rows_are_drawn() {
        let (_, frame) = record(browser(ROWS));
        let drawn = texts(&frame)
            .iter()
            .filter(|text| text.starts_with("entry_"))
            .count();
        assert!(
            drawn <= (HEIGHT / ROW_HEIGHT).ceil() as usize + 2,
            "画了 {drawn} 行"
        );
        assert!(!contains(&frame, "entry_04999"), "视口外的行不该被画");
    }

    /// 数据量大 40 倍，命令数一模一样。
    #[test]
    fn command_count_is_flat_in_the_row_count() {
        let (_, small) = record(browser(5_000));
        let (_, huge) = record(browser(200_000));
        assert_eq!(small.command_count(), huge.command_count());
    }

    #[test]
    fn the_list_clips_itself() {
        let (_, frame) = record(browser(ROWS));
        assert!(clip_count(&frame) > 0, "列表容器应该裁剪");
        // 每个 clip 都要有配对的 save/restore —— 体检已经查过，这里只确认
        // 命令序列里确实成对出现。
        let saves = frame
            .commands()
            .iter()
            .filter(|command| matches!(command, DrawCommand::Save))
            .count();
        let restores = frame
            .commands()
            .iter()
            .filter(|command| matches!(command, DrawCommand::Restore))
            .count();
        assert_eq!(saves, restores, "save/restore 要配对");
    }

    #[test]
    fn everything_stays_inside_the_window() {
        let (_, frame) = record(browser(ROWS));
        for point in text_positions(&frame) {
            assert!(on_screen(point), "文字画到了窗口外：{point:?}");
        }
    }

    /// 滚轮 -> `draw_ui::handle_input` -> 列表的滚动回调。核心路由通了，宿主
    /// 只要把平台滚轮造成 `InputEvent::Wheel` 就行（见 `host::wheel_pixels`）。
    #[test]
    fn the_wheel_scrolls_the_list() {
        let mut app = browser(ROWS);
        let viewport = ViewportSize::new(Size::new(WIDTH, HEIGHT));
        app.event(&draw_core::InputEvent::Wheel {
            position: Vec2::new(WIDTH / 2.0, HEIGHT / 2.0),
            delta: Vec2::new(0.0, 10.0 * ROW_HEIGHT),
        });
        app.layout(viewport);
        assert!(app.offset() > 0.0, "滚轮应该滚下去");
        assert!(app.visible_range().start > 0);
    }

    /// 滚到一半再滚：行池大小不变，只是行被重用。
    #[test]
    fn scrolling_reuses_the_pool() {
        let mut app = browser(ROWS);
        let viewport = ViewportSize::new(Size::new(WIDTH, HEIGHT));
        let pool = app.pool_size();
        for _ in 0..20 {
            app.scroll_by(2.5 * ROW_HEIGHT);
            app.layout(viewport);
        }
        assert_eq!(app.pool_size(), pool);
        assert_eq!(app.control_count(), app.control_count());
    }
}
