//! 窗口宿主：拥有窗口、wgpu 后端和读盘的工作线程。
//!
//! 视图（[`crate::ui::Browser`]）不碰平台，这一层把两边接起来。三件事是
//! 它独有的：
//!
//! 1. **读目录在工作线程上。** 视图只说"我想看这个目录"
//!    （[`Browser::take_navigation`]）；这里起一个线程跑 [`scan::scan`]，结果
//!    通过 `EventLoopProxy` 送回主线程。扫一个有十万条目的目录时界面照常滚。
//! 2. **把平台滚轮翻译成 `InputEvent::Wheel`。** `draw_ui::handle_input`
//!    会把滚轮沿祖先链交给列表的滚动回调，但**得有人先把事件造出来** ——
//!    winit 的 `MouseWheel` 不会自己变成 `InputEvent`。这一段就是那个泵。
//! 3. **`--frames` 跑够帧数就退出**，让真实渲染管线能在无头环境里被验证。

use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use draw_backend_wgpu::{wgpu, FontConfig, FontMetrics, FontMode, WgpuBackend};
use draw_core::{InputEvent, Key, PointerButton, Size, Vec2, ViewportSize};
use draw_render::{PaintContext, RenderBackend};
use draw_theme::{SurfaceLevel, Theme};
use draw_ui::TextMeasurer;
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition};
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::keyboard::{Key as WinitKey, NamedKey};
use winit::window::{Window, WindowId};

use crate::preview::{self, Preview};
use crate::scan::{self, Listing};
use crate::ui::{Browser, ROW_HEIGHT};

/// 一个滚轮刻度滚多少逻辑像素（三行 —— macOS 一格的量级）。
const WHEEL_LINE_HEIGHT: f32 = 3.0 * ROW_HEIGHT;

/// 窗口初始大小：列表的行池就是按这个高度算出来的。
const WINDOW_WIDTH: f64 = 1100.0;
const WINDOW_HEIGHT: f64 = 680.0;

/// 命令行选项（由 [`crate::main`] 解析）。
#[derive(Clone, Debug)]
pub struct Options {
    /// 起始目录。
    pub path: std::path::PathBuf,
    /// 显示点开头的文件。
    pub hidden: bool,
    /// 渲染这么多帧后退出（`None` = 一直跑）。
    pub frames: Option<u32>,
    pub light: bool,
    pub pixel_font: bool,
}

/// 工作线程送回主线程的东西。
enum UserEvent {
    /// 一份清单，带上它是第几次请求 —— 迟到的旧结果（用户已经翻走了）会被丢掉。
    Listing(u64, Listing),
    /// 一份二进制预览，同样带请求号：按住方向键扫过一堆文件时，只有最后一次
    /// 的结果该落到右栏。
    Preview(u64, Preview),
}

/// Runs the browser until the window closes (or `--frames` runs out).
pub fn run(options: Options) {
    let event_loop = EventLoop::<UserEvent>::with_user_event()
        .build()
        .expect("create event loop");
    // 事件驱动：只有输入、尺寸变化、清单回来才重画。`Poll` 会一直重画一个
    // 没变的帧，白烧 CPU。
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut app = App::new(options, event_loop.create_proxy());
    event_loop.run_app(&mut app).expect("run event loop");
}

/// Owns the window, surface, backend and view.
struct App {
    instance: wgpu::Instance,
    window: Option<Arc<Window>>,
    surface: Option<wgpu::Surface<'static>>,
    backend: Option<WgpuBackend>,
    config: Option<wgpu::SurfaceConfiguration>,
    scale_factor: f64,
    cursor: Vec2,
    browser: Browser,
    font_mode: FontMode,
    /// Wakes the event loop when a worker thread has a listing.
    proxy: EventLoopProxy<UserEvent>,
    /// 第几次读目录请求；回来的结果带着这个号，对不上就说明已经过期。
    generation: u64,
    /// 第几次读文件请求。跟目录分开计：两者可以同时在进行，互不作废。
    preview_generation: u64,
    /// `--frames` 剩下的帧数。
    frames_left: Option<u32>,
    last_frame: Instant,
}

impl App {
    fn new(options: Options, proxy: EventLoopProxy<UserEvent>) -> Self {
        let theme = if options.light {
            Theme::light()
        } else {
            Theme::dark()
        };
        Self {
            instance: wgpu::Instance::default(),
            window: None,
            surface: None,
            backend: None,
            config: None,
            scale_factor: 1.0,
            cursor: Vec2::ZERO,
            browser: Browser::new(theme, options.path, options.hidden),
            font_mode: if options.pixel_font {
                FontMode::Pixel
            } else {
                FontMode::System
            },
            proxy,
            generation: 0,
            preview_generation: 0,
            frames_left: options.frames,
            last_frame: Instant::now(),
        }
    }

    /// Creates the window, surface, backend and swap chain on first resume.
    fn init(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attributes = Window::default_attributes()
            .with_title("quill — 文件浏览器")
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
        backend.set_clear_color(theme_background(self.browser.theme()));

        let font_config = FontConfig {
            mode: self.font_mode,
            device_pixel_rasterization: true,
        };
        if let Err(error) = backend.set_font_config(font_config) {
            eprintln!("font setup failed, using fallback: {error}");
        }
        // 用后端的真实字体度量排版，否则量到的宽度跟画出来的宽度不一致，
        // 长文件名就会按错误的宽度换行或被截断。
        self.browser.set_text_measurer(Rc::new(BackendTextMeasurer {
            metrics: backend.text_metrics(),
        }));

        self.window = Some(window);
        self.surface = Some(surface);
        self.backend = Some(backend);
        self.config = Some(config);
        self.last_frame = Instant::now();
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

    /// 把"我想看这个目录"派给工作线程。
    fn spawn_scan(&mut self, path: std::path::PathBuf) {
        let proxy = self.proxy.clone();
        let hidden = self.browser.hidden();
        self.generation += 1;
        let generation = self.generation;
        std::thread::spawn(move || {
            let listing = scan::scan(&path, hidden);
            let _ = proxy.send_event(UserEvent::Listing(generation, listing));
        });
    }

    fn spawn_preview(&mut self, path: std::path::PathBuf) {
        let proxy = self.proxy.clone();
        self.preview_generation += 1;
        let generation = self.preview_generation;
        std::thread::spawn(move || {
            let preview = preview::Preview::read(&path);
            let _ = proxy.send_event(UserEvent::Preview(generation, preview));
        });
    }

    fn feed(&mut self, event: &InputEvent) {
        self.browser.event(event);
    }

    fn render(&mut self, event_loop: &ActiveEventLoop) {
        // 1. 点击转交过来的"打开"（回调里拿不到 &mut self，所以在视图里排队）。
        self.browser.update();
        // 2. 有读目录 / 读文件的请求就派出去 —— 绝不在这一帧里读磁盘。
        if let Some(path) = self.browser.take_navigation() {
            self.spawn_scan(path);
        }
        if let Some(path) = self.browser.take_preview_request() {
            self.spawn_preview(path);
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
        self.last_frame = Instant::now();

        self.browser.layout(viewport);

        let mut ctx = PaintContext::new();
        self.browser.paint(&mut ctx);
        let list = ctx.into_draw_list();

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
        if let Some(left) = self.frames_left.take() {
            let left = left.saturating_sub(1);
            if left == 0 {
                eprintln!("--frames 跑完了，退出");
                event_loop.exit();
            } else {
                self.frames_left = Some(left);
            }
        }
    }
}

impl ApplicationHandler<UserEvent> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.init(event_loop);
    }

    fn user_event(&mut self, _event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::Listing(generation, listing) => {
                // 用户在扫描期间翻走了：这份结果已经过期，丢掉。
                if generation != self.generation {
                    return;
                }
                self.browser.apply_listing(listing);
                // 窗口标题跟着目录走 —— 一眼能看出自己在哪。
                if let Some(window) = self.window.as_ref() {
                    window.set_title(&format!(
                        "{} — quill 文件浏览器",
                        scan::display_path(self.browser.path())
                    ));
                    window.request_redraw();
                }
            }
            UserEvent::Preview(generation, preview) => {
                // 用户在字节回来之前又选中了别的文件：这份结果过期了。
                if generation != self.preview_generation {
                    return;
                }
                self.browser.apply_preview(preview);
                if let Some(window) = self.window.as_ref() {
                    window.request_redraw();
                }
            }
        }
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
            // 平台滚轮 -> `InputEvent::Wheel`。核心只负责路由（把滚轮交给最近
            // 的滚动回调），**事件本身得由宿主造出来** —— 这一段就是那个泵。
            WindowEvent::MouseWheel { delta, .. } => {
                let delta = wheel_pixels(delta, self.scale_factor as f32);
                let position = self.cursor;
                self.feed(&InputEvent::Wheel {
                    position,
                    delta: Vec2::new(0.0, delta),
                });
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let Some(key) = map_key(&event.logical_key) else {
                    return;
                };
                let input = match event.state {
                    ElementState::Pressed => InputEvent::KeyDown { key },
                    ElementState::Released => InputEvent::KeyUp { key },
                };
                self.feed(&input);
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
    fn to_logical(&self, position: PhysicalPosition<f64>) -> Vec2 {
        Vec2::new(
            position.x as f32 / self.scale_factor as f32,
            position.y as f32 / self.scale_factor as f32,
        )
    }
}

/// 平台滚轮 -> 逻辑像素（`y > 0` 表示向下滚，跟 `InputEvent::Wheel` 一致）。
///
/// 符号约定：滚轮向上（远离自己）是"往上翻"，对应偏移变小。这是鼠标滚轮的
/// 通例；某个平台（或触控板的"自然滚动"）如果方向相反，改这一个函数就够，
/// 不用动核心。
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
        WinitKey::Named(NamedKey::Home) => Some(Key::Home),
        WinitKey::Named(NamedKey::End) => Some(Key::End),
        WinitKey::Named(NamedKey::ArrowUp) => Some(Key::ArrowUp),
        WinitKey::Named(NamedKey::ArrowDown) => Some(Key::ArrowDown),
        WinitKey::Named(NamedKey::ArrowLeft) => Some(Key::ArrowLeft),
        WinitKey::Named(NamedKey::ArrowRight) => Some(Key::ArrowRight),
        WinitKey::Named(NamedKey::F5) => Some(Key::F5),
        WinitKey::Character(text) => text.chars().next().map(Key::Character),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wheel_up_scrolls_up() {
        // 滚轮向上（远离自己）-> 偏移变小 -> 看到更早的内容。
        assert!(wheel_pixels(MouseScrollDelta::LineDelta(0.0, 1.0), 1.0) < 0.0);
        assert!(wheel_pixels(MouseScrollDelta::LineDelta(0.0, -1.0), 1.0) > 0.0);
    }

    #[test]
    fn pixel_deltas_are_converted_to_logical() {
        // 2x 缩放下 40 物理像素 = 20 逻辑像素。
        let pixels = wheel_pixels(
            MouseScrollDelta::PixelDelta(PhysicalPosition::new(0.0, -40.0)),
            2.0,
        );
        assert_eq!(pixels, 20.0);
    }

    #[test]
    fn one_notch_covers_about_three_rows() {
        let pixels = wheel_pixels(MouseScrollDelta::LineDelta(0.0, -1.0), 1.0);
        assert!((pixels - 3.0 * ROW_HEIGHT).abs() < 0.001);
    }

    #[test]
    fn navigation_keys_are_mapped() {
        assert_eq!(
            map_key(&WinitKey::Named(NamedKey::ArrowDown)),
            Some(Key::ArrowDown)
        );
        assert_eq!(
            map_key(&WinitKey::Named(NamedKey::Backspace)),
            Some(Key::Backspace)
        );
        assert_eq!(
            map_key(&WinitKey::Character("r".into())),
            Some(Key::Character('r'))
        );
        // 没映射的键不该变成别的键。
        assert_eq!(map_key(&WinitKey::Named(NamedKey::Shift)), None);
    }
}
