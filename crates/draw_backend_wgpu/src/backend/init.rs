//! Backend construction: adapter/device selection and initial GPU resources.
//!
//! Kept separate from [`super`] so the public error/type surface in `mod.rs`
//! stays readable while the long `wgpu::DeviceDescriptor` boilerplate lives
//! here.

use super::*;
use std::rc::Rc;

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
            source: wgpu::ShaderSource::Wgsl(crate::shader::SHADER.into()),
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
        let nearest_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("draw_backend_wgpu.nearest_sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
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

        let font = Rc::new(Font::load_with(FontConfig::default()));
        let font_texture = {
            let (width, height) = font.atlas_size();
            upload_texture(
                &device,
                &queue,
                "draw_backend_wgpu.font_atlas",
                &font.initial_atlas(),
                width,
                height,
            )
        };
        let font_bind_group = {
            let view = font_texture.create_view(&Default::default());
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
            font,
            font_config: FontConfig::default(),
            font_texture,
            font_bind_group,
            font_sampler,
            image_sampler,
            nearest_sampler,
            textures: HashMap::new(),
            texture_sizes: HashMap::new(),
            texture_objects: HashMap::new(),
            render_targets: HashMap::new(),
            texture_filters: HashMap::new(),
            texture_effects: HashMap::new(),
            effect_pipelines: HashMap::new(),
            offscreen: None,
            msaa: None,
            frame: None,
            in_frame: false,
            viewport: ViewportSize::default(),
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
}
