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
use draw_core::{InputEvent, Key, PointerButton, Size, Vec2, ViewportSize};
use draw_profile::{inspect_with, FrameCounters, FrameStats, InspectionConfig, Severity};
use draw_render::{DrawCommand, PaintContext, RenderBackend};

use crate::app::state::{ActiveTool, AppState, HistoryAction};
use crate::document::{Color, PixelBuffer};
use crate::ui::{EditorView, IoAction};

/// 自检用的窗口尺寸（跟宿主的初始窗口一致）。
const WIDTH: f32 = 1280.0;
const HEIGHT: f32 = 800.0;

fn viewport() -> ViewportSize {
    ViewportSize::new(Size::new(WIDTH, HEIGHT))
}

/// 自检用的临时 PNG 路径（进程 id + 计数，避免和已有文件冲突）。
fn temp_png(tag: &str) -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "image_editor_selfcheck_{}_{n}_{tag}.png",
        std::process::id()
    ))
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

/// 点一下选项栏的布尔开关，并 `update` 让状态同步进画笔。
fn click_toggle(view: &mut EditorView, toggle: crate::ui::BrushToggle) {
    let center = view
        .brush_toggle_center(toggle)
        .expect("toggle button mounted");
    click_at(view, center);
    view.update();
}

/// 在一个新视图上验证像素模式：size 3 硬边无灰边，关掉后软边（圆头）有灰边。
fn check_pixel_mode_edges(failures: &mut Vec<String>) {
    let mut view = EditorView::new(crate::theme::editor_theme(false), AppState::default());
    view.layout(viewport());
    view.event(&InputEvent::KeyDown {
        key: Key::Character('b'),
    });
    view.update();
    view.layout(viewport());

    if !view.pixel_mode() {
        failures.push("像素模式默认应为开".to_string());
    }
    while view.brush_size() < 3.0 {
        match view.brush_adjust_center(crate::ui::BrushAdjust::SizeUp) {
            Some(center) => {
                click_at(&mut view, center);
                view.update();
            }
            None => break,
        }
    }

    // 硬边：size 3 不该有抗锯齿灰边。
    let hard = view
        .canvas_camera()
        .document_to_screen(Vec2::new(40.0, 40.0));
    click_at(&mut view, hard);
    view.update();
    view.mark_texture_dirty();
    let frame = view.take_texture_upload().expect("应产出合成结果");
    let gray = gray_pixels(&frame, 40, 40, 3);
    if gray != 0 {
        failures.push(format!("像素模式仍有 {gray} 个抗锯齿灰边像素"));
    }

    // 软边：关掉像素模式，并用圆头（方形的软边在整数尺寸上会饱和成实心）。
    click_toggle(&mut view, crate::ui::BrushToggle::Hard);
    click_toggle(&mut view, crate::ui::BrushToggle::Square);
    if view.pixel_mode() || view.square_mode() {
        failures.push("选项栏开关没有关掉".to_string());
    }
    if view.brush_hard() || view.brush_shape() != crate::tools::BrushShape::Round {
        failures.push("画笔没有同步成软边圆头".to_string());
    }
    let soft = view
        .canvas_camera()
        .document_to_screen(Vec2::new(40.0, 50.0));
    click_at(&mut view, soft);
    view.update();
    view.mark_texture_dirty();
    let frame = view.take_texture_upload().expect("应产出合成结果");
    if gray_pixels(&frame, 40, 50, 3) == 0 {
        failures.push("关掉像素模式后应出现抗锯齿灰边".to_string());
    }
}

fn gray_pixels(pixels: &PixelBuffer, cx: u32, cy: u32, radius: u32) -> usize {
    let mut count = 0;
    let y_end = (cy + radius).min(pixels.height.saturating_sub(1));
    let x_end = (cx + radius).min(pixels.width.saturating_sub(1));
    for y in cy.saturating_sub(radius)..=y_end {
        for x in cx.saturating_sub(radius)..=x_end {
            let color = pixels.get_pixel(x, y);
            if color.r == color.g && color.g == color.b && color.r != 0 && color.r != 255 {
                count += 1;
            }
        }
    }
    count
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

/// 画一帧但不消费视图，供交互序列中反复读命令。
fn paint_frame(view: &EditorView) -> RecordedFrame {
    let viewport = viewport();
    let mut backend = RecordingBackend::new();
    backend.begin_frame(viewport).expect("begin frame");
    let mut ctx = PaintContext::new();
    view.paint(&mut ctx);
    backend.submit(&ctx.into_draw_list()).expect("submit frame");
    backend.end_frame().expect("end frame");
    backend.last_frame().expect("a frame was recorded").clone()
}

/// 一段文字在命令流里的可点击点（基线左侧稍下方，落在行内）。
fn text_center(frame: &RecordedFrame, needle: &str) -> Option<Vec2> {
    frame.commands().iter().find_map(|command| match command {
        DrawCommand::DrawText {
            text,
            position,
            font_size,
            ..
        } if text.contains(needle) => Some(*position + Vec2::new(8.0, -font_size * 0.5)),
        _ => None,
    })
}

/// 自检主流程：返回（失败数，报告文本）。
pub fn check() -> (usize, String) {
    let mut view = EditorView::new(crate::theme::editor_theme(false), AppState::default());
    view.layout(viewport());
    // 切换工具是真正可用的交互，这里真的点一下。
    click_tool(&mut view, ActiveTool::Brush);

    // Phase 5：在当前图层上画一笔（屏幕 -> 文档坐标由视图内部换算）。
    let brush_point = view
        .canvas_camera()
        .document_to_screen(Vec2::new(64.5, 64.5));
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
    if composite.get_pixel(64, 64) != Color::BLACK {
        failures.push("画笔没有在 (64, 64) 留下像素".to_string());
    }
    if !view.can_undo() {
        failures.push("一笔画笔之后应该能撤销".to_string());
    }
    // 一笔 = 一步 undo：此刻栈里应该只有这一笔。
    if view.history_len() != (1, 0) {
        failures.push(format!(
            "一笔之后应为 undo=1/redo=0，实际 {:?}",
            view.history_len()
        ));
    }

    // Phase 6：一笔 = 一步 undo。点工具栏的“撤销”回到白底，点“重做”再画回来。
    let undo_button = view.history_center(HistoryAction::Undo).expect("撤销按钮");
    click_at(&mut view, undo_button);
    view.update();
    view.mark_texture_dirty();
    let undone = view.take_texture_upload().expect("撤销后应重合成");
    if undone.get_pixel(64, 64) != Color::WHITE {
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
    if redone.get_pixel(64, 64) != Color::BLACK {
        failures.push("重做没有恢复画笔笔触".to_string());
    }

    // Phase 7：导出当前文档为 PNG，再从磁盘解码回来核对像素。
    let export_path = temp_png("export");
    view.set_io_path(export_path.to_string_lossy().into_owned());
    let export_button = view.file_center(IoAction::Export).expect("导出按钮");
    click_at(&mut view, export_button);
    view.update();
    match crate::io::read_png(&export_path) {
        Ok(exported) => {
            if exported.get_pixel(0, 0) != Color::WHITE {
                failures.push("导出的 PNG 背景不是白色".to_string());
            }
            if exported.get_pixel(64, 64) != Color::BLACK {
                failures.push("导出的 PNG 缺少画笔像素".to_string());
            }
        }
        Err(error) => failures.push(format!("导出的 PNG 无法解码：{error}")),
    }

    // 导入：独立写一张 4×3 红色 PNG，点「导入 PNG」，应多一个图层，
    // 合成后左上角变红（红色图层在最上面）。
    let import_path = temp_png("import");
    let source = PixelBuffer::filled(4, 3, Color::RED);
    if let Err(error) = crate::io::write_png(&import_path, &source) {
        failures.push(format!("测试图写入失败：{error}"));
    } else {
        let before = view.layer_count();
        view.set_io_path(import_path.to_string_lossy().into_owned());
        let import_button = view.file_center(IoAction::Import).expect("导入按钮");
        click_at(&mut view, import_button);
        view.update();
        if view.layer_count() != before + 1 {
            failures.push("导入没有新增图层".to_string());
        }
        view.mark_texture_dirty();
        let composite = view.take_texture_upload().expect("导入后应重合成");
        if composite.get_pixel(0, 0) != Color::RED {
            failures.push("导入的图层没有进入合成结果".to_string());
        }
    }
    let _ = std::fs::remove_file(&export_path);
    let _ = std::fs::remove_file(&import_path);

    // Phase 8：吸管取色 -> 框选 -> 移动当前图层。
    let eyedropper = view.tool_center(ActiveTool::Eyedropper).expect("吸管按钮");
    click_at(&mut view, eyedropper);
    view.update();
    let black = view
        .canvas_camera()
        .document_to_screen(Vec2::new(64.5, 64.5));
    click_at(&mut view, black);
    view.update();
    if view.foreground() != Color::BLACK {
        failures.push("吸管没有取到画笔的黑色像素".to_string());
    }

    let select = view
        .tool_center(ActiveTool::RectangleSelect)
        .expect("框选按钮");
    click_at(&mut view, select);
    view.update();
    let select_start = view
        .canvas_camera()
        .document_to_screen(Vec2::new(54.0, 54.0));
    let select_end = view
        .canvas_camera()
        .document_to_screen(Vec2::new(74.0, 74.0));
    view.event(&InputEvent::PointerDown {
        position: select_start,
        button: PointerButton::Left,
    });
    view.event(&InputEvent::PointerMove {
        position: select_end,
    });
    view.event(&InputEvent::PointerUp {
        position: select_end,
        button: PointerButton::Left,
    });
    view.update();
    let selection = view.selection();
    if selection.is_none() {
        failures.push("框选没有产生选区".to_string());
    }

    let move_button = view.tool_center(ActiveTool::Move).expect("移动按钮");
    click_at(&mut view, move_button);
    view.update();
    let before = view.active_layer_position().unwrap_or_default();
    let move_start = view
        .canvas_camera()
        .document_to_screen(Vec2::new(64.5, 64.5));
    let move_end = move_start + Vec2::new(10.0, 10.0);
    view.event(&InputEvent::PointerDown {
        position: move_start,
        button: PointerButton::Left,
    });
    view.event(&InputEvent::PointerMove { position: move_end });
    view.event(&InputEvent::PointerUp {
        position: move_end,
        button: PointerButton::Left,
    });
    view.update();
    if view.active_layer_position().unwrap_or_default() == before {
        failures.push("移动工具没有改变图层位置".to_string());
    }

    // Phase 9：菜单栏 -> 下拉菜单 -> 菜单项动作。
    let view_index = crate::ui::menu::MENUS
        .iter()
        .position(|name| *name == "视图")
        .expect("视图 menu exists");
    match view.menu_center(view_index) {
        Some(center) => {
            click_at(&mut view, center);
            view.update();
            view.layout(viewport());
            let menu_frame = paint_frame(&view);
            if !view.menu_open() {
                failures.push("点「视图」后菜单没有打开".to_string());
            }
            if view.open_menu_index() != Some(view_index) {
                failures.push("打开的菜单下标不对".to_string());
            }
            for needle in ["放大", "缩小", "100%", "适配窗口"] {
                if !contains(&menu_frame, needle) {
                    failures.push(format!("视图菜单缺少菜单项：`{needle}`"));
                }
            }
            // 点「适配窗口」：应执行缩放并关闭菜单。
            match text_center(&menu_frame, "适配窗口") {
                Some(item_center) => {
                    click_at(&mut view, item_center);
                    view.update();
                }
                None => failures.push("找不到「适配窗口」菜单项的位置".to_string()),
            }
            if view.menu_open() {
                failures.push("点菜单项后菜单没有关闭".to_string());
            }
        }
        None => failures.push("找不到「视图」菜单标题".to_string()),
    }

    // 工具选项栏：切到画笔，点「大小 +」，笔刷应变大；工具名也在帧里。
    view.event(&InputEvent::KeyDown {
        key: Key::Character('b'),
    });
    view.update();
    view.layout(viewport());
    let options_frame = paint_frame(&view);
    if !contains(&options_frame, "画笔工具") {
        failures.push("选项栏缺少当前工具名".to_string());
    }
    let size_before = view.brush_size();
    match view.brush_adjust_center(crate::ui::BrushAdjust::SizeUp) {
        Some(center) => {
            click_at(&mut view, center);
            view.update();
        }
        None => failures.push("找不到选项栏的「大小 +」按钮".to_string()),
    }
    if view.brush_size() <= size_before {
        failures.push(format!(
            "选项栏「{}」没生效",
            crate::ui::BrushAdjust::SizeUp.label()
        ));
    }

    // 像素模式：默认开；选项栏有开关。绘制验证单独用一个新视图，避免污染
    // 主视图的历史（后面还会断言“只有最初那一笔”）。
    if !view.pixel_mode() {
        failures.push("像素模式默认应为开".to_string());
    }
    for needle in ["像素", "方形"] {
        if !contains(&options_frame, needle) {
            failures.push(format!("选项栏缺少「{needle}」开关"));
        }
    }
    check_pixel_mode_edges(&mut failures);

    // 调色盘：色块齐全，点一个预设色块应把前景色改掉。
    if view.palette_swatch_count() < 16 {
        failures.push("调色盘色块不足".to_string());
    }
    match view.palette_swatch_center(4) {
        Some(center) => {
            click_at(&mut view, center);
            view.update();
            if view.foreground() != Color::RED {
                failures.push("点调色盘没有把前景色设成红".to_string());
            }
        }
        None => failures.push("找不到调色盘色块".to_string()),
    }

    // 取色器：点方块中心应改前景色。
    match view.palette_picker_center() {
        Some(center) => {
            click_at(&mut view, center);
            view.update();
            let fg = view.foreground();
            if fg == Color::RED || fg == Color::BLACK {
                failures.push("取色器没有改前景色".to_string());
            }
        }
        None => failures.push("找不到取色器".to_string()),
    }

    // 可拖动右栏：向左拖分隔条，右栏应变宽。
    let sidebar_before = view.sidebar_width();
    match view.sidebar_handle_center() {
        Some(start) => {
            let end = start - Vec2::new(40.0, 0.0);
            view.event(&InputEvent::PointerDown {
                position: start,
                button: PointerButton::Left,
            });
            view.event(&InputEvent::PointerMove { position: end });
            view.event(&InputEvent::PointerUp {
                position: end,
                button: PointerButton::Left,
            });
            view.layout(viewport());
        }
        None => failures.push("找不到右栏分隔条".to_string()),
    }
    if view.sidebar_width() <= sidebar_before {
        failures.push("拖分隔条后右栏没变宽".to_string());
    }

    // 左侧调色盘面板也能拖动调宽。
    let palette_before = view.palette_width();
    match view.palette_handle_center() {
        Some(start) => {
            let end = start + Vec2::new(24.0, 0.0);
            view.event(&InputEvent::PointerDown {
                position: start,
                button: PointerButton::Left,
            });
            view.event(&InputEvent::PointerMove { position: end });
            view.event(&InputEvent::PointerUp {
                position: end,
                button: PointerButton::Left,
            });
            view.layout(viewport());
        }
        None => failures.push("找不到调色盘分隔条".to_string()),
    }
    if view.palette_width() <= palette_before {
        failures.push("拖调色盘分隔条没有变宽".to_string());
    }

    // 右侧栏标签页：「文件 / 历史 / 属性」共用一个面板。点标签切换内容，
    // 面板与「图层」之间的分隔条可以拖动调高。
    let tabs_tab = crate::ui::SidebarTab::History;
    if view.active_tab() != crate::ui::SidebarTab::File {
        failures.push("标签页默认应停在「文件」".to_string());
    }
    let tabs_before = view.tabs_height();
    match view.tabs_handle_center() {
        Some(start) => {
            let end = start + Vec2::new(0.0, 24.0);
            view.event(&InputEvent::PointerDown {
                position: start,
                button: PointerButton::Left,
            });
            view.event(&InputEvent::PointerMove { position: end });
            view.event(&InputEvent::PointerUp {
                position: end,
                button: PointerButton::Left,
            });
            view.layout(viewport());
        }
        None => failures.push("找不到标签页分隔条".to_string()),
    }
    if view.tabs_height() <= tabs_before {
        failures.push("拖标签页分隔条没有改变高度".to_string());
    }
    match view.tab_center(tabs_tab) {
        Some(center) => {
            click_at(&mut view, center);
            view.update();
            view.layout(viewport());
        }
        None => failures.push("找不到「历史」标签".to_string()),
    }
    if view.active_tab() != tabs_tab
        || !view.tab_content_visible(tabs_tab)
        || view.tab_content_visible(crate::ui::SidebarTab::File)
        || view.tab_content_visible(crate::ui::SidebarTab::Properties)
    {
        failures.push("点「历史」标签没有切换面板".to_string());
    }
    // 回到「文件」标签再录帧：下面的语义检查要看到文件面板的按钮文字。
    if let Some(center) = view.tab_center(crate::ui::SidebarTab::File) {
        click_at(&mut view, center);
        view.update();
        view.layout(viewport());
    }

    let (view, frame) = record(view);
    let camera = view.canvas_camera();

    let mut stats = FrameStats::new(0);
    stats.counters = FrameCounters::new(0, view.control_count(), frame.command_count(), 1);
    let report = inspect_with(&frame.draw_list, &stats, &InspectionConfig::default());

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
    // （框选结果由上面的 `view.selection()` 结构化断言，不靠状态栏那句瞬时提示。）
    let required: [(&str, &str); 13] = [
        ("文件", "菜单栏 / 文件面板"),
        ("编辑", "菜单栏"),
        ("帮助", "菜单栏"),
        ("画笔", "工具栏"),
        ("吸管", "工具栏"),
        ("128 × 128", "文档尺寸"),
        ("图层", "图层面板"),
        ("背景", "图层列表"),
        ("100%", "图层不透明度"),
        ("属性", "属性面板"),
        ("导入 PNG", "文件面板按钮"),
        ("导出 PNG", "文件面板按钮"),
        ("import", "导入后的图层名"),
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
    // 后续的导入 / 移动也各入了一步；这里只要求“重做栈已清空、还有可撤销的”。
    if redo_len != 0 {
        failures.push(format!("重做栈应为空，实际 redo={redo_len}"));
    }
    if undo_len < 1 {
        failures.push("撤销栈不应为空".to_string());
    }
    if view.history_rows() != undo_len + 1 + redo_len {
        failures.push("历史面板行数与栈长度不一致".to_string());
    }

    // 附加：Lucide 图标包已加载（覆盖工具栏的工具与撤销 / 重做），且图标真的
    // 描进了命令流。图标用圆头 / 圆角，会产生 FillCircle；普通 UI（圆角矩形 +
    // 线）不画圆，所以这是个干净信号。
    let toolbar_icons = ActiveTool::ALL.len() + HistoryAction::ALL.len();
    if view.icon_count() < toolbar_icons {
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
        "  历史 undo {undo_len} · redo {redo_len} · 图标 {} · 图层 {}\n",
        view.icon_count(),
        view.layer_count()
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
        out.push_str("  ✓ 画布合成 + 撤销/重做 + PNG 导入导出 + 移动/框选/吸管 + 图标包 + 菜单/工具栏/文件栏/图层栏/属性栏/状态栏 全部就位\n");
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
    let (_, frame) = record(EditorView::new(
        crate::theme::editor_theme(false),
        AppState::default(),
    ));
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
        let (view, frame) = record(EditorView::new(
            crate::theme::editor_theme(false),
            AppState::default(),
        ));
        let mut stats = FrameStats::new(0);
        stats.counters = FrameCounters::new(0, view.control_count(), frame.command_count(), 1);
        let report = inspect_with(&frame.draw_list, &stats, &InspectionConfig::default());
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
        let (_, frame) = record(EditorView::new(
            crate::theme::editor_theme(false),
            AppState::default(),
        ));
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
        let mut view = EditorView::new(crate::theme::editor_theme(false), AppState::default());
        view.layout(viewport());
        click_tool(&mut view, ActiveTool::Brush);
        let (_, frame) = record(view);
        assert!(contains(&frame, "画笔工具"), "状态栏应显示画笔工具");
    }

    #[test]
    fn all_text_stays_on_screen() {
        let (_, frame) = record(EditorView::new(
            crate::theme::editor_theme(false),
            AppState::default(),
        ));
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
