//! Surface + instanced pipelines per frame: cell backgrounds, images below text
//! (z < 0), glyphs from the font atlas, images above text, decorations. The frame
//! is a `vt::Frame` snapshot, so the VT lock is never held while drawing.

use std::collections::HashMap;
use std::ffi::c_void;
use std::ptr::NonNull;
use std::sync::LazyLock;
use std::time::Instant;

use crate::font::{ATLAS, Atlas, COLOR_ATLAS, Font, GlyphInst, TextLayout};
use crate::vt::Frame;

struct Gpu {
    instance: wgpu::Instance,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
}

/// One device for every terminal in the process.
static GPU: LazyLock<Result<Gpu, String>> = LazyLock::new(|| {
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

fn gpu() -> Result<&'static Gpu, String> {
    GPU.as_ref().map_err(Clone::clone)
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    cell: [f32; 2],
    viewport: [f32; 2],
    atlas: [f32; 2],
    color_atlas: [f32; 2],
    cols: u32,
    _pad: u32,
}

/// One decoration bar/wave per cell: position in pixels, thickness, kind, color.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct DecoInst {
    /// Pixel top-left (not snapped; renderer snaps to nearest px).
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    color: [u8; 4],
    /// 0=ul, 1=double-ul, 2=undercurl, 3=dotted, 4=dashed, 5=strikethrough.
    kind: u32,
}

/// One image quad: pixel rect and normalized source rect.
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct ImgInst {
    pos: [f32; 2],
    size: [f32; 2],
    uv0: [f32; 2],
    uv1: [f32; 2],
}

/// Uploaded image: re-uploaded when the image's generation changes.
struct Tex {
    generation: u64,
    bind: wgpu::BindGroup,
    last_frame: u64,
}

/// Textures kept for images not on screen (scrolling back re-shows them).
const TEX_CACHE: usize = 32;

/// Frame timing. `work` = snapshot upload + encode + submit + present after the
/// drawable is acquired; `interval` = time between presents (vsync-bound under load).
#[derive(Clone, Copy, Debug, Default)]
pub struct Stats {
    pub frames: u64,
    pub work_p50_us: u32,
    pub work_p95_us: u32,
    pub work_max_us: u32,
    pub interval_p50_us: u32,
    pub interval_p95_us: u32,
}

/// Most recent samples kept for percentiles.
const WINDOW: usize = 4096;

fn pct(v: &[u32], p: usize) -> u32 {
    if v.is_empty() {
        return 0;
    }
    let mut s = v.to_vec();
    s.sort_unstable();
    s[(s.len() - 1) * p / 100]
}

/// Growable vertex buffer.
struct VBuf {
    buf: wgpu::Buffer,
    cap: u64,
    label: &'static str,
}

impl VBuf {
    fn new(d: &wgpu::Device, label: &'static str) -> Self {
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

    fn write(&mut self, g: &Gpu, bytes: &[u8]) {
        if bytes.len() as u64 > self.cap {
            self.cap = (bytes.len() as u64).next_power_of_two();
            self.buf = Self::alloc(&g.device, self.label, self.cap);
        }
        g.queue.write_buffer(&self.buf, 0, bytes);
    }
}

pub struct Renderer {
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    bg_pipe: wgpu::RenderPipeline,
    glyph_pipe: wgpu::RenderPipeline,
    deco_pipe: wgpu::RenderPipeline,
    img_pipe: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    linear: wgpu::Sampler,
    imgs: VBuf,
    img_insts: Vec<ImgInst>,
    textures: HashMap<u32, Tex>,
    uniforms: wgpu::Buffer,
    bind: wgpu::BindGroup,
    /// Same layout as `bind` but sampling the RGBA emoji atlas.
    color_bind: wgpu::BindGroup,
    atlas: wgpu::Texture,
    color_atlas: wgpu::Texture,
    color_pipe: wgpu::RenderPipeline,
    cells: VBuf,
    glyphs: VBuf,
    color_glyphs: VBuf,
    decos: VBuf,
    deco_insts: Vec<DecoInst>,
    text: TextLayout,
    /// Backing scale factor (physical px per logical px).
    scale: f32,
    frames: u64,
    work: Vec<u32>,
    interval: Vec<u32>,
    last_present: Option<Instant>,
}

impl Renderer {
    /// `font_size` in points, `scale` = backing scale factor, size in physical pixels.
    ///
    /// # Safety
    /// `ns_view` is a live `NSView*` that outlives the renderer. Call on the main thread;
    /// `render` may then run on any thread.
    pub unsafe fn new(
        ns_view: NonNull<c_void>,
        width: u32,
        height: u32,
        font_size: f32,
        scale: f32,
    ) -> Result<Self, String> {
        let g = gpu()?;
        let handle = raw_window_handle::AppKitWindowHandle::new(ns_view);
        // SAFETY: the caller guarantees the view is valid and outlives the surface.
        let surface = unsafe {
            g.instance
                .create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
                    raw_display_handle: Some(raw_window_handle::RawDisplayHandle::AppKit(
                        raw_window_handle::AppKitDisplayHandle::new(),
                    )),
                    raw_window_handle: raw_window_handle::RawWindowHandle::AppKit(handle),
                })
        }
        .map_err(|e| e.to_string())?;
        let caps = surface.get_capabilities(&g.adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            // Non-sRGB target: cell colors are sRGB bytes (CSS theme values), written as-is.
            .find(|f| *f == wgpu::TextureFormat::Bgra8Unorm)
            .unwrap_or(caps.formats[0]);
        let mut config = surface
            .get_default_config(&g.adapter, width.max(1), height.max(1))
            .ok_or("surface unsupported by adapter")?;
        config.format = format;
        config.present_mode = wgpu::PresentMode::Fifo;
        config.desired_maximum_frame_latency = 2;
        config.alpha_mode = wgpu::CompositeAlphaMode::Opaque;
        surface.configure(&g.device, &config);

        let d = &g.device;
        let shader = d.create_shader_module(wgpu::include_wgsl!("term.wgsl"));
        let uniforms = d.create_buffer(&wgpu::BufferDescriptor {
            label: Some("uniforms"),
            size: size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let atlas = d.create_texture(&wgpu::TextureDescriptor {
            label: Some("glyph atlas"),
            size: wgpu::Extent3d {
                width: ATLAS,
                height: ATLAS,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let color_atlas = d.create_texture(&wgpu::TextureDescriptor {
            label: Some("color glyph atlas"),
            size: wgpu::Extent3d {
                width: COLOR_ATLAS,
                height: COLOR_ATLAS,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let sampler = d.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let entry = |binding, visibility, ty| wgpu::BindGroupLayoutEntry {
            binding,
            visibility,
            ty,
            count: None,
        };
        let layout = d.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                entry(
                    0,
                    wgpu::ShaderStages::VERTEX,
                    wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
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
        });
        let atlas_view = atlas.create_view(&Default::default());
        let bind = d.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniforms.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&atlas_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        let color_view = color_atlas.create_view(&Default::default());
        let color_bind = d.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniforms.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&color_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        let pl = d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipe = |label, vs, fs, buffer: wgpu::VertexBufferLayout| {
            d.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&pl),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some(vs),
                    compilation_options: Default::default(),
                    buffers: &[Some(buffer)],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
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
        };
        let bg_pipe = pipe(
            "bg",
            "bg_vs",
            "bg_fs",
            wgpu::VertexBufferLayout {
                array_stride: 4,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &wgpu::vertex_attr_array![0 => Unorm8x4],
            },
        );
        const GLYPH_ATTRS: [wgpu::VertexAttribute; 4] = wgpu::vertex_attr_array![
            0 => Float32x2, 1 => Float32x2, 2 => Float32x2, 3 => Unorm8x4
        ];
        let glyph_layout = || wgpu::VertexBufferLayout {
            array_stride: size_of::<GlyphInst>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &GLYPH_ATTRS,
        };
        let glyph_pipe = pipe("glyph", "glyph_vs", "glyph_fs", glyph_layout());
        let color_pipe = pipe(
            "color glyph",
            "color_glyph_vs",
            "color_glyph_fs",
            glyph_layout(),
        );
        let deco_pipe = pipe(
            "deco",
            "deco_vs",
            "deco_fs",
            wgpu::VertexBufferLayout {
                array_stride: size_of::<DecoInst>() as u64,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &wgpu::vertex_attr_array![
                    0 => Float32x2, 1 => Float32x2, 2 => Unorm8x4, 3 => Uint32
                ],
            },
        );
        let img_pipe = pipe(
            "img",
            "img_vs",
            "img_fs",
            wgpu::VertexBufferLayout {
                array_stride: size_of::<ImgInst>() as u64,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &wgpu::vertex_attr_array![
                    0 => Float32x2, 1 => Float32x2, 2 => Float32x2, 3 => Float32x2
                ],
            },
        );
        let linear = d.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Ok(Self {
            surface,
            config,
            bg_pipe,
            glyph_pipe,
            deco_pipe,
            img_pipe,
            layout,
            linear,
            imgs: VBuf::new(d, "images"),
            img_insts: Vec::new(),
            textures: HashMap::new(),
            uniforms,
            bind,
            color_bind,
            atlas,
            color_atlas,
            color_pipe,
            cells: VBuf::new(d, "cells"),
            glyphs: VBuf::new(d, "glyphs"),
            color_glyphs: VBuf::new(d, "color glyphs"),
            decos: VBuf::new(d, "decos"),
            deco_insts: Vec::new(),
            text: TextLayout::new(Font::new(font_size, scale)),
            scale,
            frames: 0,
            work: Vec::with_capacity(WINDOW),
            interval: Vec::with_capacity(WINDOW),
            last_present: None,
        })
    }

    /// Physical pixels. Returns the grid size that fits.
    pub fn resize(&mut self, width: u32, height: u32) -> (u16, u16) {
        let (w, h) = (width.max(1), height.max(1));
        if (w, h) != (self.config.width, self.config.height) {
            self.config.width = w;
            self.config.height = h;
            if let Ok(g) = gpu() {
                self.surface.configure(&g.device, &self.config);
            }
        }
        let c = self.text.font.cell;
        (
            (w as f32 / c[0]).floor().clamp(1.0, u16::MAX as f32) as u16,
            (h as f32 / c[1]).floor().clamp(1.0, u16::MAX as f32) as u16,
        )
    }

    /// Cell size in physical pixels.
    pub fn cell_px(&self) -> [f32; 2] {
        self.text.font.cell
    }

    pub fn stats(&self) -> Stats {
        Stats {
            frames: self.frames,
            work_p50_us: pct(&self.work, 50),
            work_p95_us: pct(&self.work, 95),
            work_max_us: self.work.iter().copied().max().unwrap_or(0),
            interval_p50_us: pct(&self.interval, 50),
            interval_p95_us: pct(&self.interval, 95),
        }
    }

    /// Reset the measurement window.
    pub fn reset_stats(&mut self) {
        self.frames = 0;
        self.work.clear();
        self.interval.clear();
        self.last_present = None;
    }

    fn upload_atlases(&mut self, g: &Gpu) {
        fn upload(g: &Gpu, tex: &wgpu::Texture, a: &mut Atlas) {
            let Some((y0, y1)) = a.dirty.take() else {
                return;
            };
            let row = a.side * a.bpp;
            g.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: tex,
                    mip_level: 0,
                    origin: wgpu::Origin3d { x: 0, y: y0, z: 0 },
                    aspect: wgpu::TextureAspect::All,
                },
                &a.pixels[(y0 * row) as usize..(y1 * row) as usize],
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row),
                    rows_per_image: None,
                },
                wgpu::Extent3d {
                    width: a.side,
                    height: y1 - y0,
                    depth_or_array_layers: 1,
                },
            );
        }
        upload(g, &self.atlas, &mut self.text.font.gray);
        upload(g, &self.color_atlas, &mut self.text.font.color);
    }

    /// Rebuild the font at `pt` points (same backing scale) and return the grid that
    /// fits the current pixel size. Both atlases and every cached row are dropped.
    pub fn set_font_size(&mut self, pt: f32) -> (u16, u16) {
        let ligatures = self.text.ligatures();
        self.text = TextLayout::new(Font::new(pt, self.scale));
        self.text.set_ligatures(ligatures);
        // The fresh atlases are blank: upload them over the old textures.
        self.text.font.gray.dirty = Some((0, ATLAS));
        self.text.font.color.dirty = Some((0, COLOR_ATLAS));
        self.resize(self.config.width, self.config.height)
    }

    /// Enable/disable `calt`/`liga` ligature shaping (default on); redraws shape anew.
    pub fn set_ligatures(&mut self, on: bool) {
        self.text.set_ligatures(on);
    }

    pub fn ligatures(&self) -> bool {
        self.text.ligatures()
    }

    /// Draw a snapshot (blocks on vsync when the swapchain is full).
    pub fn render(&mut self, f: &Frame) -> Result<(), String> {
        let g = gpu()?;
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t)
            | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&g.device, &self.config);
                return Ok(());
            }
            _ => return Ok(()),
        };
        let t0 = Instant::now();
        let [cw, ch] = self.text.font.cell;
        let base = self.text.font.baseline;
        let (ul_th, ul_off) = self.text.font.underline;
        let (st_th, st_off) = self.text.font.strikethrough;
        self.text.layout(f);
        self.deco_insts.clear();
        for d in &f.decos {
            use crate::vt::DecoKind::*;
            let row_top = d.row as f32 * ch;
            let (y, h) = match d.kind {
                DoubleUnderline => (base + ul_off, ul_th), // drawn twice below
                Undercurl | DottedUnderline | DashedUnderline | Underline => (base + ul_off, ul_th),
                Strikethrough => (base - st_off, st_th),
            };
            self.deco_insts.push(DecoInst {
                x: d.col as f32 * cw,
                y: row_top + y,
                w: cw,
                h,
                color: d.color,
                kind: d.kind as u32,
            });
            // Double underline: second line 2px below the first.
            if d.kind == DoubleUnderline {
                self.deco_insts.push(DecoInst {
                    x: d.col as f32 * cw,
                    y: row_top + y + ul_th * 2.0 + 1.0,
                    w: cw,
                    h,
                    color: d.color,
                    kind: d.kind as u32,
                });
            }
        }
        if let Some(c) = f.cursor {
            use crate::vt::CursorBars::*;
            let (x, y) = (c.col as f32 * cw, c.row as f32 * ch);
            let t = (1.5 * self.scale).round().max(1.0);
            let mut bar = |x, y, w, h| {
                self.deco_insts.push(DecoInst {
                    x,
                    y,
                    w,
                    h,
                    color: c.color,
                    kind: 0,
                })
            };
            match c.bars {
                Beam => bar(x, y, t, ch),
                Underline => bar(x, y + ch - t, cw, t),
                Hollow => {
                    bar(x, y, cw, t);
                    bar(x, y + ch - t, cw, t);
                    bar(x, y, t, ch);
                    bar(x + cw - t, y, t, ch);
                }
            }
        }
        // Scrollbar thumb: right edge, height ∝ visible/total, top = history position.
        if f.total_lines > f.rows && f.rows > 0 {
            let max_off = (f.total_lines - f.rows) as f32;
            let vp_h = self.config.height as f32;
            let thumb_h = (vp_h * f.rows as f32 / f.total_lines as f32).max(20.0);
            let thumb_y = (vp_h - thumb_h) * (1.0 - f.display_offset as f32 / max_off);
            let w = 4.0 * self.scale;
            self.deco_insts.push(DecoInst {
                x: self.config.width as f32 - w,
                y: thumb_y,
                w,
                h: thumb_h,
                color: [0x88, 0x88, 0x88, 0x80],
                kind: 0,
            });
        }
        self.upload_atlases(g);
        self.upload_images(g, f);
        self.imgs.write(g, bytemuck::cast_slice(&self.img_insts));
        self.cells.write(g, bytemuck::cast_slice(&f.bg));
        self.glyphs.write(g, bytemuck::cast_slice(&self.text.mono));
        self.color_glyphs
            .write(g, bytemuck::cast_slice(&self.text.color));
        self.decos.write(g, bytemuck::cast_slice(&self.deco_insts));
        g.queue.write_buffer(
            &self.uniforms,
            0,
            bytemuck::bytes_of(&Uniforms {
                cell: self.text.font.cell,
                viewport: [self.config.width as f32, self.config.height as f32],
                atlas: [ATLAS as f32; 2],
                color_atlas: [COLOR_ATLAS as f32; 2],
                cols: f.cols.max(1),
                _pad: 0,
            }),
        );
        let view = frame.texture.create_view(&Default::default());
        let mut enc = g.device.create_command_encoder(&Default::default());
        {
            let c = |v: u8| v as f64 / 255.0;
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: c(f.clear[0]),
                            g: c(f.clear[1]),
                            b: c(f.clear[2]),
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_bind_group(0, &self.bind, &[]);
            if !f.bg.is_empty() {
                pass.set_pipeline(&self.bg_pipe);
                pass.set_vertex_buffer(0, self.cells.buf.slice(..(f.bg.len() * 4) as u64));
                pass.draw(0..4, 0..f.bg.len() as u32);
            }
            // Images: z < 0 under the text, the rest over it.
            let split = f
                .images
                .iter()
                .position(|i| i.z >= 0)
                .unwrap_or(f.images.len());
            self.draw_images(&mut pass, f, 0..split);
            if !self.text.mono.is_empty() {
                pass.set_pipeline(&self.glyph_pipe);
                let n = (self.text.mono.len() * size_of::<GlyphInst>()) as u64;
                pass.set_vertex_buffer(0, self.glyphs.buf.slice(..n));
                pass.draw(0..4, 0..self.text.mono.len() as u32);
            }
            if !self.text.color.is_empty() {
                pass.set_pipeline(&self.color_pipe);
                pass.set_bind_group(0, &self.color_bind, &[]);
                let n = (self.text.color.len() * size_of::<GlyphInst>()) as u64;
                pass.set_vertex_buffer(0, self.color_glyphs.buf.slice(..n));
                pass.draw(0..4, 0..self.text.color.len() as u32);
                pass.set_bind_group(0, &self.bind, &[]);
            }
            self.draw_images(&mut pass, f, split..f.images.len());
            if !self.deco_insts.is_empty() {
                pass.set_pipeline(&self.deco_pipe);
                let n = (self.deco_insts.len() * size_of::<DecoInst>()) as u64;
                pass.set_vertex_buffer(0, self.decos.buf.slice(..n));
                pass.draw(0..4, 0..self.deco_insts.len() as u32);
            }
        }
        g.queue.submit([enc.finish()]);
        g.queue.present(frame);
        let now = Instant::now();
        if self.work.len() == WINDOW {
            self.work.remove(0);
        }
        self.work.push((now - t0).as_micros() as u32);
        if let Some(prev) = self.last_present.replace(now) {
            if self.interval.len() == WINDOW {
                self.interval.remove(0);
            }
            self.interval.push((now - prev).as_micros() as u32);
        }
        self.frames += 1;
        Ok(())
    }

    /// Ensure a texture per visible image and build one quad per placement.
    fn upload_images(&mut self, g: &Gpu, f: &Frame) {
        let [cw, ch] = self.text.font.cell;
        self.img_insts.clear();
        for im in &f.images {
            let fresh = self
                .textures
                .get(&im.image)
                .is_some_and(|t| t.generation == im.generation);
            if !fresh {
                let tex = g.device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("image"),
                    size: wgpu::Extent3d {
                        width: im.w,
                        height: im.h,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                    view_formats: &[],
                });
                g.queue.write_texture(
                    tex.as_image_copy(),
                    &im.rgba,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(im.w * 4),
                        rows_per_image: None,
                    },
                    tex.size(),
                );
                let view = tex.create_view(&Default::default());
                let bind = g.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("image"),
                    layout: &self.layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: self.uniforms.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(&view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::Sampler(&self.linear),
                        },
                    ],
                });
                self.textures.insert(
                    im.image,
                    Tex {
                        generation: im.generation,
                        bind,
                        last_frame: 0,
                    },
                );
            }
            if let Some(t) = self.textures.get_mut(&im.image) {
                t.last_frame = self.frames + 1;
            }
            let [sx, sy, sw, sh] = im.src;
            let (iw, ih) = (im.w as f32, im.h as f32);
            self.img_insts.push(ImgInst {
                pos: [im.col * cw, im.row * ch],
                size: [im.cols * cw, im.rows * ch],
                uv0: [sx as f32 / iw, sy as f32 / ih],
                uv1: [(sx + sw) as f32 / iw, (sy + sh) as f32 / ih],
            });
        }
        // Evict the least recently shown textures beyond the cache size.
        while self.textures.len() > TEX_CACHE.max(f.images.len()) {
            let Some(&old) = self
                .textures
                .iter()
                .min_by_key(|(_, t)| t.last_frame)
                .map(|(id, _)| id)
            else {
                break;
            };
            self.textures.remove(&old);
        }
    }

    fn draw_images(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        f: &Frame,
        range: std::ops::Range<usize>,
    ) {
        if range.is_empty() {
            return;
        }
        pass.set_pipeline(&self.img_pipe);
        let n = (self.img_insts.len() * size_of::<ImgInst>()) as u64;
        pass.set_vertex_buffer(0, self.imgs.buf.slice(..n));
        for i in range {
            if let Some(t) = self.textures.get(&f.images[i].image) {
                pass.set_bind_group(0, &t.bind, &[]);
                pass.draw(0..4, i as u32..i as u32 + 1);
            }
        }
        // Glyphs and decorations sample the atlas.
        pass.set_bind_group(0, &self.bind, &[]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shader parses and validates, and the Rust uniform block matches the WGSL
    /// struct (`U`: 2×vec2 + vec2 + vec2 + u32, 40 bytes), checked on the real device.
    #[test]
    fn shader_and_pipelines_validate() {
        let g = match gpu() {
            Ok(g) => g,
            Err(e) => {
                eprintln!("no GPU adapter, skipping: {e}");
                return;
            }
        };
        eprintln!("validated on {:?}", g.adapter.get_info().name);
        assert_eq!(size_of::<Uniforms>(), 40);
        let d = &g.device;
        let scope = d.push_error_scope(wgpu::ErrorFilter::Validation);
        let shader = d.create_shader_module(wgpu::include_wgsl!("term.wgsl"));
        let entry = |binding, visibility, ty| wgpu::BindGroupLayoutEntry {
            binding,
            visibility,
            ty,
            count: None,
        };
        let layout = d.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                entry(
                    0,
                    wgpu::ShaderStages::VERTEX,
                    wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(size_of::<Uniforms>() as u64),
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
        });
        let pl = d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        const ATTRS: [wgpu::VertexAttribute; 4] = wgpu::vertex_attr_array![
            0 => Float32x2, 1 => Float32x2, 2 => Float32x2, 3 => Unorm8x4
        ];
        for (vs, fs) in [
            ("glyph_vs", "glyph_fs"),
            ("color_glyph_vs", "color_glyph_fs"),
        ] {
            let _pipeline = d.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(fs),
                layout: Some(&pl),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some(vs),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: size_of::<GlyphInst>() as u64,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &ATTRS,
                    })],
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fs),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: wgpu::TextureFormat::Bgra8Unorm,
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
            });
        }
        let err = pollster::block_on(scope.pop());
        assert!(err.is_none(), "{err:?}");
    }
}
