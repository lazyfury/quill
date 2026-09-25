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
//!
//! # Layout
//!
//! This module owns the backend's types and public surface; the logic lives in
//! sibling modules so each file stays small:
//!
//! - [`init`] — adapter/device selection and initial GPU resources.
//! - [`frame`] — the offscreen target and the per-frame render pass.
//! - [`tessellate`] — turning `DrawCommand`s into vertices and draw ranges.
//! - [`pipeline`] — the render pipeline and texture/bind-group helpers.

mod frame;
mod init;
mod pipeline;
mod tessellate;

use std::collections::HashMap;
use std::fmt;
use std::ops::Range;
use std::rc::Rc;
use std::sync::mpsc;

use bytemuck::{Pod, Zeroable};

use draw_core::{Color, Rect, Transform2D, Vec2, ViewportSize};
use draw_render::{DrawList, Paint, RenderBackend, TextureId};

use crate::font::{Font, FontConfig, FontMetrics};
use pipeline::{bind_group, upload_texture};

/// Formats the offscreen render target uses.
const TARGET_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
/// Multisample count for the render target (geometry anti-aliasing).
const MSAA_SAMPLES: u32 = 4;
/// Circle tessellation resolution.
const CIRCLE_SEGMENTS: u32 = 64;
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
pub(super) struct Vertex {
    /// Normalized device coordinates.
    pub(super) position: [f32; 2],
    pub(super) uv: [f32; 2],
    pub(super) color: [f32; 4],
}

impl Vertex {
    pub(super) const ATTRIBUTES: [wgpu::VertexAttribute; 3] =
        wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x4];

    pub(super) fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: core::mem::size_of::<Self>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &Self::ATTRIBUTES,
        }
    }
}

/// How a registered texture is sampled when it is scaled.
///
/// The default is [`Linear`](Self::Linear) (smooth scaling, good for photos and
/// UI imagery). [`Nearest`](Self::Nearest) keeps hard texel edges, which is what
/// pixel-art / low-resolution canvases want when zoomed in. The filter is a
/// backend-side property of a [`TextureId`], not part of the neutral
/// `DrawImage` command, so the IR stays filter-free.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextureFilter {
    /// Bilinear magnification (the default).
    #[default]
    Linear,
    /// Nearest-texel magnification / minification.
    Nearest,
}

/// An optional post-process applied when a registered texture is drawn.
///
/// Like [`TextureFilter`], it is a backend-side property of a [`TextureId`],
/// not part of the neutral `DrawImage` command. It exists for low-resolution
/// canvases (a pixel-art emulator frame) that want a CRT/LCD look; the effect
/// is a fragment shader that reads the texture and the current `uv`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TextureEffect {
    /// A plain textured blit.
    #[default]
    None,
    /// Alternate source rows darkened — horizontal scanlines.
    Scanlines,
    /// Scanlines plus an aperture-grille colour mask.
    Crt,
    /// A dark grid between source pixels (an LCD pixel grid).
    Lcd,
    /// A 5-tap unsharp mask.
    Sharpen,
}

impl TextureEffect {
    /// The fragment entry point in `shader.wgsl` for this effect.
    pub(super) fn entry_point(self) -> &'static str {
        match self {
            TextureEffect::None => "fs_main",
            TextureEffect::Scanlines => "fs_scanlines",
            TextureEffect::Crt => "fs_crt",
            TextureEffect::Lcd => "fs_lcd",
            TextureEffect::Sharpen => "fs_sharpen",
        }
    }
}

/// Which texture a draw range samples.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Surface {
    /// The 1x1 white texture, tinted by the vertex color.
    Solid,
    /// A texture registered with [`WgpuBackend::register_texture`].
    Texture(TextureId),
    /// The built-in bitmap-font atlas.
    Font,
}

/// A contiguous vertex range plus the state it is drawn under.
#[derive(Debug, Clone)]
pub(super) struct DrawRange {
    pub(super) vertices: Range<u32>,
    pub(super) surface: Surface,
    /// `None` means "full target"; otherwise `[x, y, width, height]`.
    pub(super) scissor: Option<[u32; 4]>,
}

/// The clip after resolving the current state against the target.
#[derive(Debug, Clone, Copy)]
pub(super) enum ClipResult {
    Full,
    Scissor([u32; 4]),
    Empty,
}

/// CPU-side paint state mirroring `Save` / `Restore`.
#[derive(Debug, Clone, Copy)]
pub(super) struct State {
    pub(super) transform: Transform2D,
    pub(super) opacity: f32,
    pub(super) clip: Option<Rect>,
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
pub(super) struct OffscreenTarget {
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) texture: wgpu::Texture,
    pub(super) view: wgpu::TextureView,
}

/// A multisampled colour target that resolves into the frame's texture.
///
/// Kept per size/format and reused across frames; the requested `view` holds the
/// texture alive, so the texture handle itself does not need to be stored.
pub(super) struct MsaaTarget {
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) format: wgpu::TextureFormat,
    pub(super) view: wgpu::TextureView,
}

/// The render target of the frame currently being built.
///
/// Offscreen frames point at [`OffscreenTarget::view`]; window frames point at
/// the surface texture view supplied by the caller.
pub(super) struct Frame {
    pub(super) view: wgpu::TextureView,
    /// Multisampled colour attachment; resolves into [`Frame::view`].
    pub(super) msaa_view: wgpu::TextureView,
    pub(super) width: u32,
    pub(super) height: u32,
    pub(super) format: wgpu::TextureFormat,
    pub(super) offscreen: bool,
}

/// A `wgpu` [`RenderBackend`] rendering to an offscreen texture or a surface.
pub struct WgpuBackend {
    pub(super) instance: wgpu::Instance,
    pub(super) adapter: wgpu::Adapter,
    pub(super) device: wgpu::Device,
    pub(super) queue: wgpu::Queue,
    pub(super) shader: wgpu::ShaderModule,
    pub(super) bind_group_layout: wgpu::BindGroupLayout,
    /// One pipeline per color-target format (offscreen plus surface formats).
    pub(super) pipelines: HashMap<wgpu::TextureFormat, wgpu::RenderPipeline>,

    pub(super) white_bind_group: wgpu::BindGroup,
    pub(super) font: Rc<Font>,
    pub(super) font_config: FontConfig,
    pub(super) font_texture: wgpu::Texture,
    pub(super) font_bind_group: wgpu::BindGroup,
    pub(super) font_sampler: wgpu::Sampler,
    pub(super) image_sampler: wgpu::Sampler,
    /// Used for textures registered with [`TextureFilter::Nearest`].
    pub(super) nearest_sampler: wgpu::Sampler,
    pub(super) textures: HashMap<TextureId, wgpu::BindGroup>,
    pub(super) texture_sizes: HashMap<TextureId, (u32, u32)>,
    /// Per-texture sampling filter; absent means [`TextureFilter::Linear`].
    pub(super) texture_filters: HashMap<TextureId, TextureFilter>,
    /// Per-texture post-process; absent means [`TextureEffect::None`].
    pub(super) texture_effects: HashMap<TextureId, TextureEffect>,
    /// Effect pipelines, one per (effect, target format), built on demand.
    pub(super) effect_pipelines:
        HashMap<(TextureEffect, wgpu::TextureFormat), wgpu::RenderPipeline>,
    /// Kept so [`WgpuBackend::update_texture`] can rewrite pixels in place
    /// instead of allocating a new GPU texture every frame.
    pub(super) texture_objects: HashMap<TextureId, wgpu::Texture>,

    pub(super) offscreen: Option<OffscreenTarget>,
    pub(super) msaa: Option<MsaaTarget>,
    pub(super) frame: Option<Frame>,

    // Per-frame CPU staging.
    pub(super) in_frame: bool,
    pub(super) viewport: ViewportSize,
    pub(super) scale_factor: f32,
    pub(super) clear: Color,
    pub(super) device_width: f32,
    pub(super) device_height: f32,
    pub(super) state: State,
    pub(super) stack: Vec<State>,
    pub(super) vertices: Vec<Vertex>,
    pub(super) ranges: Vec<DrawRange>,
}

impl WgpuBackend {
    /// Sets the logical-to-device scale factor (DPR). Core/IR stay logical.
    pub fn set_scale_factor(&mut self, scale_factor: f32) {
        let scale = scale_factor.max(0.0);
        self.scale_factor = scale;
        self.font.set_scale(scale);
    }

    /// Returns the loaded font's metrics.
    ///
    /// Hosts wrap this in a `draw_ui::TextMeasurer` so layout measures text
    /// with the same advances the backend renders with.
    pub fn text_metrics(&self) -> FontMetrics {
        FontMetrics::new(self.font.clone())
    }

    /// Returns the current font configuration.
    pub fn font_config(&self) -> FontConfig {
        self.font_config.clone()
    }

    /// Switches the font mode / HiDPI rasterization and rebuilds the atlas.
    ///
    /// Call this before the first frame (or between frames). After switching,
    /// fetch [`WgpuBackend::text_metrics`] again for the new metrics.
    pub fn set_font_config(&mut self, config: FontConfig) -> Result<(), WgpuError> {
        self.font_config = config;
        self.rebuild_font()
    }

    fn rebuild_font(&mut self) -> Result<(), WgpuError> {
        let font = Rc::new(Font::load_with(self.font_config.clone()));
        font.set_scale(self.scale_factor);
        let (width, height) = font.atlas_size();
        let texture = upload_texture(
            &self.device,
            &self.queue,
            "draw_backend_wgpu.font_atlas",
            &font.initial_atlas(),
            width,
            height,
        );
        let view = texture.create_view(&Default::default());
        let group = bind_group(
            &self.device,
            &self.bind_group_layout,
            &view,
            &self.font_sampler,
            "font_atlas",
        );
        self.font = font;
        self.font_texture = texture;
        self.font_bind_group = group;
        Ok(())
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
    pub fn viewport(&self) -> ViewportSize {
        self.viewport
    }

    /// Sets the color the render target is cleared to each frame.
    pub fn set_clear_color(&mut self, color: Color) {
        self.clear = color;
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
        let filter = self.filter_for(id);
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
            self.sampler(filter),
            "texture",
        );
        self.textures.insert(id, group);
        self.texture_sizes.insert(id, (width, height));
        self.texture_objects.insert(id, texture);
        Ok(())
    }

    /// Registers an image and remembers a non-default sampling [`TextureFilter`].
    ///
    /// Equivalent to [`set_texture_filter`](Self::set_texture_filter) followed by
    /// [`register_texture`](Self::register_texture).
    pub fn register_texture_with_filter(
        &mut self,
        id: TextureId,
        width: u32,
        height: u32,
        rgba: &[u8],
        filter: TextureFilter,
    ) -> Result<(), WgpuError> {
        self.texture_filters.insert(id, filter);
        self.register_texture(id, width, height, rgba)
    }

    /// Sets how an image texture is sampled when scaled.
    ///
    /// Call before registering the texture, or afterwards: an already-registered
    /// texture gets its bind group rebuilt so the change takes effect on the next
    /// frame. Returns nothing when the filter is unchanged.
    pub fn set_texture_filter(&mut self, id: TextureId, filter: TextureFilter) {
        if self.filter_for(id) == filter {
            return;
        }
        self.texture_filters.insert(id, filter);
        let rebuilt = self.texture_objects.get(&id).map(|texture| {
            let view = texture.create_view(&Default::default());
            bind_group(
                &self.device,
                &self.bind_group_layout,
                &view,
                self.sampler(filter),
                "texture",
            )
        });
        if let Some(group) = rebuilt {
            self.textures.insert(id, group);
        }
    }

    /// The filter recorded for `id` (`Linear` when none was set).
    fn filter_for(&self, id: TextureId) -> TextureFilter {
        self.texture_filters.get(&id).copied().unwrap_or_default()
    }

    /// Sets the post-process applied when `id` is drawn. `None` clears it.
    pub fn set_texture_effect(&mut self, id: TextureId, effect: TextureEffect) {
        if effect == TextureEffect::None {
            self.texture_effects.remove(&id);
        } else {
            self.texture_effects.insert(id, effect);
        }
    }

    /// The effect recorded for `id` (`None` when none was set).
    pub(super) fn effect_for(&self, id: TextureId) -> TextureEffect {
        self.texture_effects.get(&id).copied().unwrap_or_default()
    }

    /// Ensures an effect pipeline exists for `effect`/`format`.
    pub(super) fn ensure_effect_pipeline(
        &mut self,
        format: wgpu::TextureFormat,
        effect: TextureEffect,
    ) {
        if self.effect_pipelines.contains_key(&(effect, format)) {
            return;
        }
        let pipeline = pipeline::create_render_pipeline(
            &self.device,
            &self.shader,
            &self.bind_group_layout,
            format,
            effect.entry_point(),
        );
        self.effect_pipelines.insert((effect, format), pipeline);
    }

    /// The GPU sampler for a filter.
    fn sampler(&self, filter: TextureFilter) -> &wgpu::Sampler {
        match filter {
            TextureFilter::Linear => &self.image_sampler,
            TextureFilter::Nearest => &self.nearest_sampler,
        }
    }

    /// Uploads new pixels for a texture, reusing the GPU texture and its bind
    /// group when the size has not changed.
    ///
    /// [`register_texture`](Self::register_texture) allocates a fresh texture,
    /// view and bind group every call. A host that changes an image repeatedly
    /// (a painting canvas, a live preview) should call this instead, so the only
    /// per-update GPU work is the pixel copy into the existing texture.
    pub fn update_texture(
        &mut self,
        id: TextureId,
        width: u32,
        height: u32,
        rgba: &[u8],
    ) -> Result<(), WgpuError> {
        let Some(expected) = (width as usize)
            .checked_mul(height as usize)
            .and_then(|n| n.checked_mul(4))
        else {
            return self.register_texture(id, width, height, rgba);
        };
        if width == 0 || height == 0 || rgba.len() < expected {
            return self.register_texture(id, width, height, rgba);
        }
        if self.texture_sizes.get(&id) != Some(&(width, height)) {
            return self.register_texture(id, width, height, rgba);
        }
        let Some(texture) = self.texture_objects.get(&id) else {
            return self.register_texture(id, width, height, rgba);
        };
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &rgba[..expected],
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
        Ok(())
    }

    /// Uploads any glyphs rasterized while processing the last `submit`.
    fn upload_pending_font_glyphs(&mut self) {
        let Some(pixels) = self.font.take_dirty_atlas() else {
            return;
        };
        let (width, height) = self.font.atlas_size();
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.font_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &pixels,
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
}

impl RenderBackend for WgpuBackend {
    type Error = WgpuError;

    fn begin_frame(&mut self, viewport: ViewportSize) -> Result<(), Self::Error> {
        let size = viewport.device_size(self.scale_factor);
        let width = size.width.round().max(1.0) as u32;
        let height = size.height.round().max(1.0) as u32;
        self.ensure_offscreen(width, height);
        self.ensure_msaa(width, height, TARGET_FORMAT);
        self.ensure_pipeline(TARGET_FORMAT);
        self.start_frame(viewport, width, height)?;
        let view = self
            .offscreen
            .as_ref()
            .expect("offscreen target created above")
            .view
            .clone();
        let msaa_view = self
            .msaa
            .as_ref()
            .expect("msaa target created above")
            .view
            .clone();
        self.frame = Some(Frame {
            view,
            msaa_view,
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
        self.upload_pending_font_glyphs();
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

    /// Delegates to the inherent [`WgpuBackend::register_texture`] so the
    /// neutral registration contract uploads a real GPU texture.
    fn register_texture(
        &mut self,
        id: TextureId,
        width: u32,
        height: u32,
        rgba: &[u8],
    ) -> Result<(), Self::Error> {
        WgpuBackend::register_texture(self, id, width, height, rgba)
    }
}
