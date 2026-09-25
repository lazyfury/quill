//! The `winit` + wgpu window host for the game demo.

use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use draw_backend_wgpu::{wgpu, FontConfig, FontMetrics, FontMode, WgpuBackend};
use draw_core::{FontWeight, InputEvent, Key, Size, ViewportSize};
use draw_render::{PaintContext, RenderBackend};
use draw_ui::TextMeasurer;
use game_demo::Game;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key as WinitKey, NamedKey};
use winit::window::{Window, WindowId};

/// Adapts the backend's font metrics to the HUD layout engine.
struct BackendTextMeasurer {
    metrics: FontMetrics,
}

impl TextMeasurer for BackendTextMeasurer {
    fn advance(&self, ch: char, font_size: f32) -> f32 {
        self.metrics.advance(ch, font_size)
    }

    fn advance_weighted(&self, ch: char, font_size: f32, weight: FontWeight) -> f32 {
        self.metrics.advance_weighted(ch, font_size, weight)
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

    fn measure_run_weighted(&self, text: &str, font_size: f32, weight: FontWeight) -> f32 {
        self.metrics.measure_run_weighted(text, font_size, weight)
    }
}

/// Runs the demo until the window is closed.
pub fn run() {
    let event_loop = EventLoop::new().expect("create event loop");
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut app = App::new();
    event_loop.run_app(&mut app).expect("run event loop");
}

struct App {
    instance: wgpu::Instance,
    window: Option<Arc<Window>>,
    surface: Option<wgpu::Surface<'static>>,
    backend: Option<WgpuBackend>,
    config: Option<wgpu::SurfaceConfiguration>,
    scale: f64,
    game: Option<Game>,
    last_frame: Instant,
    font_mode: FontMode,
}

impl App {
    fn new() -> Self {
        Self {
            instance: wgpu::Instance::default(),
            window: None,
            surface: None,
            backend: None,
            config: None,
            scale: 1.0,
            game: None,
            last_frame: Instant::now(),
            font_mode: FontMode::System,
        }
    }

    fn init(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let window = Arc::new(
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_title("quill — Game")
                        .with_inner_size(LogicalSize::new(900.0, 640.0)),
                )
                .expect("create window"),
        );
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

        self.scale = window.scale_factor();
        backend.set_scale_factor(self.scale as f32);
        backend.set_clear_color(draw_core::Color::new(0.05, 0.06, 0.08, 1.0));
        if let Err(error) = backend.set_font_config(FontConfig {
            mode: self.font_mode,
            device_pixel_rasterization: true,
            ..Default::default()
        }) {
            eprintln!("font setup failed: {error}");
        }

        let mut game = Game::new();
        game.set_text_measurer(Rc::new(BackendTextMeasurer {
            metrics: backend.text_metrics(),
        }));
        game.set_scale_factor(self.scale as f32);
        if let Err(error) = game.init(&mut backend) {
            eprintln!("game init failed: {error:?}");
        }

        self.window = Some(window);
        self.surface = Some(surface);
        self.backend = Some(backend);
        self.config = Some(config);
        self.game = Some(game);
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

    fn render(&mut self) {
        let scale = self.scale;
        let now = Instant::now();
        if let Some(game) = self.game.as_mut() {
            game.set_scale_factor(scale as f32);
        }
        let Some((surface, backend, config)) = self
            .surface
            .as_ref()
            .zip(self.backend.as_mut())
            .zip(self.config.as_ref())
            .map(|((surface, backend), config)| (surface, backend, config))
        else {
            return;
        };
        let Some(game) = self.game.as_mut() else {
            return;
        };

        let logical = Size::new(
            config.width as f32 / scale as f32,
            config.height as f32 / scale as f32,
        );
        let viewport = ViewportSize::new(logical);
        let dt = now.duration_since(self.last_frame).as_secs_f32().min(0.1);
        self.last_frame = now;

        game.layout(viewport);
        if let Err(error) = game.advance(dt, backend) {
            eprintln!("advance failed: {error:?}");
            return;
        }
        let mut ctx = PaintContext::new();
        game.paint(&mut ctx);
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

        if game.needs_frame() {
            if let Some(window) = self.window.as_ref() {
                window.request_redraw();
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
                self.scale = scale_factor;
                if let Some(backend) = self.backend.as_mut() {
                    backend.set_scale_factor(scale_factor as f32);
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let Some(key) = map_key(&event.logical_key) else {
                    return;
                };
                let input = match event.state {
                    ElementState::Pressed => InputEvent::KeyDown { key },
                    ElementState::Released => InputEvent::KeyUp { key },
                };
                if let Some(game) = self.game.as_mut() {
                    game.event(&input);
                }
            }
            _ => {}
        }
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }
}

fn map_key(key: &WinitKey) -> Option<Key> {
    match key {
        WinitKey::Named(NamedKey::ArrowLeft) => Some(Key::ArrowLeft),
        WinitKey::Named(NamedKey::ArrowRight) => Some(Key::ArrowRight),
        WinitKey::Named(NamedKey::ArrowUp) => Some(Key::ArrowUp),
        WinitKey::Named(NamedKey::ArrowDown) => Some(Key::ArrowDown),
        WinitKey::Named(NamedKey::Escape) => Some(Key::Escape),
        WinitKey::Character(text) => text.chars().next().map(Key::Character),
        _ => None,
    }
}
