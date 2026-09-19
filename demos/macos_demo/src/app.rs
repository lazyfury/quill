//! quill macOS Core Graphics demo.
//!
//! Two modes:
//! - `macos_demo --offscreen out.png` renders one frame headlessly and exits.
//! - `macos_demo` opens an AppKit window showing the live scene + UI.
//!
//! Both use the exact same `Scene`/`UI` -> `DrawList` -> `CoreGraphicsBackend`
//! path, with no browser or GPU dependency.

use std::cell::{Cell, RefCell};
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;
use std::ptr::NonNull;
use std::rc::Rc;

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::{AnyThread, MainThreadMarker, MainThreadOnly};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSBackingStoreType, NSImage, NSImageView,
    NSWindow, NSWindowStyleMask,
};
use objc2_foundation::{NSPoint, NSRect, NSSize, NSString, NSTimer};

use draw_backend_coregraphics::CoreGraphicsBackend;
use draw_core::{Color, Edges, NodeId, Rect, Size, Vec2, Viewport};
use draw_render::{DrawList, Paint, PaintContext, RenderBackend, TextAlign};
use draw_scene::{SceneTree, Visual};
use draw_ui::{Button, Label, Panel, Ui, VBox};

const BACKGROUND: Color = Color::new(0.09, 0.10, 0.13, 1.0);
const ACCENT: Color = Color::new(0.30, 0.62, 0.98, 1.0);
const WARN: Color = Color::new(0.98, 0.66, 0.25, 1.0);
const TEXT: Color = Color::new(0.92, 0.94, 0.98, 1.0);

/// Application state shared by both modes.
struct Demo {
    ui: Ui,
    status: NodeId,
    clicks: Rc<Cell<u32>>,
    scene: SceneTree,
    rotating: NodeId,
    time: f32,
}

impl Demo {
    fn new() -> Self {
        // UI: Panel { Label, Button, status } using the component API.
        let mut ui = Ui::new();
        let panel = ui.add(ui.root(), Panel::new());
        ui.set_anchors(panel.id(), Edges::new(1.0, 0.0, 1.0, 0.0));
        ui.set_offsets(panel.id(), Edges::new(-360.0, 40.0, -40.0, 320.0));
        let vbox = ui.add(panel.id(), VBox::new().separation(12.0));
        ui.add(vbox.id(), Label::new("Hello"));
        ui.add(
            vbox.id(),
            Label::new("native Core Graphics backend")
                .font_size(14.0)
                .color(Color::new(0.70, 0.75, 0.85, 1.0)),
        );
        let clicks = Rc::new(Cell::new(0));
        let counter = clicks.clone();
        ui.add(
            vbox.id(),
            Button::new("Click me").on_click(move || counter.set(counter.get() + 1)),
        );
        let status = ui.add(vbox.id(), Label::new("Status: Clicked 0 times"));

        // Scene: a rotated Node2D with a child.
        let mut scene = SceneTree::new();
        let root = scene.root();
        let rotating = scene.add_node2d(root, "Rotating");
        scene.set_visual(
            rotating,
            Visual::Rect {
                size: Size::new(120.0, 80.0),
                color: ACCENT,
            },
        );
        let child = scene.add_node2d(rotating, "Child");
        scene.set_position(child, Vec2::new(80.0, 0.0));
        scene.set_visual(
            child,
            Visual::Circle {
                radius: 16.0,
                color: WARN,
            },
        );
        scene.update();

        Self {
            ui,
            status: status.id(),
            clicks,
            scene,
            rotating,
            time: 0.0,
        }
    }

    fn frame(&mut self, viewport: Viewport) -> DrawList {
        self.time += 0.016;
        let size = viewport.logical_size();
        self.scene.set_position(
            self.rotating,
            Vec2::new(size.width * 0.30, size.height * 0.42),
        );
        self.scene.set_rotation(self.rotating, self.time);
        self.scene.update();

        self.ui.set_text(
            self.status,
            format!("Status: Clicked {} times", self.clicks.get()),
        );
        self.ui.layout(viewport);

        let mut ctx = PaintContext::new();
        ctx.fill_rect(Rect::from_min_size(Vec2::ZERO, size), BACKGROUND);
        self.scene.paint(&mut ctx);
        self.ui.paint(&mut ctx);
        ctx.draw_text(
            "Scene / Node2D Demo",
            Vec2::new(40.0, 60.0),
            18.0,
            TextAlign::Left,
            Paint::new(TEXT.with_alpha(0.85)),
        );
        ctx.draw_text(
            "quill - macOS Core Graphics",
            Vec2::new(size.width * 0.5, size.height - 32.0),
            20.0,
            TextAlign::Center,
            Paint::new(TEXT),
        );
        ctx.into_draw_list()
    }
}

pub fn main() -> Result<(), String> {
    let mut offscreen: Option<String> = None;
    let mut selftest = false;
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut index = 0;
    while index < args.len() {
        if args[index] == "--offscreen" {
            index += 1;
            offscreen = Some(args.get(index).cloned().ok_or("--offscreen needs a path")?);
        } else if args[index] == "--selftest" {
            selftest = true;
        } else if args[index] == "--help" {
            println!("usage: macos_demo [--offscreen <out.png> | --selftest]");
            return Ok(());
        }
        index += 1;
    }

    match offscreen {
        Some(path) => render_offscreen(&path),
        None => run_window(selftest),
    }
}

fn render_offscreen(path: &str) -> Result<(), String> {
    let mut demo = Demo::new();
    let viewport = Viewport::new(Size::new(900.0, 600.0));
    let list = demo.frame(viewport);

    let mut backend = CoreGraphicsBackend::new();
    backend.set_scale_factor(2.0);
    backend
        .begin_frame(viewport)
        .map_err(|error| format!("begin_frame: {error:?}"))?;
    backend
        .submit(&list)
        .map_err(|error| format!("submit: {error:?}"))?;
    backend
        .end_frame()
        .map_err(|error| format!("end_frame: {error:?}"))?;

    write_png(Path::new(path), &backend).map_err(|error| format!("write png: {error}"))?;
    println!("wrote {path}");
    Ok(())
}

fn write_png(path: &Path, backend: &CoreGraphicsBackend) -> std::io::Result<()> {
    let (width, height) = backend.pixel_size();
    let pixels = backend.pixels();

    // Convert premultiplied BGRA to straight RGBA for PNG.
    let mut rgba = Vec::with_capacity(width * height * 4);
    for pixel in pixels.chunks_exact(4) {
        let (b, g, r, a) = (pixel[0], pixel[1], pixel[2], pixel[3]);
        if a == 0 {
            rgba.extend_from_slice(&[0, 0, 0, 0]);
        } else if a == 255 {
            rgba.extend_from_slice(&[r, g, b, a]);
        } else {
            let inv = 255.0 / a as f32;
            let un = |v: u8| (v as f32 * inv).min(255.0) as u8;
            rgba.extend_from_slice(&[un(r), un(g), un(b), a]);
        }
    }

    let file = File::create(path)?;
    let mut encoder = png::Encoder::new(BufWriter::new(file), width as u32, height as u32);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder
        .write_header()
        .map_err(|error| std::io::Error::other(error.to_string()))?;
    writer
        .write_image_data(&rgba)
        .map_err(|error| std::io::Error::other(error.to_string()))?;
    Ok(())
}

fn run_window(selftest: bool) -> Result<(), String> {
    let mtm = MainThreadMarker::new().ok_or("must run on the main thread")?;
    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Regular);

    let content = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(900.0, 600.0));
    let style =
        NSWindowStyleMask::Titled | NSWindowStyleMask::Closable | NSWindowStyleMask::Resizable;
    let window = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
            NSWindow::alloc(mtm),
            content,
            style,
            NSBackingStoreType::Buffered,
            false,
        )
    };
    window.setTitle(&NSString::from_str("quill — macOS Core Graphics backend"));
    window.center();

    let image_view = NSImageView::initWithFrame(NSImageView::alloc(mtm), content);
    image_view.setImageScaling(objc2_app_kit::NSImageScaling::ScaleProportionallyUpOrDown);
    window.setContentView(Some(&image_view));
    window.makeKeyAndOrderFront(None);
    app.activate();

    let demo = Rc::new(RefCell::new(Demo::new()));
    let backend = RefCell::new(CoreGraphicsBackend::new());
    let view: Retained<NSImageView> = image_view.clone();
    let reported = Rc::new(Cell::new(!selftest));

    // NSTimer retains the block; the local binding keeps it alive across run().
    let block = {
        let demo = demo.clone();
        let view = view.clone();
        let app = app.clone();
        let reported = reported.clone();
        RcBlock::new(move |_timer: NonNull<NSTimer>| {
            let bounds = view.bounds();
            let width = bounds.size.width as f32;
            let height = bounds.size.height as f32;
            if width < 1.0 || height < 1.0 {
                return;
            }
            let viewport = Viewport::new(Size::new(width, height));

            let mut demo = demo.borrow_mut();
            let list = demo.frame(viewport);

            let mut backend = backend.borrow_mut();
            backend.set_scale_factor(window_backing_scale(&view) as f32);
            if backend.begin_frame(viewport).is_err() {
                return;
            }
            let _ = backend.submit(&list);
            let _ = backend.end_frame();

            if let Some(cg_image) = backend.image() {
                let image = NSImage::initWithCGImage_size(
                    NSImage::alloc(),
                    &cg_image,
                    NSSize::new(width as f64, height as f64),
                );
                view.setImage(Some(&image));
            }

            if selftest && !reported.replace(true) {
                let ok = frame_looks_rendered(&backend);
                println!("WINDOW_SELFTEST {}", if ok { "OK" } else { "FAIL" });
                let _ = std::io::Write::flush(&mut std::io::stdout());
                app.terminate(None);
            }
        })
    };

    let _timer =
        unsafe { NSTimer::scheduledTimerWithTimeInterval_repeats_block(1.0 / 60.0, true, &block) };

    app.run();
    Ok(())
}

/// Checks that the live window frame actually rendered the scene.
fn frame_looks_rendered(backend: &CoreGraphicsBackend) -> bool {
    let mut background = 0usize;
    let mut accent = 0usize;
    for pixel in backend.pixels().chunks_exact(4) {
        let (b, g, r) = (pixel[0], pixel[1], pixel[2]);
        if r == 23 && g == 26 && b == 33 {
            background += 1;
        } else if r == 77 && g == 158 && b == 250 {
            accent += 1;
        }
    }
    background > 1000 && accent > 100
}

fn window_backing_scale(view: &NSImageView) -> f64 {
    view.window()
        .map(|window| window.backingScaleFactor())
        .unwrap_or(1.0)
}
