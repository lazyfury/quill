//! The `winit` window runner for the wgpu demo (native only).

use std::sync::Arc;
use std::time::Instant;

use draw_backend_wgpu::{wgpu, WgpuBackend};
use draw_core::{InputEvent, Key, PointerButton, Size, Vec2, Viewport};
use draw_render::{PaintContext, RenderBackend};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition};
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key as WinitKey, NamedKey};
use winit::window::{Window, WindowId};

use crate::demo::Demo;

/// Runs the demo until the window is closed.
pub fn run() {
    let event_loop = EventLoop::new().expect("create event loop");
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App::new();
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
    last_frame: Instant,
}

impl App {
    fn new() -> Self {
        Self {
            instance: wgpu::Instance::default(),
            window: None,
            surface: None,
            backend: None,
            config: None,
            scale_factor: 1.0,
            cursor: Vec2::ZERO,
            demo: Demo::new(),
            last_frame: Instant::now(),
        }
    }

    /// Creates the window, surface, backend and swap chain on first resume.
    fn init(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attributes = Window::default_attributes()
            .with_title("quill — wgpu demo")
            .with_inner_size(LogicalSize::new(900.0, 620.0));
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
        backend.set_clear_color(draw_core::Color::new(0.09, 0.10, 0.13, 1.0));

        self.window = Some(window);
        self.surface = Some(surface);
        self.backend = Some(backend);
        self.config = Some(config);
        self.last_frame = Instant::now();
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
        self.demo.event(event);
    }

    fn render(&mut self) {
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
        let viewport = Viewport::new(logical);

        let now = Instant::now();
        let dt = now.duration_since(self.last_frame).as_secs_f32().min(0.1);
        self.last_frame = now;
        self.demo.update(viewport, dt);

        let mut ctx = PaintContext::new();
        self.demo.paint(&mut ctx);
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
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => self.resize(size.width, size.height),
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.scale_factor = scale_factor;
                if let Some(backend) = self.backend.as_mut() {
                    backend.set_scale_factor(scale_factor as f32);
                }
            }
            WindowEvent::RedrawRequested => self.render(),
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
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
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
        WinitKey::Character(text) => text.chars().next().map(Key::Character),
        _ => None,
    }
}
