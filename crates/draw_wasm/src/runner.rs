use std::cell::RefCell;
use std::rc::Rc;

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement, PointerEvent, Window};

use draw_backend_canvas::Canvas2dBackend;
use draw_core::{EventResult, InputEvent, Key, PointerButton, Size, Vec2, Viewport};
use draw_render::{PaintContext, RenderBackend};

/// Application hook driven by the WASM runner.
///
/// `update` receives the current logical viewport (already DPR-independent),
/// `paint` emits commands into a [`PaintContext`], and `event` receives input.
/// The runner handles canvas sizing, DPR, the `requestAnimationFrame` loop and
/// DOM input translation.
pub trait App {
    /// Called once with the canvas 2D context before the first frame.
    ///
    /// Override this to install a real text measurer (see
    /// [`CanvasTextMeasurer`](crate::CanvasTextMeasurer)) so layout baselines
    /// match what the backend draws.
    fn attach_context(&mut self, _ctx: &CanvasRenderingContext2d) {}

    /// Whether the pointer is currently over a clickable control.
    ///
    /// While this is `true` the runner sets the canvas CSS cursor to
    /// `pointer`; otherwise it resets to `default`. Evaluated every frame.
    fn pointer_cursor(&self) -> bool {
        false
    }

    fn update(&mut self, viewport: Viewport);
    fn paint(&mut self, ctx: &mut PaintContext);
    fn event(&mut self, _event: &InputEvent) -> EventResult {
        EventResult::Ignored
    }
}

/// Starts the render loop and input handling on the canvas element `canvas_id`.
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

    let app = Rc::new(RefCell::new(app));

    attach_pointer_listeners(&canvas, &app);
    attach_keyboard_listeners(&window, &app);

    app.borrow_mut().attach_context(&ctx);

    let mut backend = Canvas2dBackend::new(ctx);
    // Render one frame synchronously so the first pixels (and any probe data)
    // exist before `init()` resolves.
    render_frame(&app, &mut backend, &canvas, &window);

    // requestAnimationFrame loop. `f` is captured by its own closure, forming an
    // intentional cycle that keeps the loop alive for the page lifetime.
    let f: Rc<RefCell<Option<Closure<dyn FnMut()>>>> = Rc::new(RefCell::new(None));
    let g = f.clone();
    let f_for_loop = f.clone();
    let canvas_for_loop = canvas.clone();
    let window_for_loop = window.clone();

    *g.borrow_mut() = Some(Closure::wrap(Box::new(move || {
        render_frame(&app, &mut backend, &canvas_for_loop, &window_for_loop);

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

fn render_frame<A: App>(
    app: &Rc<RefCell<A>>,
    backend: &mut Canvas2dBackend,
    canvas: &HtmlCanvasElement,
    window: &Window,
) {
    let scale_factor = device_pixel_ratio(window) as f32;
    backend.set_scale_factor(scale_factor);

    apply_cursor(app, canvas);

    let viewport = Viewport::new(logical_size(canvas, window));
    let mut app = app.borrow_mut();
    app.update(viewport);

    let mut ctx = PaintContext::new();
    app.paint(&mut ctx);
    drop(app);
    let list = ctx.into_draw_list();

    let _ = backend.begin_frame(viewport);
    let _ = backend.submit(&list);
    let _ = backend.end_frame();
}

fn apply_cursor<A: App>(app: &Rc<RefCell<A>>, canvas: &HtmlCanvasElement) {
    let cursor = if app.borrow().pointer_cursor() {
        "pointer"
    } else {
        "default"
    };
    let _ = canvas.style().set_property("cursor", cursor);
}

fn attach_pointer_listeners<A: App + 'static>(canvas: &HtmlCanvasElement, app: &Rc<RefCell<A>>) {
    let down = {
        let app = app.clone();
        let canvas = canvas.clone();
        Closure::<dyn FnMut(PointerEvent)>::new(move |event: PointerEvent| {
            let event = InputEvent::PointerDown {
                position: pointer_position(&canvas, &event),
                button: pointer_button(event.button()),
            };
            app.borrow_mut().event(&event);
        })
    };
    let up = {
        let app = app.clone();
        let canvas = canvas.clone();
        Closure::<dyn FnMut(PointerEvent)>::new(move |event: PointerEvent| {
            let event = InputEvent::PointerUp {
                position: pointer_position(&canvas, &event),
                button: pointer_button(event.button()),
            };
            app.borrow_mut().event(&event);
        })
    };
    let movement = {
        let app = app.clone();
        let canvas = canvas.clone();
        Closure::<dyn FnMut(PointerEvent)>::new(move |event: PointerEvent| {
            let event = InputEvent::PointerMove {
                position: pointer_position(&canvas, &event),
            };
            app.borrow_mut().event(&event);
        })
    };
    let leave = {
        let app = app.clone();
        Closure::<dyn FnMut(PointerEvent)>::new(move |_event: PointerEvent| {
            app.borrow_mut().event(&InputEvent::PointerLeave);
        })
    };

    let _ = canvas.add_event_listener_with_callback("pointerdown", down.as_ref().unchecked_ref());
    let _ = canvas.add_event_listener_with_callback("pointerup", up.as_ref().unchecked_ref());
    let _ =
        canvas.add_event_listener_with_callback("pointermove", movement.as_ref().unchecked_ref());
    let _ = canvas.add_event_listener_with_callback("pointerleave", leave.as_ref().unchecked_ref());

    // The app lives for the page lifetime, so the closures are intentionally
    // leaked rather than stored.
    down.forget();
    up.forget();
    movement.forget();
    leave.forget();
}

fn attach_keyboard_listeners<A: App + 'static>(window: &Window, app: &Rc<RefCell<A>>) {
    let down = {
        let app = app.clone();
        Closure::<dyn FnMut(web_sys::KeyboardEvent)>::new(move |event: web_sys::KeyboardEvent| {
            app.borrow_mut().event(&InputEvent::KeyDown {
                key: key_from_event(&event),
            });
        })
    };
    let up = {
        let app = app.clone();
        Closure::<dyn FnMut(web_sys::KeyboardEvent)>::new(move |event: web_sys::KeyboardEvent| {
            app.borrow_mut().event(&InputEvent::KeyUp {
                key: key_from_event(&event),
            });
        })
    };

    let _ = window.add_event_listener_with_callback("keydown", down.as_ref().unchecked_ref());
    let _ = window.add_event_listener_with_callback("keyup", up.as_ref().unchecked_ref());
    down.forget();
    up.forget();
}

fn pointer_position(canvas: &HtmlCanvasElement, event: &PointerEvent) -> Vec2 {
    let rect = canvas.get_bounding_client_rect();
    let x = event.client_x() as f64 - rect.left();
    let y = event.client_y() as f64 - rect.top();
    Vec2::new(x as f32, y as f32)
}

fn pointer_button(button: i16) -> PointerButton {
    match button {
        1 => PointerButton::Middle,
        2 => PointerButton::Right,
        _ => PointerButton::Left,
    }
}

fn key_from_event(event: &web_sys::KeyboardEvent) -> Key {
    match event.key().as_str() {
        "Enter" => Key::Enter,
        "Escape" => Key::Escape,
        "Backspace" => Key::Backspace,
        "Delete" => Key::Delete,
        "Tab" => Key::Tab,
        " " => Key::Space,
        "Home" => Key::Home,
        "End" => Key::End,
        "ArrowUp" => Key::ArrowUp,
        "ArrowDown" => Key::ArrowDown,
        "ArrowLeft" => Key::ArrowLeft,
        "ArrowRight" => Key::ArrowRight,
        "F1" => Key::F1,
        "F2" => Key::F2,
        "F3" => Key::F3,
        "F4" => Key::F4,
        "F5" => Key::F5,
        "F6" => Key::F6,
        "F7" => Key::F7,
        "F8" => Key::F8,
        "F9" => Key::F9,
        "F10" => Key::F10,
        "F11" => Key::F11,
        "F12" => Key::F12,
        other => other
            .chars()
            .next()
            .map(Key::Character)
            .unwrap_or(Key::Space),
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
