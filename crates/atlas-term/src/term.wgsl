// Cell backgrounds (one instance per cell) and glyphs (one instance per drawn glyph).
struct U {
    cell: vec2<f32>,
    viewport: vec2<f32>,
    atlas: vec2<f32>,
    // Size of the RGBA emoji atlas (the glyph pipelines sample whichever atlas is bound).
    color_atlas: vec2<f32>,
    cols: u32,
};
@group(0) @binding(0) var<uniform> u: U;
@group(0) @binding(1) var atlas_tex: texture_2d<f32>;
@group(0) @binding(2) var atlas_smp: sampler;

fn ndc(px: vec2<f32>) -> vec4<f32> {
    return vec4<f32>(px.x / u.viewport.x * 2.0 - 1.0, 1.0 - px.y / u.viewport.y * 2.0, 0.0, 1.0);
}

fn corner(v: u32) -> vec2<f32> {
    return vec2<f32>(f32(v & 1u), f32((v >> 1u) & 1u));
}

struct BgOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn bg_vs(@builtin(vertex_index) v: u32, @builtin(instance_index) i: u32, @location(0) color: vec4<f32>) -> BgOut {
    let cell = vec2<f32>(f32(i % u.cols), f32(i / u.cols));
    var o: BgOut;
    o.pos = ndc((cell + corner(v)) * u.cell);
    o.color = color;
    return o;
}

@fragment
fn bg_fs(i: BgOut) -> @location(0) vec4<f32> {
    return i.color;
}

struct GlyphOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

@vertex
fn glyph_vs(
    @builtin(vertex_index) v: u32,
    @location(0) pos: vec2<f32>,
    @location(1) size: vec2<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) color: vec4<f32>,
) -> GlyphOut {
    let c = corner(v);
    var o: GlyphOut;
    o.pos = ndc(pos + c * size);
    o.uv = (uv + c * size) / u.atlas;
    o.color = color;
    return o;
}

// Color glyphs (emoji) live in the RGBA atlas, which is a different size: rescale uv.
@vertex
fn color_glyph_vs(
    @builtin(vertex_index) v: u32,
    @location(0) pos: vec2<f32>,
    @location(1) size: vec2<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) color: vec4<f32>,
) -> GlyphOut {
    let c = corner(v);
    var o: GlyphOut;
    o.pos = ndc(pos + c * size);
    o.uv = (uv + c * size) / u.color_atlas;
    o.color = color;
    return o;
}

@fragment
fn glyph_fs(i: GlyphOut) -> @location(0) vec4<f32> {
    let a = textureSample(atlas_tex, atlas_smp, i.uv).r;
    return vec4<f32>(i.color.rgb, i.color.a * a);
}

// Color glyph: the bitmap's own color (straight alpha); the cell fg is not applied.
@fragment
fn color_glyph_fs(i: GlyphOut) -> @location(0) vec4<f32> {
    return textureSample(atlas_tex, atlas_smp, i.uv);
}

// Decoration instances (underline, strikethrough, undercurl, …)
struct DecoInst {
    @location(0) pos: vec2<f32>,
    @location(1) size: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) @interpolate(flat) kind: u32,
};

struct DecoOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,   // 0..1 within the instance rect
    @location(1) color: vec4<f32>,
    @location(2) @interpolate(flat) kind: u32,
};

@vertex
fn deco_vs(@builtin(vertex_index) v: u32,
           @location(0) ipos: vec2<f32>,
           @location(1) isize: vec2<f32>,
           @location(2) icolor: vec4<f32>,
           @location(3) ikind: u32) -> DecoOut {
    let c = corner(v);
    var o: DecoOut;
    o.pos = ndc(ipos + c * isize);
    o.uv = c;
    o.color = icolor;
    o.kind = ikind;
    return o;
}

@fragment
fn deco_fs(i: DecoOut) -> @location(0) vec4<f32> {
    var alpha = i.color.a;
    // Undercurl (kind=2): wavy sine pattern, ~1.5 cycles across the cell width.
    if i.kind == 2u {
        let wave = sin(i.uv.x * 6.2832 * 1.5) * 0.5 + 0.5;
        let dist = abs(i.uv.y - wave);
        alpha *= clamp(1.0 - dist * 8.0, 0.0, 1.0);
    }
    // Dotted (kind=3): dots every 3px (uv mapped to cell width via 'size.x').
    if i.kind == 3u {
        let period = 3.0 / max(i.uv.x * 0.0001 + 1.0, 1.0); // width-independent
        let dot = fract(i.uv.x * 4.0); // 4 dots per cell
        alpha *= step(0.45, dot);
    }
    // Dashed (kind=4): two dashes per cell.
    if i.kind == 4u {
        let dash = fract(i.uv.x * 2.0);
        alpha *= step(dash, 0.6);
    }
    return vec4<f32>(i.color.rgb, alpha);
}

// Images: one textured quad per placement (binding 1/2 = the image + a linear sampler).
struct ImgOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn img_vs(
    @builtin(vertex_index) v: u32,
    @location(0) pos: vec2<f32>,
    @location(1) size: vec2<f32>,
    @location(2) uv0: vec2<f32>,
    @location(3) uv1: vec2<f32>,
) -> ImgOut {
    let c = corner(v);
    var o: ImgOut;
    o.pos = ndc(pos + c * size);
    o.uv = mix(uv0, uv1, c);
    return o;
}

@fragment
fn img_fs(i: ImgOut) -> @location(0) vec4<f32> {
    return textureSample(atlas_tex, atlas_smp, i.uv);
}
