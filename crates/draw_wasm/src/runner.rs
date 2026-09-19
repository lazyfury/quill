use std::cell::RefCell;
use std::rc::Rc;

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement, Window};

use draw_backend_canvas::Canvas2dBackend;
use draw_core::{Size, Viewport};
use draw_render::{PaintContext, RenderBackend};

/// Application hook driven by the WASM runner.
///
/// `update` receives the current logical viewport (already DPR-independent), and
/// `paint` emits commands into a [`PaintContext`]. The runner handles canvas
/// sizing, DPR and the `requestAnimationFrame` loop.
pub trait App {
    fn update(&mut self, viewport: Viewport);
    fn paint(&mut self, ctx: &mut PaintContext);
}

/// Starts the render loop on the canvas element with the given `id`.
///
/// Sizes the canvas each frame from its CSS size (falling back to the window),
/// applies `devicePixelRatio`, then runs `update` + `paint` + flush.
pub fn start<A>(canvas_id: &str, app: A) -> Result<(), JsValue>
where
    A: App + 'static,
{
    let window = web_sys::window().ok_or_else(|| JsValue::from_str("no window"))?;
    let document = window
        .document()
        .ok_or_else(|| JsValue::from_str("no document"))?;
    let canvas = document
        .get_element_by_id(canvas_id)
        .ok_or_else(|| JsValue::from_str("canvas element not found"))?
        .dyn_into::<HtmlCanvasElement>()?;
    let ctx = canvas
        .get_context("2d")?
        .ok_or_else(|| JsValue::from_str("2d context unavailable"))?
        .dyn_into::<CanvasRenderingContext2d>()?;

    let mut backend = Canvas2dBackend::new(ctx);
    let mut app = app;
    let canvas_for_loop = canvas.clone();
    let window_for_loop = window.clone();

    let f: Rc<RefCell<Option<Closure<dyn FnMut()>>>> = Rc::new(RefCell::new(None));
    let g = f.clone();
    let f_for_loop = f.clone();

    *g.borrow_mut() = Some(Closure::wrap(Box::new(move || {
        let scale_factor = device_pixel_ratio(&window_for_loop) as f32;
        backend.set_scale_factor(scale_factor);

        let viewport = Viewport::new(logical_size(&canvas_for_loop, &window_for_loop));
        app.update(viewport);

        let mut ctx = PaintContext::new();
        app.paint(&mut ctx);
        let list = ctx.into_draw_list();

        let _ = backend.begin_frame(viewport);
        let _ = backend.submit(&list);
        let _ = backend.end_frame();

        if let Some(callback) = f_for_loop.borrow().as_ref() {
            let _ = window_for_loop.request_animation_frame(callback.as_ref().unchecked_ref());
        }
    }) as Box<dyn FnMut()>));

    let initial = f
        .borrow()
        .as_ref()
        .map(|callback| window.request_animation_frame(callback.as_ref().unchecked_ref()));
    match initial {
        Some(Ok(_)) => Ok(()),
        _ => Err(JsValue::from_str("failed to schedule animation frame")),
    }
}

fn device_pixel_ratio(window: &Window) -> f64 {
    let dpr = window.device_pixel_ratio();
    if dpr.is_finite() && dpr > 0.0 {
        dpr
    } else {
        1.0
    }
}

fn logical_size(canvas: &HtmlCanvasElement, window: &Window) -> Size {
    let width = canvas.client_width();
    let height = canvas.client_height();
    if width > 0 && height > 0 {
        return Size::new(width as f32, height as f32);
    }
    let width = window
        .inner_width()
        .ok()
        .and_then(|value| value.as_f64())
        .unwrap_or(800.0);
    let height = window
        .inner_height()
        .ok()
        .and_then(|value| value.as_f64())
        .unwrap_or(600.0);
    Size::new(width as f32, height as f32)
}
