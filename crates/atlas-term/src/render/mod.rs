//! Surface + instanced pipelines per frame: cell backgrounds, images below text
//! (z < 0), glyphs from the font atlas, images above text, decorations. The frame
//! is a `vt::Frame` snapshot, so the VT lock is never held while drawing.
//!
//! `gpu` owns the device and the shared pipeline/bind-group builders, `images` the inline
//! image textures, `stats` the frame timing; this file is the `Renderer` itself.

mod gpu;
mod images;
mod stats;
#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::ffi::c_void;
use std::ptr::NonNull;
use std::time::Instant;

use gpu::{
    DecoInst, Gpu, ImgInst, Uniforms, VBuf, bind_group, bind_layout, glyph_vertex_layout, gpu,
    pipeline, surface_extent, texture,
};
use images::Tex;
use stats::Timing;

use crate::font::{ATLAS, Atlas, COLOR_ATLAS, Font, GlyphInst, TextLayout};
pub use crate::font::{FontResolution, FontSpec};
use crate::vt::Frame;

pub use stats::Stats;

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
    /// The font request the current font was built from.
    spec: FontSpec,
    /// Backing scale factor (physical px per logical px).
    scale: f32,
    timing: Timing,
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
        spec: FontSpec,
    ) -> Result<(Self, FontResolution), String> {
        let g = gpu()?;
        let (font, resolution) = Font::with_spec(font_size, scale, &spec);
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
        let (width, height) =
            surface_extent(width, height, g.device.limits().max_texture_dimension_2d);
        let mut config = surface
            .get_default_config(&g.adapter, width, height)
            .ok_or("surface unsupported by adapter")?;
        config.format = format;
        config.present_mode = wgpu::PresentMode::Fifo;
        config.desired_maximum_frame_latency = 2;
        config.alpha_mode = wgpu::CompositeAlphaMode::Opaque;
        surface.configure(&g.device, &config);

        let d = &g.device;
        let shader = d.create_shader_module(wgpu::include_wgsl!("../term.wgsl"));
        let uniforms = d.create_buffer(&wgpu::BufferDescriptor {
            label: Some("uniforms"),
            size: size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let atlas = texture(
            d,
            "glyph atlas",
            (ATLAS, ATLAS),
            wgpu::TextureFormat::R8Unorm,
        );
        let color_atlas = texture(
            d,
            "color glyph atlas",
            (COLOR_ATLAS, COLOR_ATLAS),
            wgpu::TextureFormat::Rgba8Unorm,
        );
        let sampler = d.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let layout = bind_layout(d, None);
        let atlas_view = atlas.create_view(&Default::default());
        let bind = bind_group(d, None, &layout, &uniforms, &atlas_view, &sampler);
        let color_view = color_atlas.create_view(&Default::default());
        let color_bind = bind_group(d, None, &layout, &uniforms, &color_view, &sampler);
        let pl = d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipe = |label: &str, shaders: (&str, &str), buffer: wgpu::VertexBufferLayout<'_>| {
            pipeline(d, &shader, &pl, format, label, shaders, buffer)
        };
        let bg_pipe = pipe(
            "bg",
            ("bg_vs", "bg_fs"),
            wgpu::VertexBufferLayout {
                array_stride: 4,
                step_mode: wgpu::VertexStepMode::Instance,
                attributes: &wgpu::vertex_attr_array![0 => Unorm8x4],
            },
        );
        let glyph_pipe = pipe("glyph", ("glyph_vs", "glyph_fs"), glyph_vertex_layout());
        let color_pipe = pipe(
            "color glyph",
            ("color_glyph_vs", "color_glyph_fs"),
            glyph_vertex_layout(),
        );
        let deco_pipe = pipe(
            "deco",
            ("deco_vs", "deco_fs"),
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
            ("img_vs", "img_fs"),
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
        let renderer = Self {
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
            text: TextLayout::new(font),
            spec,
            scale,
            timing: Timing::new(),
        };
        Ok((renderer, resolution))
    }

    /// Physical pixels, clamped to what the surface accepts (see `surface_extent`). Returns the grid size that fits.
    pub fn resize(&mut self, width: u32, height: u32) -> (u16, u16) {
        let (mut w, mut h) = (width.max(1), height.max(1));
        if let Ok(g) = gpu() {
            (w, h) = surface_extent(w, h, g.device.limits().max_texture_dimension_2d);
            if (w, h) != (self.config.width, self.config.height) {
                self.config.width = w;
                self.config.height = h;
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
        self.timing.stats()
    }

    /// Reset the measurement window.
    pub fn reset_stats(&mut self) {
        self.timing.reset();
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

    /// The font request the current font was built from.
    pub fn font_spec(&self) -> &FontSpec {
        &self.spec
    }

    /// Rebuild the font for `spec` at `pt` points (same backing scale): the grid that fits
    /// the current pixel size, and which family the request resolved to. Both atlases and
    /// every cached row are dropped.
    pub fn set_font(&mut self, pt: f32, spec: &FontSpec) -> ((u16, u16), FontResolution) {
        let ligatures = self.text.ligatures();
        let (font, res) = Font::with_spec(pt, self.scale, spec);
        self.text = TextLayout::new(font);
        self.spec = spec.clone();
        self.text.set_ligatures(ligatures);
        // The fresh atlases are blank: upload them over the old textures.
        self.text.font.gray.dirty = Some((0, ATLAS));
        self.text.font.color.dirty = Some((0, COLOR_ATLAS));
        (self.resize(self.config.width, self.config.height), res)
    }

    /// Enable/disable `calt`/`liga` ligature shaping (default on); redraws shape anew.
    pub fn set_ligatures(&mut self, on: bool) {
        self.text.set_ligatures(on);
    }

    pub fn ligatures(&self) -> bool {
        self.text.ligatures()
    }

    /// Decorations for this frame: underlines/strikethrough, the cursor bars and the
    /// scrollbar thumb, as pixel-space instances.
    fn build_decos(&mut self, f: &Frame) {
        let [cw, ch] = self.text.font.cell;
        let base = self.text.font.baseline;
        let (ul_th, ul_off) = self.text.font.underline;
        let (st_th, st_off) = self.text.font.strikethrough;
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
    }

    /// Draw a snapshot (blocks on vsync when the swapchain is full). `true`: a frame was presented; `false`:
    /// the surface had no texture to draw into (lost or outdated, reconfigured for the next call).
    pub fn render(&mut self, f: &Frame) -> Result<bool, String> {
        let g = gpu()?;
        let frame = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t)
            | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&g.device, &self.config);
                return Ok(false);
            }
            _ => return Ok(false),
        };
        let t0 = Instant::now();
        self.text.layout(f);
        self.build_decos(f);
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
        self.timing.record(t0);
        Ok(true)
    }
}
