//! Inline images on the GPU: one texture per image (re-uploaded when its generation
//! changes, LRU-evicted past a byte budget), one quad per placement, drawn under or over the text.

use std::collections::HashMap;

use super::Renderer;
use super::gpu::{Gpu, ImgInst, bind_group, texture};
use crate::vt::Frame;

/// Uploaded image: its bind group keeps the view, and so the texture, alive.
pub(super) struct Tex {
    bind: wgpu::BindGroup,
}

/// GPU bytes of image textures kept per terminal (RGBA8). Images on screen always stay, even past this.
pub(super) const TEX_BUDGET: u64 = 64 << 20;

/// Byte-bounded LRU of uploaded images, keyed by image id. `T` is the GPU payload (a `Tex`; a unit in the
/// tests, which have no adapter). Eviction is least recently shown first, never one shown this frame.
pub(super) struct TexCache<T> {
    map: HashMap<u32, Entry<T>>,
    bytes: u64,
    budget: u64,
}

struct Entry<T> {
    val: T,
    bytes: u64,
    generation: u64,
    last_frame: u64,
}

impl<T> TexCache<T> {
    pub(super) fn new(budget: u64) -> Self {
        Self {
            map: HashMap::new(),
            bytes: 0,
            budget,
        }
    }

    /// Texture bytes currently held.
    pub(super) fn bytes(&self) -> u64 {
        self.bytes
    }

    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.map.len()
    }

    /// The cached upload of `id` if it is of this `generation`.
    pub(super) fn fresh(&self, id: u32, generation: u64) -> bool {
        self.map
            .get(&id)
            .is_some_and(|e| e.generation == generation)
    }

    pub(super) fn get(&self, id: u32) -> Option<&T> {
        self.map.get(&id).map(|e| &e.val)
    }

    /// Mark `id` as shown in `frame`.
    pub(super) fn touch(&mut self, id: u32, frame: u64) {
        if let Some(e) = self.map.get_mut(&id) {
            e.last_frame = frame;
        }
    }

    /// Store an upload of `bytes`, replacing the previous one under `id`.
    pub(super) fn insert(&mut self, id: u32, generation: u64, bytes: u64, val: T, frame: u64) {
        let old = self.map.insert(
            id,
            Entry {
                val,
                bytes,
                generation,
                last_frame: frame,
            },
        );
        self.bytes = self.bytes + bytes - old.map_or(0, |e| e.bytes);
    }

    /// Drop least recently shown entries until under budget; entries shown in `frame` stay.
    pub(super) fn evict(&mut self, frame: u64) {
        while self.bytes > self.budget {
            let Some(old) = self
                .map
                .iter()
                .filter(|(_, e)| e.last_frame != frame)
                .min_by_key(|(_, e)| e.last_frame)
                .map(|(id, _)| *id)
            else {
                break;
            };
            if let Some(e) = self.map.remove(&old) {
                self.bytes -= e.bytes;
            }
        }
    }
}

impl Renderer {
    /// Ensure a texture per visible image and build one quad per placement.
    pub(super) fn upload_images(&mut self, g: &Gpu, f: &Frame) {
        let [cw, ch] = self.text.font.cell;
        let frame = self.timing.frames + 1;
        self.img_insts.clear();
        for im in &f.images {
            let fresh = self.textures.fresh(im.image, im.generation);
            if !fresh {
                let tex = texture(
                    &g.device,
                    "image",
                    (im.w, im.h),
                    wgpu::TextureFormat::Rgba8Unorm,
                );
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
                let bind = bind_group(
                    &g.device,
                    Some("image"),
                    &self.layout,
                    &self.uniforms,
                    &view,
                    &self.linear,
                );
                self.textures.insert(
                    im.image,
                    im.generation,
                    im.w as u64 * im.h as u64 * 4,
                    Tex { bind },
                    frame,
                );
            }
            self.textures.touch(im.image, frame);
            let [sx, sy, sw, sh] = im.src;
            let (iw, ih) = (im.w as f32, im.h as f32);
            self.img_insts.push(ImgInst {
                pos: [im.col * cw, (im.row + f.over as f32) * ch],
                size: [im.cols * cw, im.rows * ch],
                uv0: [sx as f32 / iw, sy as f32 / ih],
                uv1: [(sx + sw) as f32 / iw, (sy + sh) as f32 / ih],
            });
        }
        // Evict the least recently shown textures beyond the byte budget.
        self.textures.evict(frame);
    }

    pub(super) fn draw_images(
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
            if let Some(t) = self.textures.get(f.images[i].image) {
                pass.set_bind_group(0, &t.bind, &[]);
                pass.draw(0..4, i as u32..i as u32 + 1);
            }
        }
        // Glyphs and decorations sample the atlas.
        pass.set_bind_group(0, &self.bind, &[]);
    }
}
