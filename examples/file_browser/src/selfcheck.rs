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
use draw_core::{InputEvent, PointerButton, Size, Vec2, ViewportSize};
use draw_profile::{inspect, FrameCounters, FrameStats, InspectionReport, Severity};
use draw_render::{DrawCommand, PaintContext, RenderBackend};
use draw_theme::Theme;

use crate::preview::{self, Preview};
use crate::scan::{Entry, Listing};
use crate::ui::{Browser, MAIN_WIDTH, PREVIEW_MIN, RESIZE_GUTTER, ROW_HEIGHT};

/// 自检用的窗口尺寸（跟宿主的初始窗口一致）。
const WIDTH: f32 = 1100.0;
const HEIGHT: f32 = 680.0;

/// 预览栏里放这么多字节：64 KiB = 4 096 行 hex，画出来的却只有视口那几十行。
const PREVIEW_BYTES: usize = 64 * 1024;

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

/// 含 `needle` 的那条文字画在什么位置。用来断言"这段内容属于哪一栏"。
fn position_of(frame: &RecordedFrame, needle: &str) -> Option<Vec2> {
    frame.commands().iter().find_map(|command| match command {
        DrawCommand::DrawText { text, position, .. } if text.contains(needle) => Some(*position),
        _ => None,
    })
}

/// 一个选中了文件、右栏放着字节的浏览器（不碰磁盘：`fixture` 直接给字节）。
fn previewed(bytes: Vec<u8>) -> Browser {
    let mut app = browser(ROWS);
    // 第 1 行是文件（每 10 行一个目录），选中它才会请求预览。
    app.select(1);
    app.apply_preview(Preview::fixture("sample.bin", bytes));
    app.layout(ViewportSize::new(Size::new(WIDTH, HEIGHT)));
    app
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
    out.push_str(&format!(
        "  分栏：主栏 {:.0} + 把手 {:.0} + 预览栏 {:.0}\n",
        app.main_width(),
        RESIZE_GUTTER,
        app.preview_width()
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
    // 落在**列表**上（主栏中间），不是落在右栏或者分隔条上 —— 滚轮是沿祖先链
    // 找滚动回调的，点在空地上就没人接。
    let over_list = Vec2::new(MAIN_WIDTH / 2.0, HEIGHT / 2.0);
    scrolled.event(&draw_core::InputEvent::Wheel {
        position: over_list,
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

    // -- 分栏：拖那条分隔条，改的是右栏的宽度 --
    let mut split = browser(ROWS);
    let before = split.preview_width();
    let gutter = Vec2::new(split.main_width() + RESIZE_GUTTER / 2.0, 360.0);
    split.event(&InputEvent::PointerDown {
        position: gutter,
        button: PointerButton::Left,
    });
    split.event(&InputEvent::PointerMove {
        position: gutter + Vec2::new(60.0, 0.0),
    });
    split.event(&InputEvent::PointerUp {
        position: gutter + Vec2::new(60.0, 0.0),
        button: PointerButton::Left,
    });
    split.layout(viewport);
    let after = split.preview_width();
    if (before - after - 60.0).abs() < 1e-3 {
        out.push_str(&format!(
            "  ok    拖 60px：预览栏 {before:.0} -> {after:.0}（主栏 {:.0}）\n",
            split.main_width()
        ));
    } else {
        fail(
            &mut out,
            &mut failures,
            format!("拖动没生效：预览栏 {before} -> {after}，应该正好少 60"),
        );
    }

    // 分隔条管不了"窗口变窄了" —— 那一半由 layout 补，否则右栏会被挤成 0 宽。
    split.layout(ViewportSize::new(Size::new(600.0, 600.0)));
    if (split.preview_width() - PREVIEW_MIN).abs() < 1e-3 {
        out.push_str(&format!(
            "  ok    窗口窄到 600 时预览栏仍保留 {:.0}px\n",
            split.preview_width()
        ));
    } else {
        fail(
            &mut out,
            &mut failures,
            format!(
                "窗口变窄后预览栏是 {}，应该被保住 {}",
                split.preview_width(),
                PREVIEW_MIN
            ),
        );
    }

    // -- 二进制预览：右栏把字节画成 hex --
    let (_, preview_frame) = record(previewed(b"Hello, world!\n".to_vec()));
    for needle in ["sample.bin", "00000000", "48 65 6c 6c 6f", "Hello, world!."] {
        if contains(&preview_frame, needle) {
            out.push_str(&format!("  ok    右栏含 `{needle}`\n"));
        } else {
            fail(&mut out, &mut failures, format!("右栏里找不到 `{needle}`"));
        }
    }

    // 右栏的内容确实画在右栏里（x 越过主栏右边界），不是混在左栏。
    match position_of(&preview_frame, "00000000") {
        Some(point) if point.x > MAIN_WIDTH => {
            out.push_str(&format!("  ok    hex 行画在右栏（x={:.0}）\n", point.x));
        }
        other => fail(
            &mut out,
            &mut failures,
            format!("hex 行没画在右栏：{other:?}（主栏右边界 {MAIN_WIDTH}）"),
        ),
    }

    // 64 KiB = 4 096 行数据，画出来的只有视口那几十行。
    let big = previewed(vec![0x5a; PREVIEW_BYTES]);
    let (big, big_frame) = record(big);
    let hex_rows = texts(&big_frame)
        .iter()
        .filter(|text| text.len() == 8 && text.chars().all(|ch| ch.is_ascii_hexdigit()))
        .count();
    let hex_budget = (HEIGHT / crate::ui::PREVIEW_ROW_HEIGHT).ceil() as usize + 2;
    if big.preview_rows() == PREVIEW_BYTES / preview::BYTES_PER_ROW && hex_rows <= hex_budget {
        out.push_str(&format!(
            "  ok    {:.0} KiB 预览有 {} 行数据，只画了 {hex_rows} 行（<= {hex_budget}）\n",
            PREVIEW_BYTES as f32 / 1024.0,
            big.preview_rows()
        ));
    } else {
        fail(
            &mut out,
            &mut failures,
            format!(
                "预览没被虚拟化：{} 行数据画了 {hex_rows} 行（上限 {hex_budget}）",
                big.preview_rows()
            ),
        );
    }
    if !contains(&big_frame, "0000fff0") {
        out.push_str("  ok    最后一行的偏移量根本没被画出来\n");
    } else {
        fail(
            &mut out,
            &mut failures,
            "视口外的 hex 行也被画了 —— 虚拟化失效".to_string(),
        );
    }
    let grown = big.control_count() - app.control_count();
    if grown < 150 {
        out.push_str(&format!(
            "  ok    4 096 行数据只让树多了 {grown} 个控件（池 {}）\n",
            big.preview_pool_size()
        ));
    } else {
        fail(
            &mut out,
            &mut failures,
            format!("预览让控件数涨了 {grown} 个 —— 像是把数据全挂上了"),
        );
    }

    // -- 文本模式：同一份字节，按换行看 --
    let mut texted = previewed(b"Hello, world!\nsecond line\nthird\n".to_vec());
    // 两种模式同时在树上，但藏起来的那个一行都不挂：它的容器高度是 0，
    // `ListState::sync` 直接返回。
    if texted.idle_pool_size() == 0 {
        out.push_str(&format!(
            "  ok    待命的文本列表一行都没挂（在用 {} 行）\n",
            texted.preview_pool_size()
        ));
    } else {
        fail(
            &mut out,
            &mut failures,
            format!(
                "藏起来的那个列表也挂了 {} 行 —— 两种模式同时在花钱",
                texted.idle_pool_size()
            ),
        );
    }
    let hex_pool = texted.preview_pool_size();
    out.push_str(&format!("  右栏副标题：{}\n", texted.preview_detail()));

    // 走真实的点击路径（点"文本"那个按钮），不是直接调 `set_mode` —— 这样
    // "按钮在那儿、点得动"也被验证了。
    let tab = texted
        .tab_center(preview::PreviewMode::Text)
        .expect("切换按钮已经排布过了");
    texted.event(&InputEvent::PointerDown {
        position: tab,
        button: PointerButton::Left,
    });
    texted.event(&InputEvent::PointerUp {
        position: tab,
        button: PointerButton::Left,
    });
    texted.update();
    texted.layout(viewport);
    if texted.mode() == preview::PreviewMode::Text && texted.preview_rows() == 3 {
        out.push_str(&format!(
            "  ok    点\"文本\"按钮：模式切过去了，{} 行文本\n",
            texted.preview_rows()
        ));
    } else {
        fail(
            &mut out,
            &mut failures,
            format!(
                "点了按钮没切成文本模式：{:?} / {} 行",
                texted.mode(),
                texted.preview_rows()
            ),
        );
    }
    let (texted, text_frame) = record(texted);
    for needle in ["Hello, world!", "second line", "third"] {
        if contains(&text_frame, needle) {
            out.push_str(&format!("  ok    文本模式含 `{needle}`\n"));
        } else {
            fail(
                &mut out,
                &mut failures,
                format!("文本模式里找不到 `{needle}`"),
            );
        }
    }
    // 行尾的换行符不该被画成一个 `·` —— 换行是分隔，不是内容。
    if !contains(&text_frame, "!·") {
        out.push_str("  ok    换行符没被当成内容画出来\n");
    } else {
        fail(&mut out, &mut failures, "行尾多画了一个换行符".to_string());
    }
    // 换过去之后 hex 那几列就不该还在画面上了。
    if !contains(&text_frame, "48 65 6c 6c 6f") {
        out.push_str("  ok    hex 那几列被换掉了\n");
    } else {
        fail(
            &mut out,
            &mut failures,
            "切到文本模式了还画着 hex 列".to_string(),
        );
    }
    // 藏起来的那个不再长：hex 的行池停在切换前的规模，没有跟着文本模式的数据走。
    let idle = texted.idle_pool_size();
    if idle == hex_pool {
        out.push_str(&format!(
            "  ok    切过去后 hex 的行池停在 {idle} 行，没再长\n"
        ));
    } else {
        fail(
            &mut out,
            &mut failures,
            format!("藏起来的 hex 列表从 {hex_pool} 行长到了 {idle} 行"),
        );
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
            position: Vec2::new(MAIN_WIDTH / 2.0, HEIGHT / 2.0),
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

    /// 右栏把选中文件的字节画成 hex：三列都在。
    #[test]
    fn the_preview_pane_draws_the_bytes() {
        let (_, frame) = record(previewed(b"Hello, world!\n".to_vec()));
        assert!(contains(&frame, "sample.bin"), "右栏标题");
        assert!(contains(&frame, "00000000"), "偏移量那一列");
        assert!(contains(&frame, "48 65 6c 6c 6f"), "十六进制那一列");
        assert!(contains(&frame, "Hello, world!."), "ascii 那一列");
    }

    /// 右栏的内容画在主栏**右边** —— 两栏没叠在一起。
    #[test]
    fn the_preview_pane_is_on_the_right() {
        let (_, frame) = record(previewed(b"Hello, world!\n".to_vec()));
        let point = position_of(&frame, "00000000").expect("画了 hex 行");
        assert!(
            point.x > MAIN_WIDTH,
            "hex 行应该在 x>{MAIN_WIDTH} 的地方，实际 {point:?}"
        );
    }

    /// 64 KiB = 4 096 行数据，画出来的只有视口那几十行。
    #[test]
    fn a_big_preview_draws_only_the_visible_rows() {
        let (app, frame) = record(previewed(vec![0x5a; PREVIEW_BYTES]));
        assert_eq!(app.preview_rows(), PREVIEW_BYTES / preview::BYTES_PER_ROW);
        assert!(!contains(&frame, "0000fff0"), "视口外的 hex 行不该被画");
        assert!(app.preview_pool_size() < 40);
    }

    /// 拖 60px：主栏吃掉这 60px，预览栏让出来 —— 一个手柄调两个栏。
    #[test]
    fn dragging_the_gutter_narrows_the_preview_pane() {
        let mut app = browser(ROWS);
        let before = app.preview_width();
        let gutter = Vec2::new(app.main_width() + RESIZE_GUTTER / 2.0, 360.0);
        app.event(&InputEvent::PointerDown {
            position: gutter,
            button: PointerButton::Left,
        });
        app.event(&InputEvent::PointerMove {
            position: gutter + Vec2::new(60.0, 0.0),
        });
        app.event(&InputEvent::PointerUp {
            position: gutter + Vec2::new(60.0, 0.0),
            button: PointerButton::Left,
        });
        app.layout(ViewportSize::new(Size::new(WIDTH, HEIGHT)));
        assert!((before - app.preview_width() - 60.0).abs() < 1e-3);
    }

    /// 文本模式：同一份字节，按换行看 —— 而且是真的点按钮切过去的。
    #[test]
    fn the_text_mode_shows_lines() {
        let mut app = previewed(b"Hello, world!\nsecond line\n".to_vec());
        assert_eq!(app.idle_pool_size(), 0, "藏起来的文本列表没挂行");
        let tab = app
            .tab_center(preview::PreviewMode::Text)
            .expect("按钮已排布");
        app.event(&InputEvent::PointerDown {
            position: tab,
            button: PointerButton::Left,
        });
        app.event(&InputEvent::PointerUp {
            position: tab,
            button: PointerButton::Left,
        });
        app.update();
        app.layout(ViewportSize::new(Size::new(WIDTH, HEIGHT)));
        assert_eq!(app.mode(), preview::PreviewMode::Text);
        assert_eq!(app.preview_rows(), 2);

        let (_, frame) = record(app);
        assert!(contains(&frame, "Hello, world!"), "第一行原样");
        assert!(contains(&frame, "second line"), "第二行原样");
        assert!(!contains(&frame, "48 65 6c 6c 6f"), "hex 列被换掉了");
    }

    /// 分隔条（`min`/`max`）管不了窗口变窄，那一半由 `layout` 补。
    #[test]
    fn a_narrow_window_keeps_the_preview_pane_alive() {
        let mut app = browser(ROWS);
        app.layout(ViewportSize::new(Size::new(600.0, 600.0)));
        assert!((app.preview_width() - PREVIEW_MIN).abs() < 1e-3);
    }
}
