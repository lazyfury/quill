//! Headless `DrawList -> wgpu -> pixels` tests.
//!
//! The backend renders to an offscreen texture and the tests read the pixels
//! back programmatically. No window and no screenshot are involved (see
//! `AGENTS.md`).
//!
//! If no adapter is available (e.g. a GPU-less CI box) the tests skip rather
//! than fail, so the rest of the workspace still builds and tests.

use draw_backend_wgpu::{
    wgpu, FontConfig, FontMode, PixelBuffer, TextureFilter, WgpuBackend, PIXEL_GLYPH_RATIO,
};
use draw_core::{Color, FontWeight, Rect, Size, Vec2, ViewportSize};
use draw_render::{
    CornerRadii, Paint, PaintContext, RenderBackend, RenderTargetId, TextAlign, TextureId,
};

/// Attempts to create a backend; `None` means "skip, no GPU adapter".
fn backend() -> Option<WgpuBackend> {
    match WgpuBackend::new() {
        Ok(backend) => Some(backend),
        Err(error) => {
            eprintln!("skipping wgpu test: {error}");
            None
        }
    }
}

fn viewport(width: f32, height: f32) -> ViewportSize {
    ViewportSize::new(Size::new(width, height))
}

fn render(backend: &mut WgpuBackend, ctx: PaintContext, viewport: ViewportSize) -> PixelBuffer {
    backend.begin_frame(viewport).unwrap();
    backend.submit(&ctx.into_draw_list()).unwrap();
    backend.end_frame().unwrap();
    backend.read_pixels().unwrap()
}

fn assert_pixel(pixels: &PixelBuffer, x: u32, y: u32, expected: [u8; 4]) {
    assert_pixel_tol(pixels, x, y, expected, 2);
}

fn assert_pixel_tol(pixels: &PixelBuffer, x: u32, y: u32, expected: [u8; 4], tolerance: i32) {
    let actual = pixels.pixel(x, y).expect("pixel in bounds");
    let close = actual
        .iter()
        .zip(expected.iter())
        .all(|(a, b)| (*a as i32 - *b as i32).abs() <= tolerance);
    assert!(
        close,
        "pixel ({x}, {y}) = {actual:?}, expected {expected:?} (+/- {tolerance})"
    );
}

#[test]
fn fills_rect_with_solid_color() {
    let Some(mut backend) = backend() else {
        return;
    };
    let mut ctx = PaintContext::new();
    ctx.fill_rect(
        Rect::from_min_size(Vec2::ZERO, Size::new(32.0, 32.0)),
        Color::RED,
    );

    let pixels = render(&mut backend, ctx, viewport(32.0, 32.0));
    assert_eq!(pixels.width, 32);
    assert_eq!(pixels.height, 32);
    assert_pixel(&pixels, 16, 16, [255, 0, 0, 255]);
}

#[test]
fn clear_color_fills_the_untouched_target() {
    let Some(mut backend) = backend() else {
        return;
    };
    backend.set_clear_color(Color::rgb(0.0, 0.0, 1.0));

    let pixels = render(&mut backend, PaintContext::new(), viewport(16.0, 16.0));
    assert_pixel(&pixels, 8, 8, [0, 0, 255, 255]);
}

#[test]
fn scale_factor_resizes_target_and_scales_geometry() {
    let Some(mut backend) = backend() else {
        return;
    };
    backend.set_scale_factor(2.0);
    assert_eq!(backend.scale_factor(), 2.0);

    // Logical 8x8 rect at the origin on a logical 16x16 viewport.
    let mut ctx = PaintContext::new();
    ctx.fill_rect(
        Rect::from_min_size(Vec2::ZERO, Size::splat(8.0)),
        Color::BLUE,
    );

    let pixels = render(&mut backend, ctx, viewport(16.0, 16.0));
    // DPR doubled the backing store...
    assert_eq!(pixels.width, 32);
    assert_eq!(pixels.height, 32);
    // ...and the logical rect now covers device [0, 16).
    assert_pixel(&pixels, 4, 4, [0, 0, 255, 255]);
    assert_pixel(&pixels, 24, 24, [0, 0, 0, 0]);
}

#[test]
fn clip_rect_limits_subsequent_drawing() {
    let Some(mut backend) = backend() else {
        return;
    };
    let full = Rect::from_min_size(Vec2::ZERO, Size::splat(32.0));

    let mut ctx = PaintContext::new();
    ctx.fill_rect(full, Color::RED);
    ctx.clip_rect(Rect::from_min_size(Vec2::splat(4.0), Size::splat(8.0)));
    ctx.fill_rect(full, Color::GREEN);

    let pixels = render(&mut backend, ctx, viewport(32.0, 32.0));
    assert_pixel(&pixels, 8, 8, [0, 255, 0, 255]); // inside the clip
    assert_pixel(&pixels, 16, 16, [255, 0, 0, 255]); // clipped away
    assert_pixel(&pixels, 2, 2, [255, 0, 0, 255]); // outside the clip
}

#[test]
fn opacity_blends_toward_the_background() {
    let Some(mut backend) = backend() else {
        return;
    };
    backend.set_clear_color(Color::BLACK);

    let mut ctx = PaintContext::new();
    ctx.set_opacity(0.5);
    ctx.fill_rect(
        Rect::from_min_size(Vec2::ZERO, Size::splat(16.0)),
        Color::WHITE,
    );

    let pixels = render(&mut backend, ctx, viewport(16.0, 16.0));
    assert_pixel(&pixels, 8, 8, [128, 128, 128, 255]);
}

#[test]
fn save_restore_restores_opacity() {
    let Some(mut backend) = backend() else {
        return;
    };
    backend.set_clear_color(Color::BLACK);

    let mut ctx = PaintContext::new();
    ctx.set_opacity(0.5);
    ctx.save();
    ctx.set_opacity(1.0);
    ctx.restore();
    ctx.fill_rect(
        Rect::from_min_size(Vec2::ZERO, Size::splat(16.0)),
        Color::WHITE,
    );

    let pixels = render(&mut backend, ctx, viewport(16.0, 16.0));
    assert_pixel(&pixels, 8, 8, [128, 128, 128, 255]);
}

#[test]
fn transform_is_baked_into_the_geometry() {
    let Some(mut backend) = backend() else {
        return;
    };

    let mut ctx = PaintContext::new();
    ctx.set_transform(draw_core::Transform2D::from_translation(Vec2::new(
        8.0, 8.0,
    )));
    ctx.fill_rect(
        Rect::from_min_size(Vec2::ZERO, Size::splat(8.0)),
        Color::GREEN,
    );

    let pixels = render(&mut backend, ctx, viewport(32.0, 32.0));
    assert_pixel(&pixels, 12, 12, [0, 255, 0, 255]); // translated
    assert_pixel(&pixels, 2, 2, [0, 0, 0, 0]); // origin left untouched
}

#[test]
fn circle_covers_center_but_not_corner() {
    let Some(mut backend) = backend() else {
        return;
    };

    let mut ctx = PaintContext::new();
    ctx.fill_circle(Vec2::new(16.0, 16.0), 8.0, Color::RED);

    let pixels = render(&mut backend, ctx, viewport(32.0, 32.0));
    assert_pixel(&pixels, 16, 16, [255, 0, 0, 255]);
    assert_pixel(&pixels, 1, 1, [0, 0, 0, 0]);
}

#[test]
fn stroke_rect_outline_leaves_the_center_empty() {
    let Some(mut backend) = backend() else {
        return;
    };

    let mut ctx = PaintContext::new();
    ctx.stroke_rect(
        Rect::from_min_size(Vec2::new(4.0, 4.0), Size::splat(16.0)),
        2.0,
        Color::WHITE,
    );

    let pixels = render(&mut backend, ctx, viewport(32.0, 32.0));
    assert_pixel(&pixels, 5, 5, [255, 255, 255, 255]); // on the top edge
    assert_pixel(&pixels, 12, 12, [0, 0, 0, 0]); // hole in the middle
}

#[test]
fn line_covers_its_segment_but_not_the_sides() {
    let Some(mut backend) = backend() else {
        return;
    };

    let mut ctx = PaintContext::new();
    ctx.draw_line(
        Vec2::new(4.0, 16.0),
        Vec2::new(28.0, 16.0),
        2.0,
        Color::WHITE,
    );

    let pixels = render(&mut backend, ctx, viewport(32.0, 32.0));
    assert_pixel(&pixels, 16, 16, [255, 255, 255, 255]); // on the segment
    assert_pixel(&pixels, 16, 4, [0, 0, 0, 0]); // above it
    assert_pixel(&pixels, 2, 16, [0, 0, 0, 0]); // before the start
}

#[test]
fn rounded_rect_fills_the_center_but_not_the_corner() {
    let Some(mut backend) = backend() else {
        return;
    };

    let mut ctx = PaintContext::new();
    ctx.fill_rounded_rect(
        Rect::from_min_size(Vec2::new(4.0, 4.0), Size::splat(24.0)),
        8.0,
        Color::RED,
    );

    let pixels = render(&mut backend, ctx, viewport(32.0, 32.0));
    assert_pixel(&pixels, 16, 16, [255, 0, 0, 255]); // center
    assert_pixel(&pixels, 16, 5, [255, 0, 0, 255]); // top edge
    assert_pixel(&pixels, 4, 4, [0, 0, 0, 0]); // rounded-away corner
}

#[test]
fn rounded_rect_supports_mixed_corners() {
    let Some(mut backend) = backend() else {
        return;
    };

    let mut ctx = PaintContext::new();
    ctx.fill_rounded_rect_corners(
        Rect::from_min_size(Vec2::new(4.0, 4.0), Size::splat(24.0)),
        CornerRadii::new(0.0, 8.0, 8.0, 0.0),
        Color::RED,
    );

    let pixels = render(&mut backend, ctx, viewport(32.0, 32.0));
    assert_pixel(&pixels, 4, 4, [255, 0, 0, 255]); // square top-left
    assert_pixel(&pixels, 4, 27, [255, 0, 0, 255]); // square bottom-left
    assert_pixel(&pixels, 27, 4, [0, 0, 0, 0]); // rounded top-right
    assert_pixel(&pixels, 27, 27, [0, 0, 0, 0]); // rounded bottom-right
}

#[test]
fn stroke_rounded_rect_leaves_the_center_empty() {
    let Some(mut backend) = backend() else {
        return;
    };

    let mut ctx = PaintContext::new();
    ctx.stroke_rounded_rect(
        Rect::from_min_size(Vec2::new(4.0, 4.0), Size::splat(24.0)),
        8.0,
        2.0,
        Color::WHITE,
    );

    let pixels = render(&mut backend, ctx, viewport(32.0, 32.0));
    assert_pixel(&pixels, 16, 5, [255, 255, 255, 255]); // top edge
    assert_pixel(&pixels, 16, 16, [0, 0, 0, 0]); // hole in the middle
}

#[test]
fn draw_image_samples_a_registered_texture() {
    let Some(mut backend) = backend() else {
        return;
    };
    // 2x2: top-left red, top-right green, bottom-left blue, bottom-right white.
    let texture = [
        255, 0, 0, 255, 0, 255, 0, 255, // row 0
        0, 0, 255, 255, 255, 255, 255, 255, // row 1
    ];
    let id = TextureId::new(1);
    backend.register_texture(id, 2, 2, &texture).unwrap();

    let mut ctx = PaintContext::new();
    ctx.draw_image(
        id,
        Rect::from_min_size(Vec2::ZERO, Size::splat(32.0)),
        None,
        Paint::new(Color::WHITE),
    );

    let pixels = render(&mut backend, ctx, viewport(32.0, 32.0));
    // Linear filtering blends slightly at the two-texel boundary, so allow a
    // wider tolerance than the exact-fill assertions.
    assert_pixel_tol(&pixels, 8, 8, [255, 0, 0, 255], 16);
    assert_pixel_tol(&pixels, 24, 8, [0, 255, 0, 255], 16);
    assert_pixel_tol(&pixels, 8, 24, [0, 0, 255, 255], 16);
    assert_pixel_tol(&pixels, 24, 24, [255, 255, 255, 255], 16);
}

#[test]
fn draw_image_samples_a_texture_registered_through_the_trait() {
    let Some(mut backend) = backend() else {
        return;
    };
    // A 1x1 white texture, registered through the neutral `RenderBackend`
    // contract rather than the inherent wgpu method.
    let id = TextureId::new(11);
    RenderBackend::register_texture(&mut backend, id, 1, 1, &[255, 255, 255, 255]).unwrap();

    let mut ctx = PaintContext::new();
    ctx.draw_image(
        id,
        Rect::from_min_size(Vec2::ZERO, Size::splat(16.0)),
        None,
        Paint::new(Color::WHITE),
    );

    let pixels = render(&mut backend, ctx, viewport(16.0, 16.0));
    assert_pixel(&pixels, 8, 8, [255, 255, 255, 255]);
}

#[test]
fn a_render_target_is_sampleable_as_a_texture() {
    let Some(mut backend) = backend() else {
        return;
    };
    let target = RenderTargetId::from_raw(90);
    backend.create_render_target(target, 16, 16).unwrap();

    // Render solid red into the target (a complete offscreen pass).
    let mut inner = PaintContext::new();
    inner.fill_rect(
        Rect::from_min_size(Vec2::ZERO, Size::splat(16.0)),
        Color::RED,
    );
    backend
        .render_to_target(target, &inner.into_draw_list())
        .unwrap();

    // Composite the target as a texture over a 32x32 frame.
    let mut ctx = PaintContext::new();
    ctx.draw_image(
        target.texture(),
        Rect::from_min_size(Vec2::ZERO, Size::splat(32.0)),
        None,
        Paint::new(Color::WHITE),
    );
    let pixels = render(&mut backend, ctx, viewport(32.0, 32.0));
    assert_pixel(&pixels, 16, 16, [255, 0, 0, 255]);

    backend.destroy_render_target(target).unwrap();
}

#[test]
fn nearest_texture_filter_keeps_hard_texel_edges() {
    let Some(mut backend) = backend() else {
        return;
    };
    // 2x1: left texel red, right texel green.
    let texture = [255, 0, 0, 255, 0, 255, 0, 255];
    let id = TextureId::new(7);
    backend
        .register_texture_with_filter(id, 2, 1, &texture, TextureFilter::Nearest)
        .unwrap();

    let mut ctx = PaintContext::new();
    ctx.draw_image(
        id,
        Rect::from_min_size(Vec2::ZERO, Size::splat(32.0)),
        None,
        Paint::new(Color::WHITE),
    );

    let pixels = render(&mut backend, ctx, viewport(32.0, 32.0));
    // Each half samples its texel exactly, including right up to the boundary
    // (linear filtering would blend the two centre columns).
    assert_pixel(&pixels, 4, 16, [255, 0, 0, 255]);
    assert_pixel(&pixels, 15, 16, [255, 0, 0, 255]);
    assert_pixel(&pixels, 16, 16, [0, 255, 0, 255]);
    assert_pixel(&pixels, 27, 16, [0, 255, 0, 255]);
}

#[test]
fn set_texture_filter_rebuilds_an_existing_texture() {
    let Some(mut backend) = backend() else {
        return;
    };
    let texture = [255, 0, 0, 255, 0, 255, 0, 255];
    let id = TextureId::new(8);
    // Registered with the default linear filter: the centre is a blend.
    backend.register_texture(id, 2, 1, &texture).unwrap();
    let mut ctx = PaintContext::new();
    ctx.draw_image(
        id,
        Rect::from_min_size(Vec2::ZERO, Size::splat(32.0)),
        None,
        Paint::new(Color::WHITE),
    );
    let linear = render(&mut backend, ctx, viewport(32.0, 32.0));
    let blended = linear.pixel(16, 16).unwrap();
    assert!(
        blended[0] < 255 && blended[1] > 0,
        "linear sampling should blend at the boundary, got {blended:?}"
    );

    // Switching to nearest rebuilds the bind group; the boundary is now hard.
    backend.set_texture_filter(id, TextureFilter::Nearest);
    let mut ctx = PaintContext::new();
    ctx.draw_image(
        id,
        Rect::from_min_size(Vec2::ZERO, Size::splat(32.0)),
        None,
        Paint::new(Color::WHITE),
    );
    let nearest = render(&mut backend, ctx, viewport(32.0, 32.0));
    assert_pixel(&nearest, 16, 16, [0, 255, 0, 255]);
}

#[test]
fn draw_image_honours_the_source_subrect() {
    let Some(mut backend) = backend() else {
        return;
    };
    // 2x2 with only the bottom-right white; sample just that texel.
    let texture = [
        0, 0, 0, 0, 0, 0, 0, 0, // row 0 transparent
        0, 0, 0, 0, 255, 255, 255, 255, // row 1 bottom-right white
    ];
    let id = TextureId::new(2);
    backend.register_texture(id, 2, 2, &texture).unwrap();

    let mut ctx = PaintContext::new();
    ctx.draw_image(
        id,
        Rect::from_min_size(Vec2::ZERO, Size::splat(16.0)),
        Some(Rect::from_min_size(Vec2::new(1.0, 1.0), Size::splat(1.0))),
        Paint::new(Color::WHITE),
    );

    let pixels = render(&mut backend, ctx, viewport(16.0, 16.0));
    assert_pixel(&pixels, 8, 8, [255, 255, 255, 255]);
}

#[test]
fn update_texture_rewrites_an_existing_texture_in_place() {
    let Some(mut backend) = backend() else {
        return;
    };
    let id = TextureId::new(3);
    backend
        .register_texture(id, 1, 1, &[255, 0, 0, 255])
        .unwrap();

    let mut ctx = PaintContext::new();
    ctx.draw_image(
        id,
        Rect::from_min_size(Vec2::ZERO, Size::splat(8.0)),
        None,
        Paint::new(Color::WHITE),
    );
    let pixels = render(&mut backend, ctx, viewport(8.0, 8.0));
    assert_pixel(&pixels, 4, 4, [255, 0, 0, 255]);

    // Same size -> in-place rewrite of the same texture id.
    backend.update_texture(id, 1, 1, &[0, 255, 0, 255]).unwrap();
    let mut ctx = PaintContext::new();
    ctx.draw_image(
        id,
        Rect::from_min_size(Vec2::ZERO, Size::splat(8.0)),
        None,
        Paint::new(Color::WHITE),
    );
    let pixels = render(&mut backend, ctx, viewport(8.0, 8.0));
    assert_pixel(&pixels, 4, 4, [0, 255, 0, 255]);
}

#[test]
fn draw_text_rasterizes_visible_glyphs() {
    let Some(mut backend) = backend() else {
        return;
    };
    backend.set_clear_color(Color::BLACK);

    let mut ctx = PaintContext::new();
    ctx.draw_text(
        "M",
        Vec2::new(4.0, 20.0),
        16.0,
        TextAlign::Left,
        Paint::new(Color::WHITE),
    );

    let pixels = render(&mut backend, ctx, viewport(32.0, 32.0));
    // The glyph cell occupies x in [4, 20), y in [4, 20). Look for ink.
    let inked = (4..20).any(|y| (4..20).any(|x| pixels.pixel(x, y).unwrap()[0] > 128));
    assert!(
        inked,
        "expected the glyph to draw at least one bright pixel"
    );
}

/// Bold reaches the GPU: the same glyph at 700 has more ink than at 400 (when
/// the default family ships a bolder face).
#[test]
fn bold_text_has_more_ink_than_regular() {
    let Some(mut backend) = backend() else {
        return;
    };
    let metrics = backend.text_metrics();
    if !metrics.is_system() {
        return; // the pixel font has one weight
    }
    let default = metrics.name().unwrap_or_default().to_string();
    let has_bolder = metrics
        .families()
        .iter()
        .find(|family| family.name == default)
        .is_some_and(|family| family.weights.iter().any(|weight| *weight >= 600));
    if !has_bolder {
        return; // no bolder face to compare against
    }

    backend.set_clear_color(Color::BLACK);
    let ink = |backend: &mut WgpuBackend, weight: FontWeight| -> u64 {
        let mut ctx = PaintContext::new();
        ctx.draw_text_weighted(
            "D",
            Vec2::new(0.0, 24.0),
            32.0,
            weight,
            TextAlign::Left,
            Paint::new(Color::WHITE),
        );
        let pixels = render(backend, ctx, viewport(32.0, 32.0));
        let mut sum = 0u64;
        for y in 0..32 {
            for x in 0..32 {
                sum += pixels.pixel(x, y).map(|pixel| pixel[0] as u64).unwrap_or(0);
            }
        }
        sum
    };
    let regular = ink(&mut backend, FontWeight::NORMAL);
    let bold = ink(&mut backend, FontWeight::BOLD);
    assert!(
        bold > regular,
        "bold ink {bold} is not more than regular {regular}"
    );
}

#[test]
fn font_config_switches_to_pixel_mode() {
    let Some(mut backend) = backend() else {
        return;
    };
    backend
        .set_font_config(FontConfig {
            mode: FontMode::Pixel,
            device_pixel_rasterization: true,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(backend.font_config().mode, FontMode::Pixel);

    let metrics = backend.text_metrics();
    assert!(!metrics.is_system());
    // Pixel glyphs are scaled down from the em and rounded to whole pixels, so
    // they match a proportional font's visual size at the same `font_size`.
    let cell = (16.0 * PIXEL_GLYPH_RATIO).round();
    assert!((metrics.advance('i', 16.0) - cell).abs() < 1e-4);
    assert!((metrics.advance('W', 16.0) - cell).abs() < 1e-4);

    // Pixel mode still renders ink (CJK -> missing-glyph box).
    backend.set_clear_color(Color::BLACK);
    let mut ctx = PaintContext::new();
    ctx.draw_text(
        "中",
        Vec2::new(2.0, 24.0),
        20.0,
        TextAlign::Left,
        Paint::new(Color::WHITE),
    );
    let pixels = render(&mut backend, ctx, viewport(32.0, 32.0));
    assert!(pixels.data.chunks(4).any(|pixel| pixel[0] > 128));
}

#[test]
fn device_pixel_rasterization_inks_text_at_high_dpi() {
    let Some(mut backend) = backend() else {
        return;
    };
    if !backend.text_metrics().is_system() {
        return;
    }
    backend.set_scale_factor(2.0);
    backend.set_clear_color(Color::BLACK);

    let mut ctx = PaintContext::new();
    ctx.draw_text(
        "M",
        Vec2::new(4.0, 24.0),
        16.0,
        TextAlign::Left,
        Paint::new(Color::WHITE),
    );
    // Logical 32x32 -> device 64x64; the glyph is rasterized at 32px.
    let pixels = render(&mut backend, ctx, viewport(32.0, 32.0));
    assert_eq!(pixels.width, 64);
    assert!(pixels.data.chunks(4).any(|pixel| pixel[0] > 128));
}

#[test]
fn draw_text_renders_cjk_with_a_system_font() {
    let Some(mut backend) = backend() else {
        return;
    };
    // Only meaningful when a real (non-bitmap) font with CJK coverage loaded.
    if !backend.text_metrics().is_system() {
        eprintln!("skipping CJK test: no system font loaded");
        return;
    }
    backend.set_clear_color(Color::BLACK);

    let mut ctx = PaintContext::new();
    ctx.draw_text(
        "中",
        Vec2::new(2.0, 24.0),
        20.0,
        TextAlign::Left,
        Paint::new(Color::WHITE),
    );
    let pixels = render(&mut backend, ctx, viewport(32.0, 32.0));

    let inked = pixels.data.chunks(4).any(|pixel| pixel[0] > 128);
    assert!(inked, "expected CJK glyph ink from the system font");
}

#[test]
fn glyphs_snap_to_the_device_pixel_grid() {
    let Some(mut backend) = backend() else {
        return;
    };
    backend.set_clear_color(Color::BLACK);

    let render_at = |backend: &mut WgpuBackend, x: f32| {
        let mut ctx = PaintContext::new();
        ctx.draw_text(
            "M",
            Vec2::new(x, 24.0),
            16.0,
            TextAlign::Left,
            Paint::new(Color::WHITE),
        );
        render(backend, ctx, viewport(32.0, 32.0))
    };

    // Glyphs are rasterized on the device grid and sampled with nearest
    // filtering, so a sub-pixel offset must not resample them.
    let a = render_at(&mut backend, 6.0);
    let b = render_at(&mut backend, 6.25);
    assert_eq!(a.data, b.data, "a sub-pixel offset changed the glyph");

    // A whole-pixel shift does move it, so the check above is not vacuous.
    let c = render_at(&mut backend, 7.0);
    assert_ne!(a.data, c.data, "a full-pixel shift should move the glyph");
}

#[test]
fn pixel_font_maps_glyph_texels_one_to_one() {
    let Some(mut backend) = backend() else {
        return;
    };
    backend
        .set_font_config(FontConfig {
            mode: FontMode::Pixel,
            device_pixel_rasterization: true,
            ..Default::default()
        })
        .unwrap();
    backend.set_clear_color(Color::BLACK);

    // The cell is a fraction of the em, so pick the font size that yields an
    // 8px cell; `pixel_cell` rounds, so this is exactly 1:1. `_`, `|` and `A`
    // include ink in the first/last columns, which the old half-texel UV inset
    // used to drop.
    use font8x8::UnicodeFonts;
    let font_size = 8.0 / PIXEL_GLYPH_RATIO;
    for ch in ['L', '_', '|', 'A', 'W'] {
        let mut ctx = PaintContext::new();
        ctx.draw_text(
            &ch.to_string(),
            Vec2::new(0.0, 8.0),
            font_size,
            TextAlign::Left,
            Paint::new(Color::WHITE),
        );
        let pixels = render(&mut backend, ctx, viewport(8.0, 8.0));

        let glyph = font8x8::BASIC_FONTS.get(ch).expect("printable ASCII glyph");
        for (row, bits) in glyph.iter().enumerate() {
            for col in 0..8u32 {
                let on = pixels.pixel(col, row as u32).unwrap()[0] > 127;
                let expected = (bits >> col) & 1 == 1;
                assert_eq!(on, expected, "glyph {ch:?} pixel (col={col}, row={row})");
            }
        }
    }
}

#[test]
fn text_alignment_shifts_the_glyph() {
    let Some(mut backend) = backend() else {
        return;
    };
    backend.set_clear_color(Color::BLACK);

    let mut left = PaintContext::new();
    left.draw_text(
        "I",
        Vec2::new(16.0, 24.0),
        16.0,
        TextAlign::Left,
        Paint::new(Color::WHITE),
    );
    let left_pixels = render(&mut backend, left, viewport(32.0, 32.0));

    let mut right = PaintContext::new();
    right.draw_text(
        "I",
        Vec2::new(16.0, 24.0),
        16.0,
        TextAlign::Right,
        Paint::new(Color::WHITE),
    );
    let right_pixels = render(&mut backend, right, viewport(32.0, 32.0));

    assert_ne!(left_pixels.data, right_pixels.data);
}

#[test]
fn scene_tree_renders_through_the_backend_unchanged() {
    use draw_scene::{SceneTree, Visual};

    let Some(mut backend) = backend() else {
        return;
    };

    // A scene built with the same API the other backends consume.
    let mut tree = SceneTree::new();
    let root = tree.root();
    let node = tree.add_node2d(root, "Box");
    tree.set_position(node, Vec2::new(6.0, 6.0));
    tree.set_visual(
        node,
        Visual::Rect {
            size: Size::splat(10.0),
            color: Color::RED,
        },
    );
    tree.update();

    let mut ctx = PaintContext::new();
    tree.paint(&mut ctx);

    let pixels = render(&mut backend, ctx, viewport(32.0, 32.0));
    assert_pixel(&pixels, 10, 10, [255, 0, 0, 255]);
    assert_pixel(&pixels, 2, 2, [0, 0, 0, 0]);
}

#[test]
fn renders_into_an_external_texture_view() {
    let Some(mut backend) = backend() else {
        return;
    };
    let size = 16u32;
    // A `Bgra8Unorm` view mirrors a typical window-surface format, exercising
    // the per-format pipeline cache.
    let texture = backend.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("test.external"),
        size: wgpu::Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Bgra8Unorm,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&Default::default());

    let mut ctx = PaintContext::new();
    ctx.fill_rect(
        Rect::from_min_size(Vec2::ZERO, Size::splat(16.0)),
        Color::RED,
    );

    backend
        .begin_frame_with_view(
            view,
            size,
            size,
            wgpu::TextureFormat::Bgra8Unorm,
            viewport(16.0, 16.0),
        )
        .unwrap();
    assert!(!backend.is_offscreen_frame());
    backend.submit(&ctx.into_draw_list()).unwrap();
    backend.end_frame().unwrap();

    // Whatever the target format, `read_texture` returns the raw texel bytes;
    // for `Bgra8Unorm` that is B, G, R, A, so red reads back as [0, 0, 255, 255].
    let pixels = backend.read_texture(&texture, size, size).unwrap();
    assert_pixel(&pixels, 8, 8, [0, 0, 255, 255]);
}

#[test]
fn frame_lifecycle_reports_misuse() {
    let Some(mut backend) = backend() else {
        return;
    };
    assert!(backend.submit(&draw_render::DrawList::new()).is_err());
    assert!(backend.end_frame().is_err());

    backend.begin_frame(viewport(8.0, 8.0)).unwrap();
    assert!(backend.begin_frame(viewport(8.0, 8.0)).is_err());
    backend.end_frame().unwrap();
}
