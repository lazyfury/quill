//! 窗口宿主：拥有 winit 窗口与 wgpu 后端，把平台事件翻译成后端无关的
//! `InputEvent`。
//!
//! 视图（[`crate::ui::EditorView`]）完全不碰平台；这一层照
//! `examples/file_browser` / `examples/wgpu_demo` 的骨架来：
//!
//! ```text
//! winit events -> InputEvent -> EditorView -> DrawList -> WgpuBackend -> surface
//! ```
//!
//! Phase 1 没有长任务，所以没有工作线程；`--frames N` 让真实渲染管线能在
//! 无头环境里跑够帧数再退出。

use std::rc::Rc;
use std::sync::Arc;

use draw_backend_wgpu::{wgpu, FontConfig, FontMetrics, FontMode, WgpuBackend};
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
use crate::ui::EditorView;

/// 初始窗口大小。
const WINDOW_WIDTH: f64 = 1280.0;
const WINDOW_HEIGHT: f64 = 800.0;

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

struct App {
    instance: wgpu::Instance,
    window: Option<Arc<Window>>,
    surface: Option<wgpu::Surface<'static>>,
    backend: Option<WgpuBackend>,
    config: Option<wgpu::SurfaceConfiguration>,
    scale_factor: f64,
    cursor: Vec2,
    editor: EditorView,
    font_mode: FontMode,
    /// 当前修饰键状态（Ctrl/Cmd+Z 这类快捷键在平台层处理）。
    modifiers: ModifiersState,
    /// `--frames` 剩下的帧数。
    frames_left: Option<u32>,
}

impl App {
    fn new(options: Options) -> Self {
        let theme = if options.light {
            Theme::light()
        } else {
            Theme::dark()
        };
        tracing::info!(target: "image_editor", theme = ?theme.mode, "application_start");
        let state = AppState::default();
        tracing::info!(
            target: "image_editor",
            name = state.document.name.as_str(),
            width = state.document.width,
            height = state.document.height,
            "document_created"
        );
        Self {
            instance: wgpu::Instance::default(),
            window: None,
            surface: None,
            backend: None,
            config: None,
            scale_factor: 1.0,
            cursor: Vec2::ZERO,
            editor: EditorView::new(theme, state),
            font_mode: if options.pixel_font {
                FontMode::Pixel
            } else {
                FontMode::System
            },
            modifiers: ModifiersState::empty(),
            frames_left: options.frames,
        }
    }

    /// 首次 resume 时创建窗口、surface、backend 和交换链。
    fn init(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let title = format!(
            "{} — quill 图像编辑器（Phase 6）",
            self.editor.document_name()
        );
        let attributes = Window::default_attributes()
            .with_title(title)
            .with_inner_size(LogicalSize::new(WINDOW_WIDTH, WINDOW_HEIGHT));
        let window = Arc::new(event_loop.create_window(attributes).expect("create window"));

        let surface = self
            .instance
            .create_surface(window.clone())
            .expect("create surface");

        let mut backend = WgpuBackend::from_instance(
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

        self.scale_factor = window.scale_factor();
        backend.set_scale_factor(self.scale_factor as f32);
        backend.set_clear_color(theme_background(self.editor.theme()));

        let font_config = FontConfig {
            mode: self.font_mode,
            device_pixel_rasterization: true,
        };
        if let Err(error) = backend.set_font_config(font_config) {
            eprintln!("font setup failed, using fallback: {error}");
        }
        // 用后端的真实字体度量排版，否则量到的宽度跟画出来的宽度不一致。
        self.editor.set_text_measurer(Rc::new(BackendTextMeasurer {
            metrics: backend.text_metrics(),
        }));

        // 文档合成结果 -> 后端纹理。视图画的是 `Visual::Image(DOCUMENT_TEXTURE)`，
        // 所以必须在首帧之前把同一 id 注册好，否则后端会退回白色兜底纹理。
        if let Some(pixels) = self.editor.take_texture_upload() {
            if let Err(error) = backend.update_texture(
                crate::canvas::DOCUMENT_TEXTURE,
                pixels.width,
                pixels.height,
                &pixels.data,
            ) {
                eprintln!("document texture upload failed: {error}");
            }
        }

        self.window = Some(window);
        self.surface = Some(surface);
        self.backend = Some(backend);
        self.config = Some(config);
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    fn resize(&mut self, width: u32, height: u32) {
        let (Some(surface), Some(backend), Some(config)) = (
            self.surface.as_ref(),
            self.backend.as_ref(),
            self.config.as_mut(),
        ) else {
            return;
        };
        if width == 0 || height == 0 {
            return;
        }
        config.width = width;
        config.height = height;
        surface.configure(backend.device(), config);
    }

    fn feed(&mut self, event: &InputEvent) {
        let _ = self.editor.event(event);
    }

    fn render(&mut self, event_loop: &ActiveEventLoop) {
        // 1. 把共享状态（工具、提示）同步到控件上。
        self.editor.update();
        // 2. 文档像素有变化就重新合成并上传纹理。
        if let Some(pixels) = self.editor.take_texture_upload() {
            if let Some(backend) = self.backend.as_mut() {
                if let Err(error) = backend.update_texture(
                    crate::canvas::DOCUMENT_TEXTURE,
                    pixels.width,
                    pixels.height,
                    &pixels.data,
                ) {
                    eprintln!("document texture upload failed: {error}");
                }
            }
        }

        let (Some(surface), Some(backend), Some(config)) = (
            self.surface.as_ref(),
            self.backend.as_mut(),
            self.config.as_ref(),
        ) else {
            return;
        };

        let logical = Size::new(
            config.width as f32 / self.scale_factor as f32,
            config.height as f32 / self.scale_factor as f32,
        );
        let viewport = ViewportSize::new(logical);

        // 2. 排布 -> 3. 绘制成后端无关的 DrawList。
        self.editor.layout(viewport);
        let mut ctx = PaintContext::new();
        self.editor.paint(&mut ctx);
        let list = ctx.into_draw_list();

        // 4. 交给后端。
        let surface_texture = match surface.get_current_texture() {
            Ok(texture) => texture,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                surface.configure(backend.device(), config);
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

        if backend
            .begin_frame_with_view(view, config.width, config.height, config.format, viewport)
            .is_ok()
        {
            let _ = backend.submit(&list);
            let _ = backend.end_frame();
        }
        surface_texture.present();

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
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.init(event_loop);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        // A redraw is already the render itself; do not request another.
        if matches!(event, WindowEvent::RedrawRequested) {
            self.render(event_loop);
            return;
        }

        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
                return;
            }
            WindowEvent::Resized(size) => self.resize(size.width, size.height),
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.scale_factor = scale_factor;
                if let Some(backend) = self.backend.as_mut() {
                    backend.set_scale_factor(scale_factor as f32);
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = self.to_logical(position);
                self.feed(&InputEvent::PointerMove {
                    position: self.cursor,
                });
            }
            WindowEvent::CursorLeft { .. } => self.feed(&InputEvent::PointerLeave),
            WindowEvent::MouseInput { state, button, .. } => {
                let position = self.cursor;
                let button = pointer_button(button);
                let event = match state {
                    ElementState::Pressed => InputEvent::PointerDown { position, button },
                    ElementState::Released => InputEvent::PointerUp { position, button },
                };
                self.feed(&event);
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let delta = wheel_pixels(delta, self.scale_factor as f32);
                let position = self.cursor;
                self.feed(&InputEvent::Wheel {
                    position,
                    delta: Vec2::new(0.0, delta),
                });
            }
            WindowEvent::KeyboardInput { event, .. } => {
                // Ctrl/Cmd+Z 这类快捷键先在这里截掉：`InputEvent::KeyDown` 没有
                // 修饰键字段（`draw_core` 刻意保持最小），所以由宿主映射到
                // `EditorView::undo/redo`，不当作普通按键 / 文本。
                if !(event.state == ElementState::Pressed
                    && self.history_shortcut(&event.logical_key))
                {
                    // 已提交文本（IME / 打字）先送：重命名编辑用它写入缓冲区。
                    if event.state == ElementState::Pressed {
                        if let Some(text) = event.text.as_ref() {
                            let text: String = text.chars().filter(|ch| !ch.is_control()).collect();
                            if !text.is_empty() {
                                self.feed(&InputEvent::TextInput { text });
                            }
                        }
                    }
                    if let Some(key) = map_key(&event.logical_key) {
                        let input = match event.state {
                            ElementState::Pressed => InputEvent::KeyDown { key },
                            ElementState::Released => InputEvent::KeyUp { key },
                        };
                        self.feed(&input);
                    }
                }
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                self.modifiers = modifiers.state();
            }
            _ => {}
        }

        // Any handled event may have changed the UI; schedule exactly one frame.
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }
}

impl App {
    /// Ctrl/Cmd+Z 撤销，Ctrl+Y 或 Shift+Ctrl/Cmd+Z 重做；命中返回 `true`。
    fn history_shortcut(&mut self, key: &WinitKey) -> bool {
        if !(self.modifiers.control_key() || self.modifiers.super_key()) {
            return false;
        }
        let WinitKey::Character(text) = key else {
            return false;
        };
        match text.to_lowercase().as_str() {
            "z" if self.modifiers.shift_key() => {
                self.editor.redo();
                true
            }
            "z" => {
                self.editor.undo();
                true
            }
            "y" => {
                self.editor.redo();
                true
            }
            _ => false,
        }
    }

    fn to_logical(&self, position: winit::dpi::PhysicalPosition<f64>) -> Vec2 {
        Vec2::new(
            position.x as f32 / self.scale_factor as f32,
            position.y as f32 / self.scale_factor as f32,
        )
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
