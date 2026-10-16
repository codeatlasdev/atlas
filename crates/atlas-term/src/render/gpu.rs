//! The process-wide wgpu device, the GPU-side structs shared with `term.wgsl`, the growable
//! vertex buffer, and the bind-group/pipeline builders used by both `Renderer::new` and the
//! shader-validation test (one definition of the layout, so the test checks the real thing).

use std::sync::LazyLock;

use crate::font::GlyphInst;

pub(super) struct Gpu {
    pub instance: wgpu::Instance,
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
}

/// One device for every terminal in the process.
static GPU: LazyLock<Result<Gpu, String>> = LazyLock::new(|| {
    // Only the Metal backend is compiled in: elsewhere (Linux CI, `cargo test --workspace`)
    // `Instance::new` would panic instead of reporting that there is no adapter.
    if wgpu::Instance::enabled_backend_features().is_empty() {
        return Err("no wgpu backend for this platform (the GPU terminal is macOS only)".into());
    }
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::PRIMARY,
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    });
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::LowPower,
        ..Default::default()
    }))
    .map_err(|e| e.to_string())?;
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("atlas-term"),
        ..Default::default()
    }))
    .map_err(|e| e.to_string())?;
    Ok(Gpu {
        instance,
        adapter,
        device,
        queue,
    })
});

pub(super) fn gpu() -> Result<&'static Gpu, String> {
    GPU.as_ref().map_err(Clone::clone)
}

/// The surface size wgpu accepts for a requested pixel size: `Surface::configure` panics on a zero side and on one
/// past the device's `max_texture_dimension_2d`, so every configure goes through here.
pub(super) fn surface_extent(width: u32, height: u32, max_side: u32) -> (u32, u32) {
    let side = |v: u32| v.min(max_side).max(1);
    (side(width), side(height))
}

/// Mirrors the WGSL `U` block (2×vec2 + vec2 + vec2 + u32, 40 bytes).
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(super) struct Uniforms {
    pub cell: [f32; 2],
    pub viewport: [f32; 2],
    pub atlas: [f32; 2],
    pub color_atlas: [f32; 2],
    pub cols: u32,
    pub _pad: u32,
}

/// One decoration bar/wave per cell: position in pixels, thickness, kind, color.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(super) struct DecoInst {
    /// Pixel top-left (not snapped; renderer snaps to nearest px).
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub color: [u8; 4],
    /// 0=ul, 1=double-ul, 2=undercurl, 3=dotted, 4=dashed, 5=strikethrough.
    pub kind: u32,
}

/// One image quad: pixel rect and normalized source rect.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(super) struct ImgInst {
    pub pos: [f32; 2],
    pub size: [f32; 2],
    pub uv0: [f32; 2],
    pub uv1: [f32; 2],
}

/// Growable vertex buffer.
pub(super) struct VBuf {
    pub buf: wgpu::Buffer,
    cap: u64,
    label: &'static str,
}

impl VBuf {
    pub fn new(d: &wgpu::Device, label: &'static str) -> Self {
        let cap = 64 * 1024;
        Self {
            buf: Self::alloc(d, label, cap),
            cap,
            label,
        }
    }

    fn alloc(d: &wgpu::Device, label: &'static str, size: u64) -> wgpu::Buffer {
        d.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    pub fn write(&mut self, g: &Gpu, bytes: &[u8]) {
        if bytes.len() as u64 > self.cap {
            self.cap = (bytes.len() as u64).next_power_of_two();
            self.buf = Self::alloc(&g.device, self.label, self.cap);
        }
        g.queue.write_buffer(&self.buf, 0, bytes);
    }
}

/// A texture the glyph/image shaders can sample (`R8`/`Rgba8` atlas or an inline image).
pub(super) fn texture(
    d: &wgpu::Device,
    label: &str,
    (width, height): (u32, u32),
    format: wgpu::TextureFormat,
) -> wgpu::Texture {
    d.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

/// Bindings 0 (uniforms), 1 (texture), 2 (sampler): shared by every pipeline.
/// `min_uniform_size` lets the validation test pin the block size.
pub(super) fn bind_layout(
    d: &wgpu::Device,
    min_uniform_size: Option<wgpu::BufferSize>,
) -> wgpu::BindGroupLayout {
    let entry = |binding, visibility, ty| wgpu::BindGroupLayoutEntry {
        binding,
        visibility,
        ty,
        count: None,
    };
    d.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &[
            entry(
                0,
                wgpu::ShaderStages::VERTEX,
                wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: min_uniform_size,
                },
            ),
            entry(
                1,
                wgpu::ShaderStages::FRAGMENT,
                wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
            ),
            entry(
                2,
                wgpu::ShaderStages::FRAGMENT,
                wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            ),
        ],
    })
}

/// The uniforms plus one texture view and sampler, in `bind_layout`'s order.
pub(super) fn bind_group(
    d: &wgpu::Device,
    label: Option<&str>,
    layout: &wgpu::BindGroupLayout,
    uniforms: &wgpu::Buffer,
    view: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    d.create_bind_group(&wgpu::BindGroupDescriptor {
        label,
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: uniforms.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
        ],
    })
}

const GLYPH_ATTRS: [wgpu::VertexAttribute; 4] = wgpu::vertex_attr_array![
    0 => Float32x2, 1 => Float32x2, 2 => Float32x2, 3 => Unorm8x4
];

/// Per-instance layout of a `GlyphInst` (mono and color glyph pipelines).
pub(super) fn glyph_vertex_layout() -> wgpu::VertexBufferLayout<'static> {
    wgpu::VertexBufferLayout {
        array_stride: size_of::<GlyphInst>() as u64,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &GLYPH_ATTRS,
    }
}

/// An instanced triangle-strip pipeline from `term.wgsl` into a `format` target.
pub(super) fn pipeline(
    d: &wgpu::Device,
    shader: &wgpu::ShaderModule,
    layout: &wgpu::PipelineLayout,
    format: wgpu::TextureFormat,
    label: &str,
    (vs, fs): (&str, &str),
    buffer: wgpu::VertexBufferLayout<'_>,
) -> wgpu::RenderPipeline {
    d.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some(vs),
            compilation_options: Default::default(),
            buffers: &[Some(buffer)],
        },
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some(fs),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleStrip,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: Default::default(),
        multiview_mask: None,
        cache: None,
    })
}
