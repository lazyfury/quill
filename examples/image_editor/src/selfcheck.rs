//! 无头自检：同一棵视图走完整条管线，然后**读命令**，不看画面。
//!
//! 窗口宿主画进 wgpu 的那棵 `EditorView`，这里画进
//! [`RecordingBackend`] —— 录下这一帧的 viewport 和完整的 `DrawCommand`
//! 序列，也就是真实后端本来要变成像素的那份数据。两层检查读它：
//!
//! 1. `draw_profile::inspect` 做结构体检（退化几何、`NaN`、`Save`/`Restore`
//!    配不上、opacity 越界、命令预算），按严重度排序。
//! 2. 语义检查看命令**里面**：菜单栏、工具栏、画布、图层面板、状态栏的文字
//!    都得出现；再点一下画笔按钮，状态栏必须跟着变。
//!
//! 同一套检查跑两种方式：
//!
//! - `image_editor --selfcheck` 打印报告，有 Error 级发现或断言失败就退出 1。
//! - `cargo test --manifest-path examples/image_editor/Cargo.toml` 断言同样
//!   的条件，不开窗就能抓到 UI 回归。

use draw_backend_recording::{RecordedFrame, RecordingBackend};
use draw_core::{InputEvent, PointerButton, Size, Vec2, ViewportSize};
use draw_profile::{inspect_with, FrameCounters, FrameStats, InspectionConfig, Severity};
use draw_render::{DrawCommand, PaintContext, RenderBackend};
use draw_theme::Theme;

use crate::app::state::{ActiveTool, AppState, HistoryAction};
use crate::document::Color;
use crate::ui::EditorView;

/// 自检用的窗口尺寸（跟宿主的初始窗口一致）。
const WIDTH: f32 = 1280.0;
const HEIGHT: f32 = 800.0;

/// 这个界面显示一整套 Lucide 图标（工具栏 + 图标网格），每个图标按矢量重描边
/// 成若干 `Line` / `FillCircle`（不走栅格缓存），所以命令数天然高于普通 UI 的
/// 2048 默认预算；这里把预算抬到 4096 而不是把警告藏掉。
fn inspection_config() -> InspectionConfig {
    InspectionConfig {
        max_draw_commands: 4096,
        ..InspectionConfig::default()
    }
}

fn viewport() -> ViewportSize {
    ViewportSize::new(Size::new(WIDTH, HEIGHT))
}

/// 走一遍真实的帧生命周期：建视图 -> 排布 -> 绘制 -> 录下来。
fn record(mut view: EditorView) -> (EditorView, RecordedFrame) {
    let viewport = viewport();
    view.update();
    view.layout(viewport);

    let mut backend = RecordingBackend::new();
    backend.begin_frame(viewport).expect("begin frame");
    let mut ctx = PaintContext::new();
    view.paint(&mut ctx);
    backend.submit(&ctx.into_draw_list()).expect("submit frame");
    backend.end_frame().expect("end frame");

    let frame = backend.last_frame().expect("a frame was recorded").clone();
    (view, frame)
}

/// 点一下某个工具按钮，模拟真实点击（走 `EditorView::event`）。
fn click_tool(view: &mut EditorView, tool: ActiveTool) {
    let center = view.tool_center(tool).expect("tool button mounted");
    click_at(view, center);
    view.update();
}

/// 在逻辑坐标 `position` 处点一下（走 `EditorView::event`，跟真实鼠标一样）。
fn click_at(view: &mut EditorView, position: Vec2) {
    view.event(&InputEvent::PointerDown {
        position,
        button: PointerButton::Left,
    });
    view.event(&InputEvent::PointerUp {
        position,
        button: PointerButton::Left,
    });
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

fn contains(frame: &RecordedFrame, needle: &str) -> bool {
    texts(frame).iter().any(|text| text.contains(needle))
}

/// 自检主流程：返回（失败数，报告文本）。
pub fn check() -> (usize, String) {
    let mut view = EditorView::new(Theme::dark(), AppState::default());
    view.layout(viewport());
    // 切换工具是真正可用的交互，这里真的点一下。
    click_tool(&mut view, ActiveTool::Brush);

    // Phase 5：在当前图层上画一笔（屏幕 -> 文档坐标由视图内部换算）。
    let brush_point = view
        .canvas_camera()
        .document_to_screen(Vec2::new(400.0, 300.0));
    view.event(&InputEvent::PointerDown {
        position: brush_point,
        button: PointerButton::Left,
    });
    view.event(&InputEvent::PointerMove {
        position: brush_point + Vec2::new(30.0, 0.0),
    });
    view.event(&InputEvent::PointerUp {
        position: brush_point + Vec2::new(30.0, 0.0),
        button: PointerButton::Left,
    });
    view.update();

    // CPU 合成器：一个新文档 + 一笔黑色，应当能合成出对应的像素。
    let mut failures: Vec<String> = Vec::new();
    view.mark_texture_dirty();
    let composite = view.take_texture_upload().expect("脏文档应产出合成结果");
    if composite.get_pixel(0, 0) != Color::WHITE {
        failures.push("CPU 合成结果不是白底".to_string());
    }
    if composite.get_pixel(400, 300) != Color::BLACK {
        failures.push("画笔没有在 (400, 300) 留下像素".to_string());
    }
    if !view.can_undo() {
        failures.push("一笔画笔之后应该能撤销".to_string());
    }

    // Phase 6：一笔 = 一步 undo。点工具栏的“撤销”回到白底，点“重做”再画回来。
    let undo_button = view.history_center(HistoryAction::Undo).expect("撤销按钮");
    click_at(&mut view, undo_button);
    view.update();
    view.mark_texture_dirty();
    let undone = view.take_texture_upload().expect("撤销后应重合成");
    if undone.get_pixel(400, 300) != Color::WHITE {
        failures.push("撤销没有还原画笔笔触".to_string());
    }
    if !view.can_redo() {
        failures.push("撤销之后应该能重做".to_string());
    }

    let redo_button = view.history_center(HistoryAction::Redo).expect("重做按钮");
    click_at(&mut view, redo_button);
    view.update();
    view.mark_texture_dirty();
    let redone = view.take_texture_upload().expect("重做后应重合成");
    if redone.get_pixel(400, 300) != Color::BLACK {
        failures.push("重做没有恢复画笔笔触".to_string());
    }

    let (view, frame) = record(view);
    let camera = view.canvas_camera();

    let mut stats = FrameStats::new(0);
    stats.counters = FrameCounters::new(0, view.control_count(), frame.command_count(), 1);
    let report = inspect_with(&frame.draw_list, &stats, &inspection_config());

    // 结构体检：Error 级即为失败。
    for finding in report
        .findings()
        .iter()
        .filter(|finding| finding.severity == Severity::Error)
    {
        failures.push(format!("结构问题：{}", finding.summary()));
    }

    // 画布：相机可用，图像进了绘制列表。
    if !(camera.zoom.is_finite() && camera.zoom > 0.0) {
        failures.push(format!("相机缩放异常：{}", camera.zoom));
    }
    if !frame
        .commands()
        .iter()
        .any(|command| matches!(command, DrawCommand::DrawImage { .. }))
    {
        failures.push("画布没有发出 DrawImage（文档图像没进绘制列表）".to_string());
    }

    // 语义：每块面板的关键文字都要出现在命令流里。
    let required: [(&str, &str); 12] = [
        ("文件", "菜单栏"),
        ("编辑", "菜单栏"),
        ("帮助", "菜单栏"),
        ("画笔", "工具栏"),
        ("吸管", "工具栏"),
        ("800 × 600", "文档尺寸"),
        ("图层", "图层面板"),
        ("背景", "图层列表"),
        ("100%", "图层不透明度"),
        ("图标", "图标面板"),
        ("属性", "属性面板"),
        ("画笔工具", "状态栏（点过画笔后）"),
    ];
    for (needle, panel) in required {
        if !contains(&frame, needle) {
            failures.push(format!("{panel} 缺少文字：`{needle}`"));
        }
    }

    // 撤销 / 重做把提示写到状态栏（最后一帧应是“重做：画笔”）。
    if !contains(&frame, "重做") {
        failures.push("状态栏缺少撤销/重做提示".to_string());
    }
    let (undo_len, redo_len) = view.history_len();
    if undo_len != 1 || redo_len != 0 {
        failures.push(format!(
            "一笔之后的栈应为 undo=1, redo=0，实际 undo={undo_len}, redo={redo_len}"
        ));
    }

    // 附加：Lucide 图标包已加载，且图标真的描进了命令流。图标用圆头 / 圆角，
    // 会产生 FillCircle；普通 UI（圆角矩形 + 线）不画圆，所以这是个干净信号。
    if view.icon_count() < 20 {
        failures.push(format!("图标包只加载了 {} 个图标", view.icon_count()));
    }
    if !frame
        .commands()
        .iter()
        .any(|command| matches!(command, DrawCommand::FillCircle { .. }))
    {
        failures.push("图标没有画进命令流（没有 FillCircle）".to_string());
    }

    let mut out = String::new();
    let (width, height) = view.document_size();
    out.push_str("image_editor 自检\n");
    out.push_str(&format!(
        "  当前工具 {} · 文档 {width} × {height} · 控件 {} · 命令 {} · 文字 {}\n",
        view.active_tool().label(),
        view.control_count(),
        frame.command_count(),
        texts(&frame).len()
    ));
    out.push_str(&format!(
        "  历史 undo {undo_len} · redo {redo_len} · 图标 {}\n",
        view.icon_count()
    ));
    out.push_str(&format!(
        "  体检：Error {} · Warning {}\n",
        report.count_of(Severity::Error),
        report.count_of(Severity::Warning)
    ));
    for failure in &failures {
        out.push_str(&format!("  ✗ {failure}\n"));
    }
    if failures.is_empty() {
        out.push_str("  ✓ 画布合成 + 撤销/重做 + 图标包 + 菜单/工具栏/图层栏/图标栏/属性栏/状态栏 全部就位\n");
    }
    (failures.len(), out)
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

/// 打印整帧的绘制命令（`--dump` 用；只给一次，方便定位问题）。
fn dump_commands() -> String {
    let (_, frame) = record(EditorView::new(Theme::dark(), AppState::default()));
    let mut out = String::from("\n绘制命令:\n");
    for (index, command) in frame.commands().iter().enumerate() {
        out.push_str(&format!("  {index:>4}  {command:?}\n"));
    }
    out
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
        let (view, frame) = record(EditorView::new(Theme::dark(), AppState::default()));
        let mut stats = FrameStats::new(0);
        stats.counters = FrameCounters::new(0, view.control_count(), frame.command_count(), 1);
        let report = inspect_with(&frame.draw_list, &stats, &inspection_config());
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
    fn every_panel_puts_ink_on_the_frame() {
        let (_, frame) = record(EditorView::new(Theme::dark(), AppState::default()));
        for needle in ["文件", "画笔", "图层", "属性"] {
            assert!(contains(&frame, needle), "缺少 `{needle}`");
        }
        assert!(
            frame
                .commands()
                .iter()
                .any(|command| matches!(command, DrawCommand::DrawImage { .. })),
            "画布应发出 DrawImage"
        );
    }

    #[test]
    fn clicking_the_brush_button_updates_the_status_bar() {
        let mut view = EditorView::new(Theme::dark(), AppState::default());
        view.layout(viewport());
        click_tool(&mut view, ActiveTool::Brush);
        let (_, frame) = record(view);
        assert!(contains(&frame, "画笔工具"), "状态栏应显示画笔工具");
    }

    #[test]
    fn all_text_stays_on_screen() {
        let (_, frame) = record(EditorView::new(Theme::dark(), AppState::default()));
        for command in frame.commands() {
            if let DrawCommand::DrawText { position, .. } = command {
                assert!(
                    position.x >= 0.0
                        && position.y >= 0.0
                        && position.x <= WIDTH
                        && position.y <= HEIGHT,
                    "文字画到了窗口外：{position:?}"
                );
            }
        }
    }
}
