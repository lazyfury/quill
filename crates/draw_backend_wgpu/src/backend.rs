//! A [`RenderBackend`] that rasterizes a [`DrawList`] with `wgpu`.
//!
//! The backend renders to an offscreen `Rgba8Unorm` texture. That keeps the
//! whole `DrawList -> pixels` path verifiable with native `cargo test` (no
//! window, no browser, no screenshot): render a frame, then read the target's
//! pixels back and assert on them via [`WgpuBackend::read_pixels`].
//!
//! # How the IR maps onto the GPU
//!
//! Transform, opacity and clip are resolved on the CPU while tessellating so
//! the shader stays a trivial textured-quad pass:
//!
//! - `SetTransform` / `SetOpacity` / `ClipRect` update a state stack.
//! - `FillRect` / `StrokeRect` / `FillCircle` / `StrokeCircle` tessellate into
//!   triangles; positions are converted to NDC and opacity is folded into the
//!   vertex color.
//! - `ClipRect` becomes a scissor rectangle per draw range.
//! - `DrawImage` samples a texture registered with
//!   [`WgpuBackend::register_texture`].
//! - `DrawText` samples the built-in bitmap-font atlas ([`crate::font`]).
//!
//! No `DrawCommand` is modified and no backend type leaks into the IR.

use std::collections::HashMap;
use std::fmt;
use std::ops::Range;
use std::sync::mpsc;

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use draw_core::{Color, Rect, Size, Transform2D, Vec2, Viewport};
use draw_render::{DrawCommand, DrawList, Paint, RenderBackend, TextAlign, TextureId};

use crate::font;
use crate::shader::SHADER;

/// Formats the offscreen render target uses.
const TARGET_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
/// Circle tessellation resolution.
const CIRCLE_SEGMENTS: u32 = 48;
/// UVs used when sampling the 1x1 white texture (any UV works).
const SOLID_UV: [f32; 4] = [0.0, 0.0, 1.0, 1.0];

/// Errors from the wgpu backend.
#[derive(Debug)]
pub enum WgpuError {
    /// No adapter matching the requested options was found.
    NoAdapter,
    /// Creating or using the logical device failed.
    Device(String),
    /// `begin_frame` was called while a frame is already open.
    AlreadyInFrame,
    /// `submit` / `end_frame` was called with no open frame.
    NotInFrame,
    /// No render target exists (no `begin_frame` has completed).
    NoTarget,
    /// A registered texture had zero size or too little pixel data.
    InvalidTexture(String),
}

impl fmt::Display for WgpuError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoAdapter => f.write_str("no suitable wgpu adapter found"),
            Self::Device(message) => write!(f, "wgpu device error: {message}"),
            Self::AlreadyInFrame => f.write_str("begin_frame called while a frame is already open"),
            Self::NotInFrame => f.write_str("submit/end_frame called with no open frame"),
            Self::NoTarget => f.write_str("no render target (begin_frame has not completed)"),
            Self::InvalidTexture(message) => write!(f, "invalid texture: {message}"),
        }
    }
}

impl std::error::Error for WgpuError {}

/// RGBA8 pixels read back from the render target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PixelBuffer {
    pub width: u32,
    pub height: u32,
    /// Tightly packed RGBA rows (`width * height * 4` bytes).
    pub data: Vec<u8>,
}

impl PixelBuffer {
    /// Returns the RGBA pixel at `(x, y)`, or `None` when out of bounds.
    pub fn pixel(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let offset = ((y * self.width + x) * 4) as usize;
        Some([
            self.data[offset],
            self.data[offset + 1],
            self.data[offset + 2],
            self.data[offset + 3],
        ])
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct Vertex {
    /// Normalized device coordinates.
    position: [f32; 2],
    uv: [f32; 2],
    color: [f32; 4],
}

impl Vertex {
    const ATTRIBUTES: [wgpu::VertexAttribute; 3] =
        wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x4];

    fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: core::mem::size_of::<Self>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBUTES,
        }
    }
}

/// Which texture a draw range samples.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Surface {
    /// The 1x1 white texture, tinted by the vertex color.
    Solid,
    /// A texture registered with [`WgpuBackend::register_texture`].
    Texture(TextureId),
    /// The built-in bitmap-font atlas.
    Font,
}

/// A contiguous vertex range plus the state it is drawn under.
#[derive(Debug, Clone)]
struct DrawRange {
    vertices: Range<u32>,
    surface: Surface,
    /// `None` means "full target"; otherwise `[x, y, width, height]`.
    scissor: Option<[u32; 4]>,
}

/// The clip after resolving the current state against the target.
#[derive(Debug, Clone, Copy)]
enum ClipResult {
    Full,
    Scissor([u32; 4]),
    Empty,
}

/// CPU-side paint state mirroring `Save` / `Restore`.
#[derive(Debug, Clone, Copy)]
struct State {
    transform: Transform2D,
    opacity: f32,
    clip: Option<Rect>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            transform: Transform2D::IDENTITY,
            opacity: 1.0,
            clip: None,
        }
    }
}

/// The persistent offscreen render target, recreated only when the size changes.
struct OffscreenTarget {
    width: u32,
    height: u32,
    texture: wgpu::Texture,
    view: wgpu::TextureView,
}

/// The render target of the frame currently being built.
///
/// Offscreen frames point at [`OffscreenTarget::view`]; window frames point at
/// the surface texture view supplied by the caller.
struct Frame {
    view: wgpu::TextureView,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
    offscreen: bool,
}

/// A `wgpu` [`RenderBackend`] rendering to an offscreen texture or a surface.
pub struct WgpuBackend {
    instance: wgpu::Instance,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
    shader: wgpu::ShaderModule,
    bind_group_layout: wgpu::BindGroupLayout,
    /// One pipeline per color-target format (offscreen plus surface formats).
    pipelines: HashMap<wgpu::TextureFormat, wgpu::RenderPipeline>,

    white_bind_group: wgpu::BindGroup,
    font_bind_group: wgpu::BindGroup,
    image_sampler: wgpu::Sampler,
    textures: HashMap<TextureId, wgpu::BindGroup>,
    texture_sizes: HashMap<TextureId, (u32, u32)>,

    offscreen: Option<OffscreenTarget>,
    frame: Option<Frame>,

    // Per-frame CPU staging.
    in_frame: bool,
    viewport: Viewport,
    scale_factor: f32,
    clear: Color,
    device_width: f32,
    device_height: f32,
    state: State,
    stack: Vec<State>,
    vertices: Vec<Vertex>,
    ranges: Vec<DrawRange>,
}

impl WgpuBackend {
    /// Creates a headless backend using the high-performance adapter.
    pub fn new() -> Result<Self, WgpuError> {
        Self::with_power_preference(wgpu::PowerPreference::HighPerformance)
    }

    /// Creates a headless backend, choosing an adapter by `power_preference`.
    pub fn with_power_preference(
        power_preference: wgpu::PowerPreference,
    ) -> Result<Self, WgpuError> {
        let instance = wgpu::Instance::default();
        Self::from_instance(&instance, None, power_preference)
    }

    /// Creates a backend on an existing [`wgpu::Instance`].
    ///
    /// Pass `compatible_surface` when the backend will render to a window
    /// surface, so the adapter is selected for that surface (required on
    /// WebGL). The instance is cloned and kept alive by the backend.
    pub fn from_instance(
        instance: &wgpu::Instance,
        compatible_surface: Option<&wgpu::Surface<'_>>,
        power_preference: wgpu::PowerPreference,
    ) -> Result<Self, WgpuError> {
        let instance = instance.clone();
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference,
            compatible_surface,
            force_fallback_adapter: false,
        }))
        .ok_or(WgpuError::NoAdapter)?;

        let (device, queue) = pollster::block_on(adapter.request_device(
            &wgpu::DeviceDescriptor {
                label: Some("draw_backend_wgpu.device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::default(),
            },
            None,
        ))
        .map_err(|error| WgpuError::Device(error.to_string()))?;

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("draw_backend_wgpu.bind_group_layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("draw_backend_wgpu.shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });

        let image_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("draw_backend_wgpu.image_sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let font_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("draw_backend_wgpu.font_sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        let white_bind_group = {
            let texture = upload_texture(
                &device,
                &queue,
                "draw_backend_wgpu.white",
                &[255, 255, 255, 255],
                1,
                1,
            );
            let view = texture.create_view(&Default::default());
            bind_group(&device, &bind_group_layout, &view, &image_sampler, "white")
        };

        let font_bind_group = {
            let atlas = font::build_atlas();
            let texture = upload_texture(
                &device,
                &queue,
                "draw_backend_wgpu.font_atlas",
                &atlas,
                font::ATLAS_WIDTH,
                font::ATLAS_HEIGHT,
            );
            let view = texture.create_view(&Default::default());
            bind_group(
                &device,
                &bind_group_layout,
                &view,
                &font_sampler,
                "font_atlas",
            )
        };

        Ok(Self {
            instance,
            adapter,
            device,
            queue,
            shader,
            bind_group_layout,
            pipelines: HashMap::new(),
            white_bind_group,
            font_bind_group,
            image_sampler,
            textures: HashMap::new(),
            texture_sizes: HashMap::new(),
            offscreen: None,
            frame: None,
            in_frame: false,
            viewport: Viewport::default(),
            scale_factor: 1.0,
            clear: Color::TRANSPARENT,
            device_width: 1.0,
            device_height: 1.0,
            state: State::default(),
            stack: Vec::new(),
            vertices: Vec::new(),
            ranges: Vec::new(),
        })
    }

    /// Sets the logical-to-device scale factor (DPR). Core/IR stay logical.
    pub fn set_scale_factor(&mut self, scale_factor: f32) {
        self.scale_factor = scale_factor.max(0.0);
    }

    /// Returns the [`wgpu::Instance`] the backend uses.
    pub fn instance(&self) -> &wgpu::Instance {
        &self.instance
    }

    /// Returns the adapter the backend selected. Use it to query surface
    /// capabilities or adapter info.
    pub fn adapter(&self) -> &wgpu::Adapter {
        &self.adapter
    }

    /// Returns the logical device used for rendering.
    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    /// Returns the queue used for submissions and texture uploads.
    pub fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    /// Returns the current scale factor.
    pub fn scale_factor(&self) -> f32 {
        self.scale_factor
    }

    /// Returns the viewport of the most recent [`RenderBackend::begin_frame`].
    pub fn viewport(&self) -> Viewport {
        self.viewport
    }

    /// Sets the color the render target is cleared to each frame.
    pub fn set_clear_color(&mut self, color: Color) {
        self.clear = color;
    }

    /// Begins a frame that renders into an external view, such as the texture
    /// view of a window surface.
    ///
    /// `width`/`height` are in device pixels and `format` must match the view's
    /// texture format. The view is cloned internally, so the caller keeps
    /// ownership of the surface texture and presents it after
    /// [`RenderBackend::end_frame`].
    pub fn begin_frame_with_view(
        &mut self,
        view: wgpu::TextureView,
        width: u32,
        height: u32,
        format: wgpu::TextureFormat,
        viewport: Viewport,
    ) -> Result<(), WgpuError> {
        let width = width.max(1);
        let height = height.max(1);
        self.ensure_pipeline(format);
        self.start_frame(viewport, width, height)?;
        self.frame = Some(Frame {
            view,
            width,
            height,
            format,
            offscreen: false,
        });
        Ok(())
    }

    /// Returns `true` when the current frame targets the offscreen texture, so
    /// [`WgpuBackend::read_pixels`] will reflect this frame.
    pub fn is_offscreen_frame(&self) -> bool {
        self.frame.as_ref().is_some_and(|frame| frame.offscreen)
    }

    /// Ensures a render pipeline exists for `format`, creating it on demand.
    fn ensure_pipeline(&mut self, format: wgpu::TextureFormat) {
        if self.pipelines.contains_key(&format) {
            return;
        }
        let pipeline =
            create_render_pipeline(&self.device, &self.shader, &self.bind_group_layout, format);
        self.pipelines.insert(format, pipeline);
    }

    /// Resets per-frame staging and validates the lifecycle.
    fn start_frame(
        &mut self,
        viewport: Viewport,
        width: u32,
        height: u32,
    ) -> Result<(), WgpuError> {
        if self.in_frame {
            return Err(WgpuError::AlreadyInFrame);
        }
        self.in_frame = true;
        self.viewport = viewport;
        self.device_width = width as f32;
        self.device_height = height as f32;
        self.state = State::default();
        self.stack.clear();
        self.vertices.clear();
        self.ranges.clear();
        Ok(())
    }

    /// Registers an RGBA8 image so `DrawImage` can reference it by [`TextureId`].
    pub fn register_texture(
        &mut self,
        id: TextureId,
        width: u32,
        height: u32,
        rgba: &[u8],
    ) -> Result<(), WgpuError> {
        if width == 0 || height == 0 {
            return Err(WgpuError::InvalidTexture("zero width or height".into()));
        }
        let expected = width as usize * height as usize * 4;
        if rgba.len() < expected {
            return Err(WgpuError::InvalidTexture(format!(
                "expected at least {expected} bytes, got {}",
                rgba.len()
            )));
        }
        let texture = upload_texture(
            &self.device,
            &self.queue,
            "draw_backend_wgpu.texture",
            &rgba[..expected],
            width,
            height,
        );
        let view = texture.create_view(&Default::default());
        let group = bind_group(
            &self.device,
            &self.bind_group_layout,
            &view,
            &self.image_sampler,
            "texture",
        );
        self.textures.insert(id, group);
        self.texture_sizes.insert(id, (width, height));
        Ok(())
    }

    /// Reads the current offscreen render target back into RGBA8 pixels.
    ///
    /// Call after [`RenderBackend::end_frame`] with an offscreen frame (see
    /// [`WgpuBackend::is_offscreen_frame`]). Blocks until the GPU work has
    /// completed, so the returned pixels are deterministic.
    pub fn read_pixels(&self) -> Result<PixelBuffer, WgpuError> {
        let target = self.offscreen.as_ref().ok_or(WgpuError::NoTarget)?;
        self.read_texture(&target.texture, target.width, target.height)
    }

    /// Reads an arbitrary texture of this backend's device back into RGBA8
    /// pixels. `texture` must have been created with `COPY_SRC`.
    pub fn read_texture(
        &self,
        texture: &wgpu::Texture,
        width: u32,
        height: u32,
    ) -> Result<PixelBuffer, WgpuError> {
        let unpadded = width * 4;
        let padded = unpadded.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
            * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;

        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("draw_backend_wgpu.readback"),
            size: (padded as u64) * (height as u64),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("draw_backend_wgpu.readback_encoder"),
            });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        self.queue.submit(Some(encoder.finish()));

        let slice = buffer.slice(..);
        let (sender, receiver) = mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
        let _ = self.device.poll(wgpu::Maintain::Wait);
        receiver
            .recv()
            .map_err(|error| WgpuError::Device(error.to_string()))?
            .map_err(|error| WgpuError::Device(format!("{error:?}")))?;

        let mapped = slice.get_mapped_range();
        let mut data = Vec::with_capacity((unpadded * height) as usize);
        for row in 0..height {
            let start = (row * padded) as usize;
            data.extend_from_slice(&mapped[start..start + unpadded as usize]);
        }
        drop(mapped);
        buffer.unmap();

        Ok(PixelBuffer {
            width,
            height,
            data,
        })
    }

    // -- frame lifecycle ---------------------------------------------------

    fn ensure_offscreen(&mut self, width: u32, height: u32) {
        if matches!(&self.offscreen, Some(target) if target.width == width && target.height == height)
        {
            return;
        }
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("draw_backend_wgpu.offscreen"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: TARGET_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        self.offscreen = Some(OffscreenTarget {
            width,
            height,
            texture,
            view,
        });
    }

    fn render_frame(&self) {
        let Some(frame) = self.frame.as_ref() else {
            return;
        };
        let Some(pipeline) = self.pipelines.get(&frame.format) else {
            return;
        };
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("draw_backend_wgpu.encoder"),
            });

        let vertex_buffer = if self.vertices.is_empty() {
            None
        } else {
            Some(
                self.device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("draw_backend_wgpu.vertices"),
                        contents: bytemuck::cast_slice(&self.vertices),
                        usage: wgpu::BufferUsages::VERTEX,
                    }),
            )
        };

        let clear = wgpu::Color {
            r: self.clear.r as f64,
            g: self.clear.g as f64,
            b: self.clear.b as f64,
            a: self.clear.a as f64,
        };

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("draw_backend_wgpu.pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &frame.view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(clear),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            if let Some(buffer) = &vertex_buffer {
                pass.set_pipeline(pipeline);
                pass.set_vertex_buffer(0, buffer.slice(..));
                for range in &self.ranges {
                    pass.set_bind_group(0, self.bind_group_for(range.surface), &[]);
                    match range.scissor {
                        Some(scissor) => {
                            pass.set_scissor_rect(scissor[0], scissor[1], scissor[2], scissor[3])
                        }
                        None => pass.set_scissor_rect(0, 0, frame.width, frame.height),
                    }
                    pass.draw(range.vertices.clone(), 0..1);
                }
            }
        }

        self.queue.submit(Some(encoder.finish()));
    }

    // -- command execution -------------------------------------------------

    fn bind_group_for(&self, surface: Surface) -> &wgpu::BindGroup {
        match surface {
            Surface::Solid => &self.white_bind_group,
            Surface::Font => &self.font_bind_group,
            Surface::Texture(id) => self.textures.get(&id).unwrap_or(&self.white_bind_group),
        }
    }

    fn execute(&mut self, command: &DrawCommand) {
        match command {
            DrawCommand::Save => self.stack.push(self.state),
            DrawCommand::Restore => {
                if let Some(state) = self.stack.pop() {
                    self.state = state;
                }
            }
            DrawCommand::SetTransform(transform) => self.state.transform = *transform,
            DrawCommand::SetOpacity(opacity) => self.state.opacity = opacity.clamp(0.0, 1.0),
            DrawCommand::ClipRect(rect) => self.state.clip = Some(*rect),
            DrawCommand::FillRect { rect, paint } => {
                let color = self.solid_color(paint);
                self.quad(*rect, SOLID_UV, color, Surface::Solid);
            }
            DrawCommand::StrokeRect { rect, paint, width } => {
                let color = self.solid_color(paint);
                self.stroke_rect(*rect, *width, color);
            }
            DrawCommand::FillCircle {
                center,
                radius,
                paint,
            } => {
                let color = self.solid_color(paint);
                self.fill_circle(*center, *radius, color);
            }
            DrawCommand::StrokeCircle {
                center,
                radius,
                paint,
                width,
            } => {
                let color = self.solid_color(paint);
                self.stroke_circle(*center, *radius, *width, color);
            }
            DrawCommand::DrawImage {
                texture,
                destination,
                source,
                paint,
            } => {
                if let Some(uv) = self.image_uv(*texture, *source) {
                    let color = self.solid_color(paint);
                    self.quad(*destination, uv, color, Surface::Texture(*texture));
                }
            }
            DrawCommand::DrawText {
                text,
                position,
                font_size,
                align,
                paint,
            } => {
                let color = self.solid_color(paint);
                self.draw_text(text, *position, *font_size, *align, color);
            }
        }
    }

    fn solid_color(&self, paint: &Paint) -> [f32; 4] {
        let color = paint.color;
        [
            color.r,
            color.g,
            color.b,
            (color.a * self.state.opacity).clamp(0.0, 1.0),
        ]
    }

    fn clip_result(&self) -> ClipResult {
        let Some(rect) = self.state.clip else {
            return ClipResult::Full;
        };
        let scale = self.scale_factor;
        let x0 = (rect.left() * scale).floor().clamp(0.0, self.device_width);
        let y0 = (rect.top() * scale).floor().clamp(0.0, self.device_height);
        let x1 = (rect.right() * scale).ceil().clamp(0.0, self.device_width);
        let y1 = (rect.bottom() * scale)
            .ceil()
            .clamp(0.0, self.device_height);
        if x1 <= x0 || y1 <= y0 {
            return ClipResult::Empty;
        }
        ClipResult::Scissor([x0 as u32, y0 as u32, (x1 - x0) as u32, (y1 - y0) as u32])
    }

    /// Captures the vertex start and scissor for a primitive, or `None` when
    /// the current clip is empty.
    fn begin(&self) -> Option<(u32, Option<[u32; 4]>)> {
        match self.clip_result() {
            ClipResult::Empty => None,
            ClipResult::Full => Some((self.vertices.len() as u32, None)),
            ClipResult::Scissor(rect) => Some((self.vertices.len() as u32, Some(rect))),
        }
    }

    fn finish(&mut self, geometry: (u32, Option<[u32; 4]>), surface: Surface) {
        let (start, scissor) = geometry;
        let end = self.vertices.len() as u32;
        if end > start {
            self.ranges.push(DrawRange {
                vertices: start..end,
                surface,
                scissor,
            });
        }
    }

    fn push_vertex(&mut self, local: Vec2, uv: [f32; 2], color: [f32; 4]) {
        let logical = self.state.transform.transform_point(local);
        let device = Vec2::new(logical.x * self.scale_factor, logical.y * self.scale_factor);
        let position = [
            device.x / self.device_width * 2.0 - 1.0,
            1.0 - device.y / self.device_height * 2.0,
        ];
        self.vertices.push(Vertex {
            position,
            uv,
            color,
        });
    }

    fn quad(&mut self, rect: Rect, uv: [f32; 4], color: [f32; 4], surface: Surface) {
        if rect.size.width <= 0.0 || rect.size.height <= 0.0 {
            return;
        }
        let Some(geometry) = self.begin() else {
            return;
        };
        let min = rect.min();
        let max = rect.max();
        let corners = [min, Vec2::new(max.x, min.y), max, Vec2::new(min.x, max.y)];
        let uvs = [
            [uv[0], uv[1]],
            [uv[2], uv[1]],
            [uv[2], uv[3]],
            [uv[0], uv[3]],
        ];
        for index in [0usize, 1, 2, 0, 2, 3] {
            self.push_vertex(corners[index], uvs[index], color);
        }
        self.finish(geometry, surface);
    }

    fn stroke_rect(&mut self, rect: Rect, width: f32, color: [f32; 4]) {
        if width <= 0.0 || rect.is_empty() {
            return;
        }
        let (left, top, right, bottom) = (rect.left(), rect.top(), rect.right(), rect.bottom());
        let inner_top = (top + width).min(bottom);
        let inner_bottom = (bottom - width).max(top);
        let inner_left = (left + width).min(right);
        let inner_right = (right - width).max(left);

        self.quad(
            Rect::from_min_max(Vec2::new(left, top), Vec2::new(right, inner_top)),
            SOLID_UV,
            color,
            Surface::Solid,
        );
        self.quad(
            Rect::from_min_max(Vec2::new(left, inner_bottom), Vec2::new(right, bottom)),
            SOLID_UV,
            color,
            Surface::Solid,
        );
        self.quad(
            Rect::from_min_max(
                Vec2::new(left, inner_top),
                Vec2::new(inner_left, inner_bottom),
            ),
            SOLID_UV,
            color,
            Surface::Solid,
        );
        self.quad(
            Rect::from_min_max(
                Vec2::new(inner_right, inner_top),
                Vec2::new(right, inner_bottom),
            ),
            SOLID_UV,
            color,
            Surface::Solid,
        );
    }

    fn fill_circle(&mut self, center: Vec2, radius: f32, color: [f32; 4]) {
        if radius <= 0.0 {
            return;
        }
        let Some(geometry) = self.begin() else {
            return;
        };
        let step = std::f32::consts::TAU / CIRCLE_SEGMENTS as f32;
        for segment in 0..CIRCLE_SEGMENTS {
            let a0 = step * segment as f32;
            let a1 = step * (segment + 1) as f32;
            self.push_vertex(center, [0.5, 0.5], color);
            self.push_vertex(circle_point(center, radius, a0), [0.5, 0.5], color);
            self.push_vertex(circle_point(center, radius, a1), [0.5, 0.5], color);
        }
        self.finish(geometry, Surface::Solid);
    }

    fn stroke_circle(&mut self, center: Vec2, radius: f32, width: f32, color: [f32; 4]) {
        if radius <= 0.0 || width <= 0.0 {
            return;
        }
        let Some(geometry) = self.begin() else {
            return;
        };
        let inner = (radius - width * 0.5).max(0.0);
        let outer = radius + width * 0.5;
        let step = std::f32::consts::TAU / CIRCLE_SEGMENTS as f32;
        for segment in 0..CIRCLE_SEGMENTS {
            let a0 = step * segment as f32;
            let a1 = step * (segment + 1) as f32;
            let i0 = circle_point(center, inner, a0);
            let i1 = circle_point(center, inner, a1);
            let o0 = circle_point(center, outer, a0);
            let o1 = circle_point(center, outer, a1);
            self.push_vertex(i0, [0.5, 0.5], color);
            self.push_vertex(o0, [0.5, 0.5], color);
            self.push_vertex(o1, [0.5, 0.5], color);
            self.push_vertex(i0, [0.5, 0.5], color);
            self.push_vertex(o1, [0.5, 0.5], color);
            self.push_vertex(i1, [0.5, 0.5], color);
        }
        self.finish(geometry, Surface::Solid);
    }

    fn image_uv(&self, texture: TextureId, source: Option<Rect>) -> Option<[f32; 4]> {
        let &(width, height) = self.texture_sizes.get(&texture)?;
        if width == 0 || height == 0 {
            return None;
        }
        let (width, height) = (width as f32, height as f32);
        let (sx, sy, sw, sh) = match source {
            Some(rect) => (rect.left(), rect.top(), rect.size.width, rect.size.height),
            None => (0.0, 0.0, width, height),
        };
        Some([
            sx / width,
            sy / height,
            (sx + sw) / width,
            (sy + sh) / height,
        ])
    }

    fn draw_text(
        &mut self,
        text: &str,
        position: Vec2,
        font_size: f32,
        align: TextAlign,
        color: [f32; 4],
    ) {
        if font_size <= 0.0 || text.is_empty() {
            return;
        }
        let advance = font_size;
        let count = text.chars().count() as f32;
        let total = advance * count;
        let start_x = match align {
            TextAlign::Left => position.x,
            TextAlign::Center => position.x - total * 0.5,
            TextAlign::Right => position.x - total,
        };
        // The glyph cell is `font_size` tall; place it so `position.y` is the
        // baseline (bottom of the cell), matching Canvas `fillText`.
        let top = position.y - font_size;
        for (index, ch) in text.chars().enumerate() {
            let uv = font::glyph_uv(ch);
            let rect = Rect::from_min_size(
                Vec2::new(start_x + advance * index as f32, top),
                Size::new(advance, advance),
            );
            self.quad(rect, uv, color, Surface::Font);
        }
    }
}

impl RenderBackend for WgpuBackend {
    type Error = WgpuError;

    fn begin_frame(&mut self, viewport: Viewport) -> Result<(), Self::Error> {
        let size = viewport.device_size(self.scale_factor);
        let width = size.width.round().max(1.0) as u32;
        let height = size.height.round().max(1.0) as u32;
        self.ensure_offscreen(width, height);
        self.ensure_pipeline(TARGET_FORMAT);
        self.start_frame(viewport, width, height)?;
        let view = self
            .offscreen
            .as_ref()
            .expect("offscreen target created above")
            .view
            .clone();
        self.frame = Some(Frame {
            view,
            width,
            height,
            format: TARGET_FORMAT,
            offscreen: true,
        });
        Ok(())
    }

    fn submit(&mut self, list: &DrawList) -> Result<(), Self::Error> {
        if !self.in_frame {
            return Err(WgpuError::NotInFrame);
        }
        for command in list.commands() {
            self.execute(command);
        }
        Ok(())
    }

    fn end_frame(&mut self) -> Result<(), Self::Error> {
        if !self.in_frame {
            return Err(WgpuError::NotInFrame);
        }
        self.in_frame = false;
        self.render_frame();
        Ok(())
    }
}

fn circle_point(center: Vec2, radius: f32, angle: f32) -> Vec2 {
    Vec2::new(
        center.x + angle.cos() * radius,
        center.y + angle.sin() * radius,
    )
}

fn create_render_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    bind_group_layout: &wgpu::BindGroupLayout,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("draw_backend_wgpu.pipeline_layout"),
        bind_group_layouts: &[bind_group_layout],
        push_constant_ranges: &[],
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("draw_backend_wgpu.pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[Vertex::layout()],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState {
                    color: wgpu::BlendComponent {
                        src_factor: wgpu::BlendFactor::SrcAlpha,
                        dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                        operation: wgpu::BlendOperation::Add,
                    },
                    alpha: wgpu::BlendComponent {
                        src_factor: wgpu::BlendFactor::One,
                        dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                        operation: wgpu::BlendOperation::Add,
                    },
                }),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview: None,
        cache: None,
    })
}

fn upload_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    label: &str,
    rgba: &[u8],
    width: u32,
    height: u32,
) -> wgpu::Texture {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        rgba,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width * 4),
            rows_per_image: Some(height),
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    texture
}

fn bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    view: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
    label: &str,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some(label),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
}
