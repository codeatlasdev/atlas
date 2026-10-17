//! GPU-side checks (skipped when no adapter is available).

use super::gpu::{Uniforms, bind_layout, glyph_vertex_layout, gpu, pipeline};

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
