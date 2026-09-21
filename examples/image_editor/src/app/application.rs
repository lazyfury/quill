//! 窗口宿主：拥有 winit 窗口与 wgpu 后端，把平台事件翻译成后端无关的
//! `InputEvent`。
//!
//! 视图（[`crate::ui::HomeView`] / [`crate::ui::EditorView`]）完全不碰平台；
//! 这一层照 `examples/file_browser` / `examples/wgpu_demo` 的骨架来：
//!
//! ```text
//! winit events -> InputEvent -> view -> DrawList -> WgpuBackend -> surface
//! ```
//!
//! **多窗口**：主窗口首屏是主页（[`crate::ui::HomeView`]），点「新建窗口」不由
//! 这里弹模态框，而是 `event_loop.create_window` 开一个**原生**的「新建文档」
//! 窗口（[`crate::ui::NewDocumentView`]）选尺寸 / 背景色；创建后编辑器仍然在
//! **主窗口**里（主窗口从主页切到 [`EditorView`]）。每个窗口有自己的 surface /
//! `WgpuBackend`（纹理命名空间独立）。
//!
//! Phase 1 没有长任务，所以没有工作线程；`--frames N` 让真实渲染管线能在
//! 无头环境里跑够帧数再退出。

use std::rc::Rc;
use std::sync::Arc;

use draw_backend_wgpu::{wgpu, FontConfig, FontMetrics, FontMode, TextureFilter, WgpuBackend};
use draw_core::{InputEvent, Key, PointerButton, Size, Vec2, ViewportSize};
use draw_render::{PaintContext, RenderBackend};
use draw_theme::{SurfaceLevel, Theme};
use draw_ui::TextMeasurer;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key as WinitKey, ModifiersState, NamedKey};
use winit::window::{Window, WindowId};

use crate::app::state::AppState;
use crate::ui::{EditorView, HomeView, NewDocumentResult, NewDocumentSpec, NewDocumentView};

/// 主窗口（主页 / 编辑器）的初始大小。
const WINDOW_WIDTH: f64 = 1280.0;
const WINDOW_HEIGHT: f64 = 800.0;
/// 「新建文档」窗口的初始大小。
const NEW_DOCUMENT_WIDTH: f64 = 360.0;
const NEW_DOCUMENT_HEIGHT: f64 = 400.0;

/// 命令行选项（由 [`crate::main`] 解析）。
#[derive(Clone, Debug, Default)]
pub struct Options {
    /// 渲染这么多帧后退出（`None` = 一直跑）。
    pub frames: Option<u32>,
    /// 使用浅色主题（默认深色）。
    pub light: bool,
    /// 使用内置点阵字体（不含中文；默认系统字体）。
    pub pixel_font: bool,
}

/// 打开窗口跑编辑器，直到关闭（或 `--frames` 跑完）。
pub fn run(options: Options) {
    let event_loop = EventLoop::new().expect("create event loop");
    // 事件驱动：只有输入、尺寸变化才重画。`Poll` 会一直重画没变的帧，白烧 CPU。
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut app = App::new(options);
    event_loop.run_app(&mut app).expect("run event loop");
}

/// 一个窗口承载的视图。
enum View {
    /// 主窗口的落地页。
    Home(HomeView),
    /// 主窗口的编辑器。
    Editor(EditorView),
    /// 「新建文档」辅助窗口。
    NewDocument(NewDocumentView),
}

/// 一个原生窗口 + 它自己的 surface / 后端 / 状态。
struct WindowState {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    backend: WgpuBackend,
    config: wgpu::SurfaceConfiguration,
    scale_factor: f64,
    cursor: Vec2,
    view: View,
}

struct App {
    instance: wgpu::Instance,
    windows: Vec<WindowState>,
    /// 主窗口（主页 -> 编辑器）；关闭它即退出应用。
    main_window: Option<WindowId>,
    /// 当前打开的「新建文档」窗口（同时只允许一个）。
    new_document: Option<WindowId>,
    /// 主题配色（`--light`）。
    light: bool,
    font_mode: FontMode,
    /// 当前修饰键状态（Ctrl/Cmd+Z 这类快捷键在平台层处理）。
    modifiers: ModifiersState,
    /// `--frames` 剩下的帧数。
    frames_left: Option<u32>,
}

impl App {
    fn new(options: Options) -> Self {
        tracing::info!(target: "image_editor", light = options.light, "application_start");
        Self {
            instance: wgpu::Instance::default(),
            windows: Vec::new(),
            main_window: None,
            new_document: None,
            light: options.light,
            font_mode: if options.pixel_font {
                FontMode::Pixel
            } else {
                FontMode::System
            },
            modifiers: ModifiersState::empty(),
            frames_left: options.frames,
        }
    }

    /// 编辑器自己的主题：设计系统的调色板 + 紧凑密度（更小 padding、mini 控件）。
    fn theme(&self) -> Theme {
        crate::theme::editor_theme(self.light)
    }

    /// 首次 resume 时创建主窗口（首屏是主页）。
    fn init(&mut self, event_loop: &ActiveEventLoop) {
        if !self.windows.is_empty() {
            return;
        }
        self.open_main_window(event_loop);
    }

    fn window_index(&self, id: WindowId) -> Option<usize> {
        self.windows
            .iter()
            .position(|state| state.window.id() == id)
    }

    /// 建 surface + 后端 + 交换链配置（每个窗口一套，纹理互相隔离）。
    fn create_surface(
        &self,
        window: &Arc<Window>,
    ) -> (
        wgpu::Surface<'static>,
        WgpuBackend,
        wgpu::SurfaceConfiguration,
    ) {
        let surface = self
            .instance
            .create_surface(window.clone())
            .expect("create surface");

        let backend = WgpuBackend::from_instance(
            &self.instance,
            Some(&surface),
            wgpu::PowerPreference::HighPerformance,
        )
        .expect("create wgpu backend");

        // 非 sRGB 的格式能让 shader 写出的 unorm 颜色跟 Canvas 后端一致。
        let capabilities = surface.get_capabilities(backend.adapter());
        let format = capabilities
            .formats
            .iter()
            .copied()
            .find(|format| !format.is_srgb())
            .unwrap_or(capabilities.formats[0]);

        let size = window.inner_size();
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: capabilities.alpha_modes[0],
            view_formats: Vec::new(),
        };
        surface.configure(backend.device(), &config);
        (surface, backend, config)
    }

    /// 给一个后端装字体，并返回用真实度量排版的 `TextMeasurer`。
    fn text_measurer(&self, backend: &mut WgpuBackend) -> Rc<dyn TextMeasurer> {
        let font_config = FontConfig {
            mode: self.font_mode,
            device_pixel_rasterization: true,
        };
        if let Err(error) = backend.set_font_config(font_config) {
            eprintln!("font setup failed, using fallback: {error}");
        }
        Rc::new(BackendTextMeasurer {
            metrics: backend.text_metrics(),
        })
    }

    /// 开主窗口（首屏是主页）。
    fn open_main_window(&mut self, event_loop: &ActiveEventLoop) {
        let theme = self.theme();
        let attributes = Window::default_attributes()
            .with_title("quill 图像编辑器")
            .with_inner_size(LogicalSize::new(WINDOW_WIDTH, WINDOW_HEIGHT));
        let window = Arc::new(
            event_loop
                .create_window(attributes)
                .expect("create main window"),
        );

        let scale_factor = window.scale_factor();
        let (surface, mut backend, config) = self.create_surface(&window);
        backend.set_scale_factor(scale_factor as f32);
        backend.set_clear_color(theme_background(theme));

        let mut home = HomeView::new(theme);
        let measurer = self.text_measurer(&mut backend);
        home.set_text_measurer(measurer);

        self.main_window = Some(window.id());
        tracing::info!(target: "image_editor", "home_window_opened");
        let window = self.push_window(
            window,
            surface,
            backend,
            config,
            scale_factor,
            View::Home(home),
        );
        window.request_redraw();
    }

    /// 开一个**原生**的「新建文档」窗口（首页「新建窗口」真正走的路）。
    ///
    /// 只负责选尺寸 / 背景色，不是模态框；创建后编辑器仍然在**主窗口**里
    /// （见 [`apply_new_document`](Self::apply_new_document)）。
    fn open_new_document_window(&mut self, event_loop: &ActiveEventLoop) {
        // 已经开着就把它提到前面，不再开第二个。
        if let Some(id) = self.new_document {
            if let Some(index) = self.window_index(id) {
                self.windows[index].window.focus_window();
                return;
            }
        }

        let theme = self.theme();
        let attributes = Window::default_attributes()
            .with_title("新建文档 — quill 图像编辑器")
            .with_inner_size(LogicalSize::new(NEW_DOCUMENT_WIDTH, NEW_DOCUMENT_HEIGHT));
        let window = Arc::new(
            event_loop
                .create_window(attributes)
                .expect("create new-document window"),
        );

        let scale_factor = window.scale_factor();
        let (surface, mut backend, config) = self.create_surface(&window);
        backend.set_scale_factor(scale_factor as f32);
        backend.set_clear_color(theme_background(theme));

        let mut view = NewDocumentView::new(theme);
        let measurer = self.text_measurer(&mut backend);
        view.set_text_measurer(measurer);

        self.new_document = Some(window.id());
        tracing::info!(target: "image_editor", "new_document_window_opened");
        let window = self.push_window(
            window,
            surface,
            backend,
            config,
            scale_factor,
            View::NewDocument(view),
        );
        window.request_redraw();
    }

    /// 把「新建文档」窗口的选择变成主窗口里的一个新编辑器。
    fn apply_new_document(&mut self, spec: NewDocumentSpec) {
        let Some(index) = self.main_window.and_then(|id| self.window_index(id)) else {
            return;
        };
        let theme = self.theme();
        let state = AppState::with_document(spec.width, spec.height, spec.background);
        let mut editor = EditorView::new(theme, state);
        tracing::info!(
            target: "image_editor",
            name = editor.document_name().as_str(),
            width = spec.width,
            height = spec.height,
            "document_created"
        );

        let metrics = self.windows[index].backend.text_metrics();
        editor.set_text_measurer(Rc::new(BackendTextMeasurer { metrics }));
        self.windows[index]
            .backend
            .set_clear_color(theme_background(editor.theme()));

        // 像素图：文档纹理放大时用最近邻采样，放大后是硬边像素而不是模糊插值。
        self.windows[index]
            .backend
            .set_texture_filter(crate::canvas::DOCUMENT_TEXTURE, TextureFilter::Nearest);
        // 文档合成结果 -> 后端纹理。视图画的是 `Visual::Image(DOCUMENT_TEXTURE)`，
        // 所以必须在首帧之前把同一 id 注册好，否则后端会退回白色兜底纹理。
        if let Some(pixels) = editor.take_texture_upload() {
            if let Err(error) = self.windows[index].backend.update_texture(
                crate::canvas::DOCUMENT_TEXTURE,
                pixels.width,
                pixels.height,
                &pixels.data,
            ) {
                eprintln!("document texture upload failed: {error}");
            }
        }

        self.windows[index]
            .window
            .set_title(&format!("{} — quill 图像编辑器", editor.document_name()));
        self.windows[index].view = View::Editor(editor);
        self.windows[index].window.request_redraw();
    }

    /// 关掉一个窗口，并维护主 / 辅助窗口的登记。
    fn close_window(&mut self, event_loop: &ActiveEventLoop, id: WindowId) {
        let Some(index) = self.window_index(id) else {
            return;
        };
        self.windows.remove(index);
        if self.main_window == Some(id) {
            // 主窗口关闭 = 退出应用（辅助窗口随之结束）。
            self.main_window = None;
            event_loop.exit();
            return;
        }
        if self.new_document == Some(id) {
            self.new_document = None;
        }
    }

    /// 收下一个已经建好的窗口状态，返回它的窗口句柄。
    fn push_window(
        &mut self,
        window: Arc<Window>,
        surface: wgpu::Surface<'static>,
        backend: WgpuBackend,
        config: wgpu::SurfaceConfiguration,
        scale_factor: f64,
        view: View,
    ) -> Arc<Window> {
        self.windows.push(WindowState {
            window: window.clone(),
            surface,
            backend,
            config,
            scale_factor,
            cursor: Vec2::ZERO,
            view,
        });
        window
    }

    fn resize(&mut self, index: usize, width: u32, height: u32) {
        let Some(state) = self.windows.get_mut(index) else {
            return;
        };
        if width == 0 || height == 0 {
            return;
        }
        state.config.width = width;
        state.config.height = height;
        state
            .surface
            .configure(state.backend.device(), &state.config);
    }

    /// 把一个后端无关的输入事件送给这个窗口的视图。
    fn feed(&mut self, index: usize, event: &InputEvent) {
        let Some(state) = self.windows.get_mut(index) else {
            return;
        };
        match &mut state.view {
            View::Home(home) => {
                let _ = home.event(event);
            }
            View::Editor(editor) => {
                let _ = editor.event(event);
            }
            View::NewDocument(view) => {
                let _ = view.event(event);
            }
        }
    }

    fn render(&mut self, event_loop: &ActiveEventLoop, index: usize) {
        let Some(state) = self.windows.get_mut(index) else {
            return;
        };

        let logical = Size::new(
            state.config.width as f32 / state.scale_factor as f32,
            state.config.height as f32 / state.scale_factor as f32,
        );
        let viewport = ViewportSize::new(logical);

        let list = {
            let mut ctx = PaintContext::new();
            match &mut state.view {
                View::Home(home) => {
                    home.layout(viewport);
                    home.paint(&mut ctx);
                }
                View::NewDocument(view) => {
                    view.update();
                    view.layout(viewport);
                    view.paint(&mut ctx);
                }
                View::Editor(editor) => {
                    // 1. 把共享状态（工具、提示）同步到控件上。
                    editor.update();
                    // 2. 文档像素有变化就重新合成并上传纹理。
                    if let Some(pixels) = editor.take_texture_upload() {
                        if let Err(error) = state.backend.update_texture(
                            crate::canvas::DOCUMENT_TEXTURE,
                            pixels.width,
                            pixels.height,
                            &pixels.data,
                        ) {
                            eprintln!("document texture upload failed: {error}");
                        }
                    }
                    // 3. 排布 -> 绘制成后端无关的 DrawList。
                    editor.layout(viewport);
                    editor.paint(&mut ctx);
                }
            }
            ctx.into_draw_list()
        };
        // 主页点过「新建窗口」没有？取走请求（只在主页窗口才可能为真）。
        let open_new_document =
            matches!(&state.view, View::Home(home) if home.take_new_window_request());
        // 「新建文档」窗口按了创建 / 取消没有？
        let new_document_result = match &state.view {
            View::NewDocument(view) => view.take_result(),
            _ => None,
        };

        // 4. 交给后端。
        let surface_texture = match state.surface.get_current_texture() {
            Ok(texture) => texture,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                state
                    .surface
                    .configure(state.backend.device(), &state.config);
                return;
            }
            Err(wgpu::SurfaceError::Timeout) => return,
            Err(error) => {
                eprintln!("surface error: {error}");
                return;
            }
        };
        let view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        if state
            .backend
            .begin_frame_with_view(
                view,
                state.config.width,
                state.config.height,
                state.config.format,
                viewport,
            )
            .is_ok()
        {
            let _ = state.backend.submit(&list);
            let _ = state.backend.end_frame();
        }
        surface_texture.present();

        let window_id = self.windows[index].window.id();
        // 主窗口的「新建窗口」：开原生「新建文档」窗口。
        if open_new_document {
            tracing::info!(target: "image_editor", "new_window_requested");
            self.open_new_document_window(event_loop);
        }
        // 「新建文档」窗口出结果：创建 -> 主窗口换成编辑器；取消 -> 只关窗。
        if let Some(result) = new_document_result {
            if let NewDocumentResult::Create(spec) = result {
                self.apply_new_document(spec);
            }
            self.close_window(event_loop, window_id);
        }

        // `event_loop.exit()` 不会立刻停 —— 退出前可能还排着一次重画，所以
        // 先把计数清掉，免得重复触发（也免得重复打印）。
        // 计数器还在时自己排下一帧：否则事件循环会 `Wait` 在一个不会再来的
        // RedrawRequested 上（Phase 1 没有工作线程来唤醒它）。
        if let Some(left) = self.frames_left.take() {
            let left = left.saturating_sub(1);
            if left == 0 {
                eprintln!("--frames 跑完了，退出");
                event_loop.exit();
            } else {
                self.frames_left = Some(left);
                if let Some(state) = self.windows.get(index) {
                    state.window.request_redraw();
                }
            }
        }
    }

    fn to_logical(&self, index: usize, position: winit::dpi::PhysicalPosition<f64>) -> Vec2 {
        let scale = self
            .windows
            .get(index)
            .map(|state| state.scale_factor)
            .unwrap_or(1.0) as f32;
        Vec2::new(position.x as f32 / scale, position.y as f32 / scale)
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.init(event_loop);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(index) = self
            .windows
            .iter()
            .position(|state| state.window.id() == window_id)
        else {
            return;
        };

        // A redraw is already the render itself; do not request another.
        if matches!(event, WindowEvent::RedrawRequested) {
            self.render(event_loop, index);
            return;
        }

        match event {
            WindowEvent::CloseRequested => {
                self.close_window(event_loop, window_id);
                return;
            }
            WindowEvent::Resized(size) => self.resize(index, size.width, size.height),
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                if let Some(state) = self.windows.get_mut(index) {
                    state.scale_factor = scale_factor;
                    state.backend.set_scale_factor(scale_factor as f32);
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                let position = self.to_logical(index, position);
                if let Some(state) = self.windows.get_mut(index) {
                    state.cursor = position;
                }
                self.feed(index, &InputEvent::PointerMove { position });
            }
            WindowEvent::CursorLeft { .. } => self.feed(index, &InputEvent::PointerLeave),
            WindowEvent::MouseInput { state, button, .. } => {
                let position = self.windows[index].cursor;
                let button = pointer_button(button);
                let event = match state {
                    ElementState::Pressed => InputEvent::PointerDown { position, button },
                    ElementState::Released => InputEvent::PointerUp { position, button },
                };
                self.feed(index, &event);
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let scale = self.windows[index].scale_factor as f32;
                let delta = wheel_pixels(delta, scale);
                let position = self.windows[index].cursor;
                self.feed(
                    index,
                    &InputEvent::Wheel {
                        position,
                        delta: Vec2::new(0.0, delta),
                    },
                );
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let pressed = event.state == ElementState::Pressed;
                // Ctrl/Cmd+Z 这类快捷键先在这里截掉：`InputEvent::KeyDown` 没有
                // 修饰键字段（`draw_core` 刻意保持最小），所以由宿主映射到
                // `EditorView::undo/redo`，不当作普通按键 / 文本。主页窗口没有
                // 编辑器，直接跳过。
                let mut consumed = false;
                if pressed {
                    if let Some(View::Editor(editor)) =
                        self.windows.get_mut(index).map(|state| &mut state.view)
                    {
                        consumed = history_shortcut(editor, self.modifiers, &event.logical_key);
                    }
                }
                if !consumed {
                    // 已提交文本（IME / 打字）先送：重命名编辑用它写入缓冲区。
                    if pressed {
                        if let Some(text) = event.text.as_ref() {
                            let text: String = text.chars().filter(|ch| !ch.is_control()).collect();
                            if !text.is_empty() {
                                self.feed(index, &InputEvent::TextInput { text });
                            }
                        }
                    }
                    if let Some(key) = map_key(&event.logical_key) {
                        let input = match event.state {
                            ElementState::Pressed => InputEvent::KeyDown { key },
                            ElementState::Released => InputEvent::KeyUp { key },
                        };
                        self.feed(index, &input);
                    }
                }
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                self.modifiers = modifiers.state();
            }
            _ => {}
        }

        // Any handled event may have changed the UI; schedule exactly one frame.
        if let Some(state) = self.windows.get(index) {
            state.window.request_redraw();
        }
    }
}

/// Ctrl/Cmd+Z 撤销，Ctrl+Y 或 Shift+Ctrl/Cmd+Z 重做；命中返回 `true`。
fn history_shortcut(editor: &mut EditorView, modifiers: ModifiersState, key: &WinitKey) -> bool {
    if !(modifiers.control_key() || modifiers.super_key()) {
        return false;
    }
    let WinitKey::Character(text) = key else {
        return false;
    };
    match text.to_lowercase().as_str() {
        "z" if modifiers.shift_key() => {
            editor.redo();
            true
        }
        "z" => {
            editor.undo();
            true
        }
        "y" => {
            editor.redo();
            true
        }
        _ => false,
    }
}

/// 一个滚轮刻度折算的逻辑像素（画布只用符号决定缩放方向；数值留给以后的
/// 内嵌滚动列表）。
const WHEEL_LINE_HEIGHT: f32 = 48.0;

/// 平台滚轮 -> 逻辑像素（`y > 0` 表示向下滚，跟 `InputEvent::Wheel` 一致）。
fn wheel_pixels(delta: MouseScrollDelta, scale: f32) -> f32 {
    match delta {
        MouseScrollDelta::LineDelta(_, lines) => -lines * WHEEL_LINE_HEIGHT,
        MouseScrollDelta::PixelDelta(position) => {
            -(position.y as f32) / if scale > 0.0 { scale } else { 1.0 }
        }
    }
}

/// 窗口清屏色 = 主题的底层背景（`draw_ui` 之外的地方由后端填）。
fn theme_background(theme: Theme) -> draw_core::Color {
    theme.surface(SurfaceLevel::Base)
}

/// 后端字体度量 -> 布局引擎的 `TextMeasurer`（`wgpu_demo` 里同一个适配器）。
struct BackendTextMeasurer {
    metrics: FontMetrics,
}

impl TextMeasurer for BackendTextMeasurer {
    fn advance(&self, ch: char, font_size: f32) -> f32 {
        self.metrics.advance(ch, font_size)
    }

    fn line_height(&self, font_size: f32) -> f32 {
        self.metrics.line_height(font_size)
    }

    fn ascent(&self, font_size: f32) -> f32 {
        self.metrics.ascent(font_size)
    }

    fn measure_run(&self, text: &str, font_size: f32) -> f32 {
        self.metrics.measure_run(text, font_size)
    }
}

fn pointer_button(button: MouseButton) -> PointerButton {
    match button {
        MouseButton::Right => PointerButton::Right,
        MouseButton::Middle => PointerButton::Middle,
        _ => PointerButton::Left,
    }
}

fn map_key(key: &WinitKey) -> Option<Key> {
    match key {
        WinitKey::Named(NamedKey::Enter) => Some(Key::Enter),
        WinitKey::Named(NamedKey::Escape) => Some(Key::Escape),
        WinitKey::Named(NamedKey::Backspace) => Some(Key::Backspace),
        WinitKey::Named(NamedKey::Delete) => Some(Key::Delete),
        WinitKey::Named(NamedKey::Tab) => Some(Key::Tab),
        WinitKey::Named(NamedKey::Space) => Some(Key::Space),
        WinitKey::Character(text) => text.chars().next().map(Key::Character),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_shortcuts_are_lowercase_and_uppercase() {
        assert_eq!(
            map_key(&WinitKey::Character("b".into())),
            Some(Key::Character('b'))
        );
        assert_eq!(map_key(&WinitKey::Named(NamedKey::Space)), Some(Key::Space));
        assert_eq!(map_key(&WinitKey::Named(NamedKey::Shift)), None);
    }

    #[test]
    fn pointer_buttons_map() {
        assert_eq!(pointer_button(MouseButton::Right), PointerButton::Right);
        assert_eq!(pointer_button(MouseButton::Middle), PointerButton::Middle);
        assert_eq!(pointer_button(MouseButton::Left), PointerButton::Left);
    }

    #[test]
    fn a_wheel_up_zooms_in_after_the_sign_convention() {
        // 滚轮向上（远离自己）-> delta.y < 0 -> 视图放大。
        assert!(wheel_pixels(MouseScrollDelta::LineDelta(0.0, 1.0), 1.0) < 0.0);
        assert!(wheel_pixels(MouseScrollDelta::LineDelta(0.0, -1.0), 1.0) > 0.0);
    }

    #[test]
    fn pixel_wheel_deltas_are_converted_to_logical() {
        let pixels = wheel_pixels(
            MouseScrollDelta::PixelDelta(winit::dpi::PhysicalPosition::new(0.0, -40.0)),
            2.0,
        );
        assert_eq!(pixels, 20.0);
    }
}
