//! Frame lifecycle: the offscreen target, pipeline cache and render pass.

use super::*;
use pipeline::create_render_pipeline;
use wgpu::util::DeviceExt;

impl WgpuBackend {
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
        viewport: ViewportSize,
    ) -> Result<(), WgpuError> {
        let width = width.max(1);
        let height = height.max(1);
        self.ensure_pipeline(format);
        self.ensure_msaa(width, height, format);
        self.start_frame(viewport, width, height)?;
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
    pub(super) fn ensure_pipeline(&mut self, format: wgpu::TextureFormat) {
        if self.pipelines.contains_key(&format) {
            return;
        }
        let pipeline =
            create_render_pipeline(&self.device, &self.shader, &self.bind_group_layout, format);
        self.pipelines.insert(format, pipeline);
    }

    /// Resets per-frame staging and validates the lifecycle.
    pub(super) fn start_frame(
        &mut self,
        viewport: ViewportSize,
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

    pub(super) fn ensure_offscreen(&mut self, width: u32, height: u32) {
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

    /// Ensures a multisampled colour target exists for `width`x`height`/`format`.
    pub(super) fn ensure_msaa(&mut self, width: u32, height: u32, format: wgpu::TextureFormat) {
        if matches!(
            &self.msaa,
            Some(target)
                if target.width == width && target.height == height && target.format == format
        ) {
            return;
        }
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("draw_backend_wgpu.msaa"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: MSAA_SAMPLES,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        self.msaa = Some(MsaaTarget {
            width,
            height,
            format,
            view,
        });
    }

    /// Encodes and submits the render pass for the current frame.
    pub(super) fn render_frame(&self) {
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
                    view: &frame.msaa_view,
                    resolve_target: Some(&frame.view),
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(clear),
                        store: wgpu::StoreOp::Discard,
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
}
