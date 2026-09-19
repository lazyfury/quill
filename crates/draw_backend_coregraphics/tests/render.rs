#![cfg(target_os = "macos")]

use draw_backend_coregraphics::CoreGraphicsBackend;
use draw_core::{Color, Rect, Size, Vec2, Viewport};
use draw_render::{PaintContext, RenderBackend, TextAlign};

fn pixel(backend: &CoreGraphicsBackend, x: usize, y: usize) -> [u8; 4] {
    let (width, _) = backend.pixel_size();
    let index = (y * width + x) * 4;
    let p = backend.pixels();
    [p[index], p[index + 1], p[index + 2], p[index + 3]]
}

#[test]
fn renders_shapes_and_text() {
    let mut backend = CoreGraphicsBackend::new();
    backend.set_scale_factor(1.0);

    let viewport = Viewport::new(Size::new(120.0, 120.0));
    backend.begin_frame(viewport).unwrap();

    let mut ctx = PaintContext::new();
    ctx.fill_rect(
        Rect::from_min_size(Vec2::ZERO, Size::new(60.0, 60.0)),
        Color::RED,
    );
    ctx.fill_circle(Vec2::new(90.0, 90.0), 12.0, Color::GREEN);
    ctx.draw_text(
        "Hi",
        Vec2::new(6.0, 110.0),
        18.0,
        TextAlign::Left,
        Color::WHITE,
    );
    let list = ctx.into_draw_list();

    backend.submit(&list).unwrap();
    backend.end_frame().unwrap();

    assert_eq!(backend.pixel_size(), (120, 120));

    // Premultiplied BGRA: [B, G, R, A].
    let red = pixel(&backend, 10, 10);
    assert!(red[2] > 200 && red[0] < 40 && red[1] < 40, "red {red:?}");

    let green = pixel(&backend, 90, 90);
    assert!(
        green[1] > 200 && green[0] < 40 && green[2] < 40,
        "green {green:?}"
    );

    let background = pixel(&backend, 115, 5);
    assert_eq!(background, [0, 0, 0, 0], "background {background:?}");

    // Text should light up pixels near the baseline.
    let mut lit = 0;
    for y in 96..116 {
        for x in 0..40 {
            let p = pixel(&backend, x, y);
            if p[3] > 40 && p[0] > 60 && p[1] > 60 && p[2] > 60 {
                lit += 1;
            }
        }
    }
    assert!(lit > 5, "expected text pixels, found {lit}");
}

#[test]
fn transform_and_clip_are_applied() {
    use draw_core::Transform2D;

    let mut backend = CoreGraphicsBackend::new();
    backend.set_scale_factor(1.0);
    backend
        .begin_frame(Viewport::new(Size::new(50.0, 50.0)))
        .unwrap();

    let mut ctx = PaintContext::new();
    // ClipRect is in viewport/logical space (post-transform).
    ctx.clip_rect(Rect::from_min_size(
        Vec2::new(25.0, 25.0),
        Size::new(10.0, 10.0),
    ));
    ctx.set_transform(Transform2D::from_translation(Vec2::new(20.0, 20.0)));
    // Fills viewport 20..30; the clip keeps only 25..30.
    ctx.fill_rect(
        Rect::from_min_size(Vec2::ZERO, Size::new(10.0, 10.0)),
        Color::RED,
    );
    let list = ctx.into_draw_list();

    backend.submit(&list).unwrap();
    backend.end_frame().unwrap();

    // Inside the clip overlap: red.
    let inside = pixel(&backend, 27, 27);
    assert!(inside[2] > 200, "inside {inside:?}");
    // Inside the fill but outside the clip: untouched.
    let clipped = pixel(&backend, 22, 22);
    assert_eq!(clipped, [0, 0, 0, 0], "clipped {clipped:?}");
    // Outside the fill: untouched.
    let outside = pixel(&backend, 32, 32);
    assert_eq!(outside, [0, 0, 0, 0], "outside {outside:?}");
}
