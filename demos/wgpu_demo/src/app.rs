//! The `winit` window runner for the wgpu demo (native only).

use std::sync::Arc;
use std::time::Instant;

use draw_backend_wgpu::{wgpu, FontConfig, FontMode, WgpuBackend};
use draw_core::{InputEvent, Key, PointerButton, Size, Vec2, ViewportSize};
use draw_debug_ui::{DebugOverlay, PerformanceOverlay};
use draw_profile::{inspect, FrameCounters, FrameStats, InspectionReport, Profiler, StageTimes};
use draw_render::{PaintContext, RenderBackend};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition};
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key as WinitKey, NamedKey};
use winit::window::{CursorIcon, Window, WindowId};

use crate::cli::Options;
use crate::demo::Demo;

/// Runs the demo until the window is closed.
pub fn run(options: Options) {
    let event_loop = EventLoop::new().expect("create event loop");
    // Event-driven: the app is static, so render only when something changes
    // (input, resize, overlay toggle). `Poll` would burn CPU redrawing an
    // unchanged frame as fast as possible.
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut app = App::new(options);
    event_loop.run_app(&mut app).expect("run event loop");
}

/// Owns the window, surface, backend and demo state.
struct App {
    instance: wgpu::Instance,
    window: Option<Arc<Window>>,
    surface: Option<wgpu::Surface<'static>>,
    backend: Option<WgpuBackend>,
    config: Option<wgpu::SurfaceConfiguration>,
    scale_factor: f64,
    cursor: Vec2,
    demo: Demo,
    /// Current text font mode (toggle with `f`).
    font_mode: FontMode,
    last_frame: Instant,
    /// Frame timings / counters for the debug overlay.
    profiler: Profiler,
    /// Findings from inspecting the previous frame's `DrawList`.
    report: InspectionReport,
    /// Component debug drawing (yellow bounds + `name#id`), toggled with F3 / ` / d.
    debug: DebugOverlay,
    /// Performance panel, toggled with F4 / p.
    perf: PerformanceOverlay,
}

impl App {
    fn new(options: Options) -> Self {
        // Profiler and overlay start in the state requested on the command line;
        // both remain runtime-switchable (overlay: backtick key).
        let mut profiler = Profiler::new();
        profiler.set_enabled(options.profiler);
        let mut debug = DebugOverlay::new();
        debug.set_open(options.debug_ui);
        let mut perf = PerformanceOverlay::new();
        perf.set_open(options.performance);

        Self {
            instance: wgpu::Instance::default(),
            window: None,
            surface: None,
            backend: None,
            config: None,
            scale_factor: 1.0,
            cursor: Vec2::ZERO,
            demo: Demo::new(),
            font_mode: if options.pixel_font {
                FontMode::Pixel
            } else {
                FontMode::System
            },
            last_frame: Instant::now(),
            profiler,
            report: InspectionReport::new(),
            debug,
            perf,
        }
    }

    /// Creates the window, surface, backend and swap chain on first resume.
    fn init(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attributes = Window::default_attributes()
            .with_title("quill — Notes")
            .with_inner_size(LogicalSize::new(1200.0, 780.0));
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

        // Prefer a non-sRGB format so the unorm colors written by the shader
        // match the Canvas backend; fall back to whatever the surface offers.
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
        backend.set_clear_color(draw_core::Color::new(0.039, 0.039, 0.039, 1.0));

        // Measure UI text with the backend's actual font.
        let font_config = FontConfig {
            mode: self.font_mode,
            device_pixel_rasterization: true,
        };
        if let Err(error) = backend.set_font_config(font_config) {
            eprintln!("font setup failed, using fallback: {error}");
        }
        self.demo.set_text_metrics(backend.text_metrics());

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

    /// Switches between the system font and the built-in pixel font.
    fn toggle_font(&mut self) {
        self.font_mode = match self.font_mode {
            FontMode::System => FontMode::Pixel,
            FontMode::Pixel => FontMode::System,
        };
        let config = FontConfig {
            mode: self.font_mode,
            device_pixel_rasterization: true,
        };
        if let Some(backend) = self.backend.as_mut() {
            if let Err(error) = backend.set_font_config(config) {
                eprintln!("font switch failed: {error}");
            }
            let metrics = backend.text_metrics();
            self.demo.set_text_metrics(metrics);
        }
    }

    fn feed(&mut self, event: &InputEvent) {
        if let InputEvent::KeyDown { key } = event {
            match key {
                Key::F3 | Key::Character('`') | Key::Character('d') => {
                    self.debug.toggle();
                    return;
                }
                Key::F4 | Key::Character('p') => {
                    self.perf.toggle();
                    return;
                }
                Key::F5 | Key::Character('o') => {
                    self.profiler.toggle();
                    return;
                }
                Key::Character('f') => {
                    self.toggle_font();
                    return;
                }
                _ => {}
            }
        }
        // The performance panel sits on top: consume input over its panel,
        // pass the rest on to the app UI.
        if self.perf.handle_input(event).is_handled() {
            return;
        }
        self.demo.event(event);
    }

    fn apply_cursor(&self) {
        let Some(window) = self.window.as_ref() else {
            return;
        };
        let cursor = if self.demo.pointer_over_clickable() {
            CursorIcon::Pointer
        } else {
            CursorIcon::Default
        };
        window.set_cursor(cursor);
    }

    fn render(&mut self) {
        self.apply_cursor();

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

        // -- timed pipeline phases ----------------------------------------
        let frame_start = Instant::now();
        let dt = frame_start
            .duration_since(self.last_frame)
            .as_secs_f32()
            .min(0.1);
        self.last_frame = frame_start;

        self.demo.update(viewport, dt);
        let update_done = Instant::now();

        self.demo.layout(viewport);
        let layout_done = Instant::now();

        let mut ctx = PaintContext::new();
        self.demo.paint(&mut ctx);
        // 1) component debug bounds, drawn on top of the app UI.
        self.debug.paint(self.demo.tree(), &mut ctx);
        // 2) performance panel, drawn last so it stays readable.
        self.perf.update(&self.profiler, &self.report, viewport);
        self.perf.paint(&mut ctx);
        let list = ctx.into_draw_list();
        let paint_done = Instant::now();

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
        let render_done = Instant::now();

        surface_texture.present();
        let frame_done = Instant::now();

        // -- record + inspect the frame -----------------------------------
        let stats = FrameStats {
            index: self.profiler.next_index(),
            frame_ms: millis(frame_done - frame_start),
            stages: StageTimes::new(
                millis(update_done - frame_start),
                millis(layout_done - update_done),
                millis(paint_done - layout_done),
                millis(render_done - paint_done),
            ),
            counters: FrameCounters::new(0, self.demo.control_count(), list.len(), 1),
        };
        self.profiler.record(stats);
        // Only audit when the performance panel can actually show it.
        if self.perf.is_open() {
            self.report = inspect(&list, &stats);
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
            self.render();
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

fn pointer_button(button: MouseButton) -> PointerButton {
    match button {
        MouseButton::Right => PointerButton::Right,
        MouseButton::Middle => PointerButton::Middle,
        _ => PointerButton::Left,
    }
}

fn millis(duration: std::time::Duration) -> f32 {
    duration.as_secs_f32() * 1000.0
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
        WinitKey::Named(NamedKey::F1) => Some(Key::F1),
        WinitKey::Named(NamedKey::F2) => Some(Key::F2),
        WinitKey::Named(NamedKey::F3) => Some(Key::F3),
        WinitKey::Named(NamedKey::F4) => Some(Key::F4),
        WinitKey::Named(NamedKey::F5) => Some(Key::F5),
        WinitKey::Named(NamedKey::F6) => Some(Key::F6),
        WinitKey::Named(NamedKey::F7) => Some(Key::F7),
        WinitKey::Named(NamedKey::F8) => Some(Key::F8),
        WinitKey::Named(NamedKey::F9) => Some(Key::F9),
        WinitKey::Named(NamedKey::F10) => Some(Key::F10),
        WinitKey::Named(NamedKey::F11) => Some(Key::F11),
        WinitKey::Named(NamedKey::F12) => Some(Key::F12),
        WinitKey::Character(text) => text.chars().next().map(Key::Character),
        _ => None,
    }
}
