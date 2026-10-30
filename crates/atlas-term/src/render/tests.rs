//! GPU-side checks (skipped when no adapter is available).

use super::gpu::{Uniforms, bind_layout, glyph_vertex_layout, gpu, pipeline};

/// The shader parses and validates, and the Rust uniform block matches the WGSL
/// struct (`U`: 2×vec2 + vec2 + vec2 + u32 + f32, 40 bytes), checked on the real device.
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
    let shader = d.create_shader_module(wgpu::include_wgsl!("../term.wgsl"));
    let layout = bind_layout(d, wgpu::BufferSize::new(size_of::<Uniforms>() as u64));
    let pl = d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: None,
        bind_group_layouts: &[Some(&layout)],
        immediate_size: 0,
    });
    for shaders in [
        ("glyph_vs", "glyph_fs"),
        ("color_glyph_vs", "color_glyph_fs"),
    ] {
        let _pipeline = pipeline(
            d,
            &shader,
            &pl,
            wgpu::TextureFormat::Bgra8Unorm,
            shaders.1,
            shaders,
            glyph_vertex_layout(),
        );
    }
    let err = pollster::block_on(scope.pop());
    assert!(err.is_none(), "{err:?}");
}

use super::gpu::surface_extent;

// Incident (#63): a pane momentarily laid out far past the GPU's texture limit (the view is created parked and its
// rect arrives before layout settles) reached `Surface::configure`, which panicked the render thread:
// "Surface width and height must be within the maximum supported texture size. Requested (200000, 200000), maximum 8192".
#[test]
fn surface_extent_clamps_an_oversized_request_to_the_device_limit() {
    assert_eq!(surface_extent(200_000, 200_000, 8192), (8192, 8192));
    assert_eq!(surface_extent(8193, 100, 8192), (8192, 100));
    assert_eq!(surface_extent(100, 8193, 8192), (100, 8192));
}

#[test]
fn surface_extent_never_returns_zero_and_keeps_valid_sizes() {
    assert_eq!(surface_extent(0, 0, 8192), (1, 1));
    assert_eq!(surface_extent(0, 600, 8192), (1, 600));
    assert_eq!(surface_extent(8192, 8192, 8192), (8192, 8192));
    assert_eq!(surface_extent(1, 1, 8192), (1, 1));
    assert_eq!(surface_extent(1280, 800, 8192), (1280, 800));
}

#[test]
fn surface_extent_survives_a_degenerate_device_limit() {
    // A reported limit of 0 must still give a configurable (non-zero) surface.
    assert_eq!(surface_extent(500, 500, 0), (1, 1));
}

use super::gpu::Sizing;
use super::images::TexCache;

const KB: u64 = 1024;

// R3 (LeakStudy): the vertex buffer grew to the next power of two and never shrank, so one full-screen burst
// kept its high-water mark for the life of the tab (multiplied by every open tab).
#[test]
fn vertex_buffer_shrinks_back_after_a_burst() {
    let mut s = Sizing::new();
    let base = s.cap();
    assert_eq!(s.on_write(6 << 20), Some(8 << 20));
    assert_eq!(s.cap(), 8 << 20);
    let mut shrank = None;
    for n in 1..=1000 {
        if let Some(c) = s.on_write(40 * KB) {
            shrank = Some((n, c));
            break;
        }
    }
    let (n, cap) = shrank.expect("never shrank after the burst");
    assert!(
        n >= 100,
        "shrank after only {n} quiet writes (thrashing risk)"
    );
    assert_eq!(cap, 128 * KB, "twice the recent peak, power of two");
    assert!(s.cap() < 8 << 20 && s.cap() >= base);
}

#[test]
fn vertex_buffer_does_not_thrash_on_a_fluctuating_size() {
    let mut s = Sizing::new();
    s.on_write(1 << 20);
    // Alternating small and near-capacity frames: the quiet streak keeps resetting.
    for i in 0..10_000 {
        let len = if i % 50 == 0 { 600 * KB } else { 10 * KB };
        assert_eq!(s.on_write(len), None, "reallocated at write {i}");
    }
    assert_eq!(s.cap(), 1 << 20);
}

#[test]
fn vertex_buffer_never_shrinks_below_its_floor() {
    let mut s = Sizing::new();
    let floor = s.cap();
    for _ in 0..1000 {
        assert_eq!(s.on_write(0), None);
    }
    assert_eq!(s.cap(), floor);
    s.on_write(floor + 1);
    for _ in 0..1000 {
        s.on_write(0);
    }
    assert_eq!(s.cap(), floor);
}

#[test]
fn vertex_buffer_grows_again_after_shrinking() {
    let mut s = Sizing::new();
    s.on_write(4 << 20);
    for _ in 0..1000 {
        s.on_write(KB);
    }
    assert!(s.cap() < 4 << 20);
    assert_eq!(s.on_write(4 << 20), Some(4 << 20));
}

// R5 (LeakStudy): textures were bounded by count (32), so 32 large images meant about 1 GB per tab.
#[test]
fn image_cache_is_bounded_in_bytes_not_count() {
    let mut c = TexCache::<()>::new(64 << 20);
    let img = 16u64 << 20; // a 2048x2048 RGBA image
    for id in 0..100u32 {
        let frame = id as u64 + 1;
        c.insert(id, 1, img, (), frame);
        c.touch(id, frame);
        c.evict(frame);
        assert!(
            c.bytes() <= 64 << 20,
            "{} bytes after image {id}",
            c.bytes()
        );
    }
    assert_eq!(c.len(), 4);
    assert!(c.get(99).is_some() && c.get(0).is_none());
}

#[test]
fn image_cache_evicts_the_least_recently_shown_first() {
    let mut c = TexCache::<()>::new(30);
    for (id, frame) in [(1, 1), (2, 2), (3, 3)] {
        c.insert(id, 1, 10, (), frame);
    }
    c.touch(1, 4); // 1 shown again: 2 is now the oldest
    c.insert(4, 1, 10, (), 4);
    c.evict(4);
    assert!(c.get(2).is_none());
    assert!(c.get(1).is_some() && c.get(3).is_some() && c.get(4).is_some());
    assert_eq!(c.bytes(), 30);
}

#[test]
fn image_cache_keeps_what_is_on_screen_even_past_the_budget() {
    let mut c = TexCache::<()>::new(10);
    c.insert(1, 1, 40, (), 7);
    c.insert(2, 1, 40, (), 7);
    c.evict(7);
    assert_eq!(c.len(), 2, "both are shown in frame 7");
    c.evict(8);
    assert_eq!(c.len(), 0, "not shown any more: evicted");
    assert_eq!(c.bytes(), 0);
}

#[test]
fn image_cache_replacing_an_upload_keeps_the_byte_count_exact() {
    let mut c = TexCache::<()>::new(1 << 30);
    c.insert(1, 1, 100, (), 1);
    assert!(c.fresh(1, 1) && !c.fresh(1, 2));
    c.insert(1, 2, 60, (), 2);
    assert_eq!(c.bytes(), 60);
    assert!(c.fresh(1, 2));
}

// The scroll thumb was a fixed light grey: invisible on a light terminal theme. It contrasts with the background.
#[test]
fn scroll_thumb_is_dark_on_light_backgrounds_and_light_on_dark_ones() {
    let on_dark = super::thumb_color([0x1a, 0x1b, 0x26, 0xff]);
    let on_light = super::thumb_color([0xfa, 0xfa, 0xfa, 0xff]);
    assert!(
        on_dark[0] > 0x80 && on_light[0] < 0x80,
        "{on_dark:?} {on_light:?}"
    );
    assert!(on_dark[3] > 0 && on_dark[3] < 0xff);
}

use super::grid_origin_y;

// Smooth scrolling: the grid is drawn `over` rows higher than the viewport top (those rows are the overscan above),
// then pushed down by the sub-row remainder. Positive `frac` = content moves down = older lines come in.
#[test]
fn grid_origin_hides_the_overscan_and_applies_the_sub_row_offset() {
    assert_eq!(
        grid_origin_y(0, 0.0, 20.0),
        0.0,
        "no smooth scrolling: today's layout, untouched"
    );
    assert_eq!(
        grid_origin_y(1, 0.0, 20.0),
        -20.0,
        "the overscan row sits above the viewport"
    );
    assert_eq!(grid_origin_y(1, 0.25, 20.0), -15.0);
    assert_eq!(grid_origin_y(1, -0.25, 20.0), -25.0);
}

// A fraction the renderer cannot honour (no overscan rows to show) must not shift the content: a strip would show.
#[test]
fn grid_origin_ignores_a_fraction_without_overscan() {
    assert_eq!(grid_origin_y(0, 0.4, 20.0), 0.0);
}
