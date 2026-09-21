//! The native host: the window (or the menu-bar panel), the wgpu surface, the
//! frame loop, and the worker thread that queries the endpoint.
//!
//! This is the only file that owns a window or a GPU device, mirroring
//! `examples/wgpu_demo/src/app.rs` (which is the reference for this pattern):
//!
//! ```text
//! winit events -> InputEvent -> BalanceApp -> DrawList -> WgpuBackend -> surface
//! ```
//!
//! ## Two faces, one view
//!
//! * **Menu bar** (the default on macOS): a status item built by
//!   [`crate::menubar`], no Dock icon (`ActivationPolicy::Accessory`), and the
//!   panel is a borderless always-on-top window that drops out of the item.
//!   Starting up creates no window at all — the app is the status item until
//!   you click it.
//! * **Window** (`--window`, and the only option off macOS): the plain window.
//!
//! [`BalanceApp`] does not know which one it is drawing into.
//!
//! ## Why a worker thread
//!
//! A balance query is a blocking HTTP call, and the event loop only paints when
//! something changes ([`ControlFlow::Wait`]). Doing the request inline would
//! freeze the UI, so the host:
//!
//! 1. reads the app's pending refresh request after the frame (or after
//!    [`App::tick`], the state half of a frame, when no window is on screen),
//! 2. runs [`api::fetch`] on a thread,
//! 3. posts the result back as a [`UserEvent`], which wakes the loop,
//! 4. hands it to [`App::apply_result`], which gives that one reply to every
//!    window that shows it — [`BalanceApp::apply_result`] for the panel or the
//!    window, [`BadgeApp::apply_result`] for the badge — mirrors the new total
//!    into the status item, and asks for a redraw.
//!
//! One request, one result, every window: the badge is a *view* of the same
//! fetch, never a second data source with a schedule of its own.
//!
//! The UI therefore stays responsive while the request is in flight.

use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use draw_backend_wgpu::{wgpu, FontConfig, FontMetrics, FontMode, WgpuBackend};
use draw_core::{InputEvent, Key, PointerButton, Rect, Size, Vec2, ViewportSize};
use draw_render::{PaintContext, RenderBackend};
use draw_theme::Theme;
use draw_ui::TextMeasurer;
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition};
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::keyboard::{Key as WinitKey, NamedKey};
use winit::monitor::MonitorHandle;
use winit::platform::macos::WindowAttributesExtMacOS;
use winit::window::{CursorIcon, Window, WindowId, WindowLevel};

use crate::api::{self, Balance};
use crate::badge::{self, BadgeApp};
use crate::go::{self, GoUsage};
use crate::settings;
use crate::ui::{BalanceApp, Tab, ARROW_HEIGHT};

#[cfg(target_os = "macos")]
use crate::menubar;

/// Window title (also the tooltip's fallback).
const TITLE: &str = "DeepSeek 余额";
/// Logical size of the plain window (`--window`).
const WINDOW_WIDTH: f32 = 560.0;
const WINDOW_HEIGHT: f32 = 500.0;
/// Logical size of the menu-bar panel's body. Narrower than the window on
/// purpose: a panel hangs off a status item rather than sitting in the middle of
/// a screen.
const PANEL_WIDTH: f32 = 300.0;
/// The body's height for the usual content: endpoint, status line, one currency
/// card and the footer (hint, countdown/debug row and the refresh button). A
/// second card is rare and taller than the panel — it was always clipped — so
/// the footer is kept compact to leave the button on screen.
const PANEL_HEIGHT: f32 = 470.0;
/// Gap between the status item and the top of the panel window, in logical
/// pixels. Zero: the window's top edge *is* the arrow's tip, so the wedge
/// touches the menu bar the way a system popover's does.
const PANEL_GAP: f32 = 0.0;
/// Stand-in status-item geometry for the moment before AppKit has laid the menu
/// bar out — `(width, right inset, height)` in logical pixels. See
/// [`is_laid_out`].
const FALLBACK_ITEM: (f32, f32, f32) = (30.0, 8.0, 24.0);
/// The panel window's logical size: the body plus the popover arrow above it.
///
/// The arrow is part of the window, not a decoration around it — the view
/// paints it, and the window has to be tall enough and transparent around it.
fn panel_size() -> Size {
    Size::new(PANEL_WIDTH, PANEL_HEIGHT + ARROW_HEIGHT)
}

/// The same, in physical pixels for a surface at `scale`.
fn panel_device_size(scale: f32) -> Size {
    let size = panel_size();
    Size::new(size.width * scale, size.height * scale)
}

/// Tallest a menu bar is allowed to be, in logical pixels, when judging whether
/// a reported status-item rectangle is in the menu bar at all. macOS draws 24
/// points, 29 with a notch-less "Liquid Glass" bar; anything further down the
/// screen is a window AppKit has not placed yet.
#[cfg(target_os = "macos")]
const MENU_BAR_MAX: f32 = 40.0;
/// Menu-bar refresh interval when `--every` is not given.
const DEFAULT_REFRESH_SECS: u64 = 60 * 5;
/// How often the countdown line is repainted while the panel is open.
///
/// The line reads out whole seconds, so a coarser tick would skip numbers and a
/// finer one would wake the loop for nothing. A closed panel has no countdown to
/// show, and then the loop sleeps until the refresh itself is due.
#[cfg(target_os = "macos")]
const COUNTDOWN_TICK: Duration = Duration::from_secs(1);
/// How long after a panel hides itself a status-item click is ignored. See
/// [`App::toggle_panel`].
#[cfg(target_os = "macos")]
const TOGGLE_GUARD: Duration = Duration::from_millis(250);

/// Which surface the tool drives.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
enum Mode {
    /// macOS only: a status-bar item, with the balance panel as a popup.
    MenuBar,
    /// A plain window.
    Window,
}

impl Mode {
    /// The menu bar is the default **on macOS only**: `NSStatusBar` has no
    /// equivalent on the other platforms, where `--window` is the only option.
    fn default_for(window_flag: bool) -> Self {
        #[cfg(target_os = "macos")]
        if !window_flag {
            return Self::MenuBar;
        }
        let _ = window_flag;
        Self::Window
    }
}

/// What the command line asked for.
pub struct Options {
    /// Render with the light palette instead of the default dark one.
    pub light: bool,
    /// Use the built-in bitmap font instead of the system font (no CJK).
    pub pixel_font: bool,
    /// Exit after this many frames (a smoke test for the real pipeline).
    pub frames: Option<u32>,
    /// Exit as soon as the first balance reply has been applied, printing what
    /// landed. Verifies the whole window + fetch + apply path without a
    /// screenshot.
    pub until_result: bool,
    /// Use the plain window instead of the macOS menu-bar item.
    pub window: bool,
    /// Seconds between automatic refreshes. `None` picks the default
    /// ([`DEFAULT_REFRESH_SECS`] in menu-bar mode, no timer in window mode) and
    /// `Some(0)` turns the timer off.
    pub every: Option<u64>,
    /// Open the second window (the borderless badge in the desktop's
    /// bottom-right corner) alongside the mode's own surface. The multi-window
    /// test: one event loop, two windows, two surfaces, two backends.
    ///
    /// On by default — see `parse` in `main.rs`; `--badge` only says it out loud.
    pub badge: bool,
    /// Fewest seconds between two refresh starts (`--min-gap`). `None` keeps the
    /// view's own default; `Some(0)` turns the throttle off, which is what a
    /// scripted run that wants every request to go out asks for.
    pub min_gap: Option<u64>,
}

/// Work finished off-thread, delivered back onto the UI thread.
pub enum UserEvent {
    /// A finished balance query.
    Balance(Result<Balance, String>),
    /// A finished OpenCode Go usage query.
    GoUsage(Result<GoUsage, String>),
    /// A click on the status item (macOS menu-bar mode).
    #[cfg(target_os = "macos")]
    Tray(tray_icon::TrayIconEvent),
    /// A status-item menu action (macOS menu-bar mode).
    #[cfg(target_os = "macos")]
    Menu(tray_icon::menu::MenuEvent),
}

/// Opens the surface and runs until it is closed.
pub fn run(options: Options) {
    let mut builder = EventLoop::<UserEvent>::with_user_event();

    #[cfg(target_os = "macos")]
    {
        use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};
        // `Accessory`: a menu-bar app has no Dock icon and no app menu. That is
        // the whole point of a background readout.
        //
        // `activate_ignoring_other_apps(false)` keeps the launch polite: without
        // it, starting the tool from a terminal would yank focus away from the
        // terminal and hand it to an app that has no window to show. Opening the
        // panel still activates us, because `Window::focus_window` activates
        // explicitly.
        builder
            .with_activation_policy(ActivationPolicy::Accessory)
            .with_activate_ignoring_other_apps(false);
    }

    let event_loop = builder.build().expect("create event loop");
    // Event-driven: paint only when something changed (input, resize, a reply
    // arriving). `Poll` would redraw an unchanged frame as fast as possible.
    event_loop.set_control_flow(ControlFlow::Wait);
    let proxy = event_loop.create_proxy();
    let mut app = App::new(options, proxy);
    event_loop.run_app(&mut app).expect("run event loop");
}

/// Where the panel's anchor geometry came from. Best evidence first; only a
/// guess is worth retrying once the run loop has laid the menu bar out.
#[cfg(target_os = "macos")]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Source {
    /// The pointer of a click on the item — the item was under it.
    Click,
    /// `tray-icon`'s reading of the item's window, in the menu bar strip.
    Polled,
    /// Nothing usable: the monitor's top-right corner is used instead.
    Fallback,
}

#[cfg(target_os = "macos")]
impl Source {
    fn label(self) -> &'static str {
        match self {
            Self::Click => "点击位置",
            Self::Polled => "轮询",
            Self::Fallback => "兜底",
        }
    }
}

/// The earlier of the two deadlines the run loop is waiting on, or `None` to
/// park in [`ControlFlow::Wait`] until an event arrives.
///
/// Two things on screen tick: the footer's countdown to the next automatic
/// refresh, and the refresh button counting its cooldown down. While either is
/// visible the loop wakes on the second — unless the refresh is due sooner, which
/// comes first — because a countdown that only moves on unrelated events reads as
/// frozen. A closed panel has neither, so it sleeps straight through to the next
/// refresh; and with no timer at all (`--every 0`) it sleeps until an event.
///
/// Both deadlines are *absolute* instants, and that is the whole point: see
/// [`next_tick`].
#[cfg(target_os = "macos")]
fn wake_at(refresh_due: Option<Instant>, tick_due: Option<Instant>) -> Option<Instant> {
    match (refresh_due, tick_due) {
        (Some(refresh), Some(tick)) => Some(refresh.min(tick)),
        (Some(at), None) | (None, Some(at)) => Some(at),
        (None, None) => None,
    }
}

/// The countdown's next step: the stored deadline while it is still ahead, and
/// one tick from now only once it has come due.
///
/// The `now + COUNTDOWN_TICK` must not be recomputed on every event batch. The
/// loop on macOS wakes many times a second for reasons of its own, and a
/// deadline that is pushed forward on every one of those passes is never reached
/// — the countdown timer then silently never fires and the loop is left doing
/// nothing but burning wakeups. Advancing only a due deadline is what keeps the
/// wake-up rate at the rate the countdown actually needs.
#[cfg(target_os = "macos")]
fn next_tick(now: Instant, tick_at: Option<Instant>) -> Instant {
    match tick_at {
        Some(at) if at > now => at,
        _ => now + COUNTDOWN_TICK,
    }
}

/// Sorts `Focused(false)` window events into "the user clicked away" and "the
/// platform is talking to itself".
///
/// `winit` queues a synthetic `Focused(false)` the moment a window is created —
/// "XXX Send `Focused(false)` right after creating the window delegate, so we
/// won't obscure the real focused events on the startup"
/// (`platform_impl/macos/window_delegate.rs`) — and for a popup that is created
/// lazily on its first click, that event lands *after* the panel is already up.
/// Treating it as a dismissal is what made the first click on the status item
/// look like it did nothing while the second one worked.
#[cfg(target_os = "macos")]
#[derive(Default)]
struct FocusFlap {
    /// Synthetic events queued by a window creation and not seen yet.
    pending: u32,
}

#[cfg(target_os = "macos")]
impl FocusFlap {
    /// Records that a window is about to be created, which queues one event.
    ///
    /// Call this *before* creating the window: the event can be delivered either
    /// side of the creation call, depending on where `winit` is in its dispatch.
    fn window_created(&mut self) {
        self.pending += 1;
    }

    /// Whether this `Focused(false)` is the panel losing the user's attention.
    fn is_dismissal(&mut self) -> bool {
        if self.pending > 0 {
            self.pending -= 1;
            return false;
        }
        true
    }
}

/// Menu-bar-mode state, in one block so the rest of the host needs no `cfg`
/// sprinkles around individual fields.
#[cfg(target_os = "macos")]
#[derive(Default)]
struct MenuBarState {
    /// The status item, once the loop is running.
    item: Option<menubar::MenuBar>,
    /// Whether the panel is on screen.
    open: bool,
    /// Set when the panel opened before the menu bar was laid out, so it should
    /// be placed again on the next event batch.
    reposition: bool,
    /// When the panel last hid itself (`Focused(false)`).
    closed_at: Option<Instant>,
    /// Which `Focused(false)` events are dismissals. See [`FocusFlap`].
    focus_flap: FocusFlap,
    /// Where the last click on the item landed. See [`TrayAnchor`].
    anchor: Option<TrayAnchor>,
    /// Automatic refresh interval, if the timer is on.
    interval: Option<Duration>,
    /// Whether the automatic timer is held (the menu's `暂停自动刷新`).
    ///
    /// Pausing only stops the *timer*: a manual refresh (the item's `刷新余额`,
    /// the panel's button, `R`/`F5`) still goes out, because that is the whole
    /// point of asking for one while the timer sleeps.
    paused: bool,
    /// When the timer next fires.
    next: Option<Instant>,
    /// When the loop next has to come back for the on-screen countdowns.
    ///
    /// Kept as a deadline rather than recomputed per batch — see [`next_tick`].
    tick_at: Option<Instant>,
    /// The last status-item rectangle reported, so the trace only speaks when
    /// it changes.
    last_tray: Option<Rect>,
}

/// Where the status item was when it was last clicked.
///
/// This is the anchor the panel is placed against, because AppKit's answer for
/// "where is the item" goes stale — see [`crate::menubar`]. The click cannot
/// lie: the pointer had to be inside the item for the click to be delivered.
#[cfg(target_os = "macos")]
#[derive(Clone, Copy, Debug)]
struct TrayAnchor {
    /// The pointer at click time. While the status item reports a stale
    /// rectangle this is the item's centre within half an icon's width; once
    /// the reading catches up it also marks that the reading is the real one
    /// (the click fell inside it).
    pointer: Vec2,
}

/// Adapts the backend's font metrics to the layout engine, so measured text
/// matches rendered text (the same adapter `wgpu_demo` uses).
struct BackendTextMeasurer {
    metrics: FontMetrics,
}

impl TextMeasurer for BackendTextMeasurer {
    fn advance(&self, ch: char, font_size: f32) -> f32 {
        self.metrics.advance(ch, font_size)
    }

    fn line_height(&self, font_size: f32) -> f32 {
        self.metrics.line_height(font_size)
    }

    fn ascent(&self, font_size: f32) -> f32 {
        self.metrics.ascent(font_size)
    }

    fn measure_run(&self, text: &str, font_size: f32) -> f32 {
        self.metrics.measure_run(text, font_size)
    }
}

/// Whether a text slot holds anything (the view's error lines start empty).
fn has_text(text: Option<&str>) -> bool {
    text.is_some_and(|text| !text.is_empty())
}

/// The badge title and value prefix for `tab`: the two surfaces that echo the
/// active page must agree on which one it is.
fn badge_labels(tab: Tab) -> (&'static str, &'static str) {
    match tab {
        Tab::DeepSeek => (badge::TITLE, badge::BALANCE_PREFIX),
        Tab::Go => (badge::GO_TITLE, badge::GO_PREFIX),
    }
}

/// Owns the window, surface, backend and view state.
struct App {
    instance: wgpu::Instance,
    /// The window mode's main window, or the menu-bar mode's panel. `None`
    /// until it is first needed — a menu-bar app starts with no window at all.
    window: Option<Arc<Window>>,
    surface: Option<wgpu::Surface<'static>>,
    backend: Option<WgpuBackend>,
    config: Option<wgpu::SurfaceConfiguration>,
    scale_factor: f64,
    cursor: Vec2,
    view: BalanceApp,
    font_mode: FontMode,
    last_frame: Instant,
    /// Wakes the event loop when a worker thread has a result.
    proxy: EventLoopProxy<UserEvent>,
    /// Frames left before exiting (`--frames`), `None` for a normal session.
    frames_left: Option<u32>,
    /// Exit once the first reply has been applied (`--until-result`).
    exit_on_result: bool,
    mode: Mode,
    /// Whether the run is non-interactive, so `--frames` / `--until-result`
    /// print a trace instead of staying quiet.
    self_check: bool,
    /// Frames presented so far, used to report the first one once.
    frames_drawn: u32,
    /// The second window (`--badge`), created at startup next to whatever the
    /// mode itself opens. Its own surface and backend; input events for it are
    /// routed by `WindowId` and ignored otherwise.
    badge: Option<Badge>,
    /// Badge frames presented so far, so the first one can report itself.
    badge_frames: u32,
    badge_enabled: bool,
    #[cfg(target_os = "macos")]
    menu: MenuBarState,
}

/// The borderless badge window of the multi-window test, with everything it
/// needs to paint: its own surface, backend, surface configuration and
/// component view ([`BadgeApp`]), exactly like the main window's set but never
/// shared with it.
struct Badge {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    backend: WgpuBackend,
    config: wgpu::SurfaceConfiguration,
    /// The view: one `Column` with two `Text` lines, laid out and painted
    /// through `draw_ui` like any other view in this repo. It paints no
    /// backdrop, so the window stays transparent around the text.
    view: BadgeApp,
}

impl App {
    fn new(options: Options, proxy: EventLoopProxy<UserEvent>) -> Self {
        let theme = if options.light {
            Theme::light()
        } else {
            Theme::dark()
        };
        let mode = Mode::default_for(options.window);

        #[cfg(target_os = "macos")]
        let interval = match options.every {
            Some(0) => None,
            Some(seconds) => Some(Duration::from_secs(seconds)),
            None if mode == Mode::MenuBar => Some(Duration::from_secs(DEFAULT_REFRESH_SECS)),
            None => None,
        };

        let mut view = match mode {
            Mode::MenuBar => BalanceApp::new_panel(theme, api::endpoint()),
            Mode::Window => BalanceApp::new(theme, api::endpoint()),
        };
        // The throttle belongs to the view, so an absent flag leaves the view's
        // own default in place.
        if let Some(seconds) = options.min_gap {
            view.set_min_refresh_gap(Duration::from_secs(seconds));
        }
        // Open on whichever tab was last used; a missing or bad settings file
        // just leaves the view's own default (DeepSeek).
        let last_tab = settings::load().unwrap_or(Tab::DeepSeek);
        view.select_tab(last_tab);

        Self {
            instance: wgpu::Instance::default(),
            window: None,
            surface: None,
            backend: None,
            config: None,
            scale_factor: 1.0,
            cursor: Vec2::ZERO,
            view,
            font_mode: if options.pixel_font {
                FontMode::Pixel
            } else {
                FontMode::System
            },
            last_frame: Instant::now(),
            proxy,
            frames_left: options.frames,
            exit_on_result: options.until_result,
            mode,
            self_check: options.frames.is_some() || options.until_result,
            frames_drawn: 0,
            badge: None,
            badge_frames: 0,
            badge_enabled: options.badge,
            #[cfg(target_os = "macos")]
            menu: MenuBarState {
                interval,
                ..MenuBarState::default()
            },
        }
    }

    // -- surfaces ----------------------------------------------------------

    /// Adds the status item to the menu bar.
    ///
    /// macOS requires the main thread *and* a running run loop, so this cannot
    /// happen before `run_app`; `resumed` is the first point that satisfies
    /// both (it follows `StartCause::Init`).
    #[cfg(target_os = "macos")]
    fn start_menu_bar(&mut self, event_loop: &ActiveEventLoop) {
        if self.menu.item.is_some() {
            return;
        }
        // Handlers first: the item starts emitting as soon as it exists.
        menubar::forward_events(self.proxy.clone());

        match menubar::MenuBar::new(menubar::TITLE_IDLE) {
            Ok(item) => {
                self.menu.item = Some(item);
                self.trace("菜单栏项已创建");
            }
            Err(message) => {
                // No status bar (a locked-down session, an unusual host): show
                // the window instead of exiting with nothing on screen.
                eprintln!("菜单栏不可用，改用普通窗口：{message}");
                self.mode = Mode::Window;
                self.init_window(event_loop, Mode::Window);
                return;
            }
        }

        // A run with `--every 0` has no timer, so `暂停自动刷新` would be a lie:
        // grey it out rather than let it toggle nothing.
        if let Some(item) = self.menu.item.as_ref() {
            item.set_pause_enabled(self.menu.interval.is_some());
        }

        // The view refreshes on open, but opening is normally noticed by a
        // frame — and with no window there are no frames. Run the state half of
        // one now so the first query actually goes out.
        self.tick();

        if self.frames_left.is_some() || self.exit_on_result {
            // Both self-checks need a surface to exercise, so open the panel.
            self.open_panel(event_loop);
        }
    }

    /// Creates the window plus its surface, backend and font stack.
    ///
    /// In menu-bar mode this is the panel: borderless, fixed size, floating
    /// above other windows, and created *hidden* so it can be placed under the
    /// status item before it is ever seen.
    fn init_window(&mut self, event_loop: &ActiveEventLoop, mode: Mode) {
        if self.window.is_some() {
            return;
        }

        let attributes = match mode {
            Mode::Window => Window::default_attributes()
                .with_title(TITLE)
                .with_inner_size(LogicalSize::new(WINDOW_WIDTH, WINDOW_HEIGHT))
                .with_min_inner_size(LogicalSize::new(360.0, 360.0)),
            // Transparent, so the view's rounded backdrop is what shapes the
            // panel and the corners really are see-through. macOS keeps the
            // window's shadow, and AppKit derives it from that same alpha, so
            // the shadow follows the rounded outline instead of a square.
            Mode::MenuBar => Window::default_attributes()
                .with_title(TITLE)
                .with_decorations(false)
                .with_transparent(true)
                .with_resizable(false)
                .with_window_level(WindowLevel::AlwaysOnTop)
                .with_visible(false)
                .with_inner_size(LogicalSize::new(panel_size().width, panel_size().height)),
        };

        let window = Arc::new(event_loop.create_window(attributes).expect("create window"));
        let surface = self
            .instance
            .create_surface(window.clone())
            .expect("create surface");

        let mut backend = WgpuBackend::from_instance(
            &self.instance,
            Some(&surface),
            wgpu::PowerPreference::HighPerformance,
        )
        .expect("create wgpu backend");

        // Prefer a non-sRGB surface format so the unorm colors the shader writes
        // match the Canvas backend; fall back to whatever the surface offers.
        let capabilities = surface.get_capabilities(backend.adapter());
        let format = capabilities
            .formats
            .iter()
            .copied()
            .find(|format| !format.is_srgb())
            .unwrap_or(capabilities.formats[0]);

        let size = window.inner_size();
        let alpha_mode = surface_alpha_mode(&capabilities.alpha_modes, mode == Mode::MenuBar);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode,
            view_formats: Vec::new(),
        };
        surface.configure(backend.device(), &config);

        self.scale_factor = window.scale_factor();
        backend.set_scale_factor(self.scale_factor as f32);
        backend.set_clear_color(clear_color(
            mode == Mode::MenuBar,
            self.view.theme().palette.background,
        ));

        // Measure text with the backend's real font (system font, so CJK works).
        let font_config = FontConfig {
            mode: self.font_mode,
            device_pixel_rasterization: true,
        };
        if let Err(error) = backend.set_font_config(font_config) {
            eprintln!("font setup failed, using fallback: {error}");
        }
        self.view.set_text_measurer(Rc::new(BackendTextMeasurer {
            metrics: backend.text_metrics(),
        }));

        self.window = Some(window);
        self.surface = Some(surface);
        self.backend = Some(backend);
        self.config = Some(config);
        self.last_frame = Instant::now();

        if mode == Mode::Window {
            self.request_redraw();
        }

        // Self-check evidence for the panel's shape, which cannot be seen in a
        // screenshot-free run: the alpha mode has to be a non-opaque one, the
        // window has to be non-opaque, and AppKit still has to give it a shadow
        // (drawn from the rounded fill's alpha).
        #[cfg(target_os = "macos")]
        if self.self_check && mode == Mode::MenuBar {
            use winit::platform::macos::WindowExtMacOS;
            let shadow = self
                .window
                .as_ref()
                .is_some_and(|window| window.has_shadow());
            self.trace(&format!(
                "面板窗口 alpha_mode={alpha_mode:?} 透明=true 阴影={shadow}"
            ));
        }
    }

    fn resize(&mut self, width: u32, height: u32) {
        let (Some(surface), Some(backend), Some(config)) = (
            self.surface.as_ref(),
            self.backend.as_ref(),
            self.config.as_mut(),
        ) else {
            return;
        };
        if width == 0 || height == 0 {
            return;
        }
        config.width = width;
        config.height = height;
        surface.configure(backend.device(), config);
    }

    // -- the badge window (multi-window test) ------------------------------

    /// Creates the second window: a borderless, transparent overlay flush with
    /// the bottom-left corner of the desktop, with its own surface and backend.
    ///
    /// It shares nothing with the main window's set — that is the point of the
    /// test. Events reach it through the `WindowId` dispatch in
    /// [`App::window_event`]; the fetch worker and the view stay untouched.
    fn init_badge(&mut self, event_loop: &ActiveEventLoop) {
        if self.badge.is_some() {
            return;
        }
        let attributes = Window::default_attributes()
            .with_title("DeepSeek 徽章")
            .with_window_level(WindowLevel::AlwaysOnBottom)
            .with_decorations(false)
            .with_transparent(true)
            .with_resizable(false)
            .with_has_shadow(false)
            .with_inner_size(LogicalSize::new(badge::BADGE_WIDTH, badge::BADGE_HEIGHT));

        let window = Arc::new(
            event_loop
                .create_window(attributes)
                .expect("create badge window"),
        );
        // Read the state back off the live window rather than trusting the
        // attribute above — this line is the shadow's evidence in a
        // screenshot-free run (AGENTS.md rule 7).
        #[cfg(target_os = "macos")]
        {
            use winit::platform::macos::WindowExtMacOS;
            self.trace(&format!("徽章窗口 阴影={}", window.has_shadow()));
        }
        let surface = self
            .instance
            .create_surface(window.clone())
            .expect("create badge surface");

        let mut backend = WgpuBackend::from_instance(
            &self.instance,
            Some(&surface),
            wgpu::PowerPreference::HighPerformance,
        )
        .expect("create badge backend");

        let capabilities = surface.get_capabilities(backend.adapter());
        let format = capabilities
            .formats
            .iter()
            .copied()
            .find(|format| !format.is_srgb())
            .unwrap_or(capabilities.formats[0]);

        let size = window.inner_size();
        let alpha_mode = surface_alpha_mode(&capabilities.alpha_modes, true);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode,
            view_formats: Vec::new(),
        };
        surface.configure(backend.device(), &config);

        backend.set_scale_factor(window.scale_factor() as f32);
        // The view owns its theme now (a value, like the main view), so the text
        // colours and the (invisible) clear colour come from the same tokens.
        let mut view = BadgeApp::new(*self.view.theme());
        backend.set_clear_color(clear_color(true, view.theme().palette.background));
        let font_config = FontConfig {
            mode: self.font_mode,
            device_pixel_rasterization: true,
        };
        if let Err(error) = backend.set_font_config(font_config) {
            eprintln!("badge font setup failed, using fallback: {error}");
        }
        // Measure with the backend's real font, exactly like the main view —
        // otherwise the text is laid out for one font and painted with another.
        view.set_text_measurer(Rc::new(BackendTextMeasurer {
            metrics: backend.text_metrics(),
        }));
        // In menu-bar mode the on-open refresh is already in flight by the time
        // this window exists, so the badge shows what the view is already doing
        // instead of an idle that was never true. It starts on the active tab,
        // which is the one the settings file remembered.
        let (title, prefix) = badge_labels(self.view.tab());
        if self.view.is_loading() {
            let line = badge::State::Loading.line(prefix);
            view.show(title, prefix, &badge::State::Loading);
            self.trace(&format!("徽章初始 {title} → {line}"));
        } else {
            view.show(title, prefix, &badge::State::Idle);
        }

        // Bottom-left corner of the primary monitor, flush with both edges.
        // Monitor geometry is physical pixels with a top-left origin — the
        // same space `set_outer_position` takes.
        let monitor = event_loop
            .primary_monitor()
            .or_else(|| event_loop.available_monitors().next());
        if let Some(monitor) = monitor {
            let at = monitor.position();
            let bounds = monitor.size();
            let position =
                PhysicalPosition::new(at.x, at.y + bounds.height as i32 - size.height as i32);
            window.set_outer_position(position);
            self.trace(&format!(
                "徽章窗口 {w}×{h} @ ({x}, {y})，显示器 {mp:?} {ms:?}",
                w = size.width,
                h = size.height,
                x = position.x,
                y = position.y,
                mp = at,
                ms = bounds
            ));
        }

        window.request_redraw();
        self.badge = Some(Badge {
            window,
            surface,
            backend,
            config,
            view,
        });
    }

    /// Handles an event for the badge window (already matched by `WindowId`).
    /// Static content, so only the four cases below matter.
    fn on_badge_event(&mut self, event: WindowEvent) {
        match event {
            WindowEvent::RedrawRequested => self.render_badge(),
            WindowEvent::Resized(size) => self.resize_badge(size.width, size.height),
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                if let Some(badge) = self.badge.as_mut() {
                    badge.backend.set_scale_factor(scale_factor as f32);
                    badge.window.request_redraw();
                }
            }
            // Closing the badge closes only the badge; the app keeps running.
            WindowEvent::CloseRequested => {
                if let Some(badge) = self.badge.as_ref() {
                    badge.window.set_visible(false);
                }
            }
            _ => {}
        }
    }

    /// Reconfigures the badge's surface after a resize, then repaints.
    fn resize_badge(&mut self, width: u32, height: u32) {
        let Some(badge) = self.badge.as_mut() else {
            return;
        };
        if width == 0 || height == 0 {
            return;
        }
        badge.config.width = width;
        badge.config.height = height;
        badge
            .surface
            .configure(badge.backend.device(), &badge.config);
        badge.window.request_redraw();
    }

    /// Runs one badge frame: lay out the component tree, paint, submit, present.
    /// Static content, so the frame is identical every time — the loop is still
    /// event-driven and the badge only redraws when asked.
    fn render_badge(&mut self) {
        let Some(badge) = self.badge.as_mut() else {
            return;
        };
        let (width, height, format) =
            (badge.config.width, badge.config.height, badge.config.format);
        let scale = badge.window.scale_factor() as f32;
        let logical = Size::new(width as f32 / scale, height as f32 / scale);
        let viewport = ViewportSize::new(logical);

        // Same three steps as the main window: layout, paint, submit.
        badge.view.layout(viewport);
        let mut ctx = PaintContext::new();
        badge.view.paint(&mut ctx);
        let list = ctx.into_draw_list();
        let commands = list.len();

        let mut reconfigured = false;
        let surface_texture = loop {
            match badge.surface.get_current_texture() {
                Ok(texture) => break texture,
                Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) if !reconfigured => {
                    badge
                        .surface
                        .configure(badge.backend.device(), &badge.config);
                    reconfigured = true;
                }
                Err(wgpu::SurfaceError::Timeout) => return,
                Err(error) => {
                    eprintln!("badge surface error: {error}");
                    return;
                }
            }
        };
        let view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        if badge
            .backend
            .begin_frame_with_view(view, width, height, format, viewport)
            .is_ok()
        {
            let _ = badge.backend.submit(&list);
            let _ = badge.backend.end_frame();
        }
        surface_texture.present();

        // Report the first badge frame the same way the main window does: the
        // surface really got a draw list, through its own pipeline.
        self.badge_frames += 1;
        if self.badge_frames == 1 {
            self.trace(&format!(
                "徽章首帧 {w}×{h} 逻辑像素，{commands} 条绘制命令（独立 surface）",
                w = logical.width,
                h = logical.height,
            ));
        }
    }

    // -- the panel ---------------------------------------------------------

    /// Opens the panel, if it is not already up.
    #[cfg(target_os = "macos")]
    fn open_panel(&mut self, event_loop: &ActiveEventLoop) {
        if self.menu.open {
            return;
        }
        if self.window.is_none() {
            // Creating the window makes `winit` queue a synthetic
            // `Focused(false)`; note it before the window exists, because it
            // can be delivered either side of this call.
            self.menu.focus_flap.window_created();
            self.init_window(event_loop, Mode::MenuBar);
        }
        self.place_panel(event_loop);

        if let Some(window) = self.window.as_ref() {
            // `set_visible(true)` is `makeKeyAndOrderFront`; `focus_window` then
            // activates the app, which an accessory app does not do by itself.
            window.set_visible(true);
            window.focus_window();
        }
        self.menu.open = true;
        // Fill the countdown before the first frame, so a reopening panel does
        // not flash a footer without a line that appears one frame later.
        self.update_countdown();
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
        self.trace(&format!(
            "打开面板，倒计时 {}",
            self.view
                .countdown_text()
                .filter(|line| !line.is_empty())
                .unwrap_or("（无）")
        ));
    }

    /// Hides the panel. The window and its GPU resources stay alive, so the
    /// next open is immediate.
    #[cfg(target_os = "macos")]
    fn close_panel(&mut self, reason: &str) {
        if !self.menu.open {
            return;
        }
        if let Some(window) = self.window.as_ref() {
            window.set_visible(false);
        }
        self.menu.open = false;
        self.trace(&format!("收起面板（{reason}）"));
    }

    /// A left click on the status item: show the panel, or hide it again.
    #[cfg(target_os = "macos")]
    fn toggle_panel(&mut self, event_loop: &ActiveEventLoop) {
        // Clicking the status item makes AppKit move key status to the status
        // bar, so the panel's own `Focused(false)` handler usually hides it a
        // few milliseconds *before* the click lands here. Without this guard
        // that pair would close and instantly reopen, and the panel could never
        // be dismissed by clicking the item again.
        if let Some(at) = self.menu.closed_at {
            if at.elapsed() < TOGGLE_GUARD {
                self.trace("点击状态栏项：刚收起过，忽略");
                return;
            }
        }

        if self.menu.open {
            self.close_panel("点击状态栏项");
        } else {
            self.open_panel(event_loop);
        }
    }

    /// Moves the panel under the status item.
    ///
    /// Both rectangles are physical pixels in screen space with a top-left
    /// origin, which is what `tray-icon` reports on macOS and what
    /// `Window::set_outer_position` expects.
    ///
    /// Which rectangle to trust is the whole problem: AppKit's frame for the
    /// item goes stale, so a click (the pointer was inside the item, so its `x`
    /// is real) beats polling, and polling beats a guess at the monitor's
    /// top-right corner. See [`App::tray_geometry`].
    #[cfg(target_os = "macos")]
    fn place_panel(&mut self, event_loop: &ActiveEventLoop) {
        let Some(window) = self.window.as_ref() else {
            return;
        };
        let polled = self.polled_tray(event_loop);
        let Some(monitor) = monitor_for(event_loop, polled.map(|tray| tray.center())) else {
            return;
        };
        let bounds = monitor_rect(&monitor);
        let scale = monitor.scale_factor() as f32;

        let (tray, source) = self.tray_geometry(event_loop, bounds, scale);
        let size = panel_device_size(scale);
        let at = menubar::anchor(size, tray, bounds, PANEL_GAP * scale);
        window.set_outer_position(PhysicalPosition::new(
            at.x.round() as i32,
            at.y.round() as i32,
        ));
        // Only a guess is worth retrying: once the menu bar has been laid out
        // the reading is good, and until then the panel sits where the guess
        // put it instead of jumping.
        self.menu.reposition = source == Source::Fallback;
        self.trace(&format!(
            "面板 {:.0}×{:.0} @ ({:.0}, {:.0})；锚点 {tray:?}（{}）；轮询 {:?}；显示器 {bounds:?}",
            size.width,
            size.height,
            at.x,
            at.y,
            source.label(),
            polled
        ));
    }

    /// The item geometry to hang the panel off, and where it came from.
    ///
    /// Priority: the pointer from the last click on the item — it was inside the
    /// item, so it cannot be stale — then a polled frame that passes
    /// [`is_laid_out`], then a guess at the monitor's top-right corner.
    #[cfg(target_os = "macos")]
    fn tray_geometry(
        &self,
        event_loop: &ActiveEventLoop,
        bounds: Rect,
        scale: f32,
    ) -> (Rect, Source) {
        panel_anchor(
            self.menu.anchor,
            self.polled_tray(event_loop),
            bounds,
            scale,
        )
    }

    /// The status item's rectangle, if the run loop reports a plausible one.
    #[cfg(target_os = "macos")]
    fn polled_tray(&self, event_loop: &ActiveEventLoop) -> Option<Rect> {
        let tray = self.menu.item.as_ref().and_then(|item| item.rect())?;
        let monitor = monitor_for(event_loop, Some(tray.center()))?;
        let bounds = monitor_rect(&monitor);
        let scale = monitor.scale_factor() as f32;
        is_laid_out(&tray, bounds, scale).then_some(tray)
    }

    /// Re-places the panel once the menu bar has been laid out.
    ///
    /// Called from [`App::service_menu_bar`], i.e. one event batch after the
    /// panel opened, which is when the status item finally reports a real
    /// rectangle.
    #[cfg(target_os = "macos")]
    fn reposition_panel(&mut self, event_loop: &ActiveEventLoop) {
        if !self.menu.reposition {
            return;
        }
        let Some(tray) = self.polled_tray(event_loop) else {
            return;
        };
        let Some(monitor) = monitor_for(event_loop, Some(tray.center())) else {
            return;
        };
        let bounds = monitor_rect(&monitor);
        let scale = monitor.scale_factor() as f32;
        let size = panel_device_size(scale);
        let at = menubar::anchor(size, tray, bounds, PANEL_GAP * scale);
        if let Some(window) = self.window.as_ref() {
            window.set_outer_position(PhysicalPosition::new(
                at.x.round() as i32,
                at.y.round() as i32,
            ));
        }
        self.menu.reposition = false;
        self.trace(&format!("面板重新定位到 ({:.0}, {:.0})", at.x, at.y));
    }

    /// Whether the status item reports a usable rectangle yet.
    #[cfg(target_os = "macos")]
    fn item_is_laid_out(&self, event_loop: &ActiveEventLoop) -> bool {
        self.polled_tray(event_loop).is_some()
    }

    /// Reports the status item's geometry whenever it changes.
    ///
    /// `tray-icon` reads it out of AppKit, and AppKit rewrites the item's window
    /// as the title grows and the menu bar is laid out, so the number is worth
    /// watching: a placement complaint ("the panel is nowhere near the item")
    /// is nearly always a reading taken at the wrong moment.
    #[cfg(target_os = "macos")]
    fn trace_tray(&mut self, event_loop: &ActiveEventLoop) {
        let Some(tray) = self.menu.item.as_ref().and_then(|item| item.rect()) else {
            return;
        };
        if self.menu.last_tray == Some(tray) {
            return;
        }
        self.menu.last_tray = Some(tray);
        let ready = if self.item_is_laid_out(event_loop) {
            ""
        } else {
            "（不在菜单栏，不用）"
        };
        self.trace(&format!("状态栏项报告 {tray:?}{ready}"));
    }

    /// Mirrors the view into the status item: the active tab's headline as the
    /// title, its state line as the tooltip.
    #[cfg(target_os = "macos")]
    fn sync_menubar(&self) {
        let Some(item) = self.menu.item.as_ref() else {
            return;
        };
        let title = self
            .active_headline()
            .unwrap_or_else(|| menubar::TITLE_IDLE.to_string());
        item.set_title(&title);
        item.set_tooltip(&self.view.summary());
        self.trace(&format!("状态栏标题 → {title}"));
    }

    /// The compact value for the menu-bar item, from the active tab: the
    /// DeepSeek total (`¥110.00`) or the Go rolling remainder (`Go 88%`).
    fn active_headline(&self) -> Option<String> {
        match self.view.tab() {
            Tab::DeepSeek => self.view.last_balance().map(Balance::headline),
            Tab::Go => self.view.last_go().map(GoUsage::headline),
        }
    }

    // -- frame -------------------------------------------------------------

    /// The view's logical size: the surface's, or the panel/window's nominal
    /// size while no surface exists yet.
    fn viewport(&self) -> ViewportSize {
        let size = match self.config.as_ref() {
            Some(config) => Size::new(
                config.width as f32 / self.scale_factor as f32,
                config.height as f32 / self.scale_factor as f32,
            ),
            None => match self.mode {
                Mode::MenuBar => panel_size(),
                Mode::Window => Size::new(WINDOW_WIDTH, WINDOW_HEIGHT),
            },
        };
        ViewportSize::new(size)
    }

    /// Seconds since the previous frame, capped so a long idle (the panel was
    /// closed, the refresh timer slept) cannot produce a huge step.
    fn frame_delta(&mut self) -> f32 {
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame).as_secs_f32().min(0.1);
        self.last_frame = now;
        dt
    }

    /// The state half of a frame: advance the view and hand any pending refresh
    /// to the worker thread. Nothing is painted, and it is idempotent within a
    /// batch — the second call in a frame finds nothing left to do.
    ///
    /// Menu-bar mode runs this from [`App::service_menu_bar`], i.e. once per
    /// event batch with the panel up or down, so the view's state machine — the
    /// status item's text, the button's cooldown — never depends on a frame
    /// happening to be drawn. It schedules the frame itself when something
    /// moved.
    fn tick(&mut self) {
        let dt = self.frame_delta();
        let viewport = self.viewport();
        let tab = self.view.tab();
        if self.view.update(viewport, dt) {
            // The button's label is the throttle made visible, and the menu item
            // carries the same state — narrating both is how a run without
            // screenshots can tell they agree.
            let menu = self.sync_refresh_menu();
            self.trace(&format!(
                "按钮 → {}{menu}",
                self.view.refresh_label().unwrap_or("（无）")
            ));
            // A moved label needs a frame of its own: with the panel sitting
            // still there is nothing else guaranteed to schedule one.
            self.request_redraw();
        }
        if self.view.tab() != tab {
            self.on_tab_changed();
        }
        if self.view.take_refresh_request() {
            // The badge narrates the same request: it flips to "刷新中…" now and
            // back to a value when *this* request comes home.
            self.refresh_badge();
            self.spawn_fetches();
        }
    }

    /// A tab switch: remember it, and move the surfaces that echo the active
    /// page — the status item and the badge.
    fn on_tab_changed(&mut self) {
        let tab = self.view.tab();
        settings::save(tab);
        #[cfg(target_os = "macos")]
        self.sync_menubar();
        self.refresh_badge();
        self.trace(&format!("标签页 → {}", tab.label()));
    }

    /// Hands a finished DeepSeek request to every window that shows it.
    ///
    /// The refresh itself stays where it was — the main view asks for it and
    /// [`App::tick`] spawns the fetches — so this is only about the *result*:
    /// the main view and the badge read the same one, which is why the two
    /// windows can never disagree and why the badge needs no endpoint, no
    /// worker and no timer of its own.
    fn apply_result(&mut self, result: Result<Balance, String>) {
        self.view.apply_result(result);
        self.refresh_badge();
    }

    /// The state the badge should show for the active tab.
    ///
    /// Ordered like the menu-bar title: a value already in hand wins over a
    /// refresh in flight (the badge, like the panel, keeps the last good number
    /// while the next one is fetched), then a failure before there was ever a
    /// value, then loading, then idle.
    fn active_badge_state(&self) -> badge::State {
        // `(headline, error, loading)` for the active tab.
        let value = match self.view.tab() {
            Tab::DeepSeek => self.view.last_balance().map(Balance::headline),
            Tab::Go => self.view.last_go().map(GoUsage::headline),
        };
        if let Some(headline) = value {
            return badge::State::Ready(headline);
        }
        let error = match self.view.tab() {
            Tab::DeepSeek => self.view.error_text(),
            Tab::Go => self.view.go_error_text(),
        };
        if has_text(error) {
            badge::State::Failed
        } else if self.view.is_loading() {
            badge::State::Loading
        } else {
            badge::State::Idle
        }
    }

    /// Pushes the active tab onto the badge window: rewrite its view, then ask
    /// for its frame — the badge, like everything else here, only redraws when
    /// told. A no-op when the run has no badge.
    fn refresh_badge(&mut self) {
        let (title, prefix) = badge_labels(self.view.tab());
        let state = self.active_badge_state();
        // The line first: `trace` needs `&self` and the badge borrow below needs
        // `&mut self`.
        let line = state.line(prefix);
        let Some(badge) = self.badge.as_mut() else {
            return;
        };
        badge.view.show(title, prefix, &state);
        badge.window.request_redraw();
        self.trace(&format!("徽章 {title} → {line}"));
    }

    /// Mirrors the throttle onto the status item's `刷新余额`, and returns a
    /// fragment for the trace (empty when there is no menu bar to mirror onto).
    fn sync_refresh_menu(&self) -> String {
        #[cfg(target_os = "macos")]
        if let Some(item) = self.menu.item.as_ref() {
            item.set_refresh_enabled(self.view.refresh_ready());
            return format!("（菜单项 enabled={}）", item.refresh_enabled());
        }
        String::new()
    }

    /// Feeds an input event to the view.
    fn feed(&mut self, event: &InputEvent) {
        self.view.event(event);
    }

    fn apply_cursor(&self) {
        let Some(window) = self.window.as_ref() else {
            return;
        };
        window.set_cursor(cursor_icon(self.view.cursor()));
    }

    /// Schedules one frame, if there is a surface that would show it.
    fn request_redraw(&self) {
        if self.mode == Mode::MenuBar && !self.panel_open() {
            return;
        }
        if let Some(window) = self.window.as_ref() {
            window.request_redraw();
        }
    }

    /// Whether a frame would be visible (always true in window mode).
    fn panel_open(&self) -> bool {
        #[cfg(target_os = "macos")]
        if self.mode == Mode::MenuBar {
            return self.menu.open;
        }
        true
    }

    /// Runs one frame: update, layout, paint, submit, present.
    fn render(&mut self) {
        self.apply_cursor();

        let Some(config) = self.config.as_ref() else {
            return;
        };
        let (width, height, format) = (config.width, config.height, config.format);
        let viewport = self.viewport();

        self.tick();
        self.view.layout(viewport);

        let mut ctx = PaintContext::new();
        self.view.paint(&mut ctx);
        let list = ctx.into_draw_list();
        let commands = list.len();

        let (Some(surface), Some(backend), Some(config)) = (
            self.surface.as_ref(),
            self.backend.as_mut(),
            self.config.as_ref(),
        ) else {
            return;
        };

        // A surface that was hidden for a while can come back `Outdated`; one
        // reconfigure is enough, and retrying twice would risk spinning.
        let mut reconfigured = false;
        let surface_texture = loop {
            match surface.get_current_texture() {
                Ok(texture) => break texture,
                Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) if !reconfigured => {
                    surface.configure(backend.device(), config);
                    reconfigured = true;
                }
                Err(wgpu::SurfaceError::Timeout) => return,
                Err(error) => {
                    eprintln!("surface error: {error}");
                    return;
                }
            }
        };

        let view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        if backend
            .begin_frame_with_view(view, width, height, format, viewport)
            .is_ok()
        {
            let _ = backend.submit(&list);
            let _ = backend.end_frame();
        }

        surface_texture.present();

        // One line per run: enough to show the pipeline really produced a frame
        // (and how much of one) without a screenshot.
        self.frames_drawn += 1;
        if self.frames_drawn == 1 {
            self.trace(&format!(
                "首帧 {:.0}×{:.0} 逻辑像素，{commands} 条绘制命令",
                viewport.logical_size().width,
                viewport.logical_size().height
            ));
        }
    }

    /// Queries every source on worker threads and posts the results back.
    ///
    /// One refresh means one query per source, each on its own thread, so a slow
    /// endpoint cannot hold up the other. They post distinct events and the view
    /// leaves `Busy` only when the last one lands.
    fn spawn_fetches(&self) {
        self.spawn_balance_fetch();
        self.spawn_go_fetch();
    }

    /// Queries the DeepSeek balance endpoint on a worker thread.
    ///
    /// Both the endpoint and the API key are re-read per request, so a variable
    /// configured after launch (e.g. written to `~/.zshrc`) is picked up by the
    /// next refresh instead of requiring a restart. An unconfigured key never
    /// touches the network: the view gets the "未配置" error instead.
    fn spawn_balance_fetch(&self) {
        let proxy = self.proxy.clone();
        let endpoint = api::endpoint();
        self.trace(&format!("发起请求 {endpoint}"));
        std::thread::spawn(move || {
            // Resolving here keeps the login-shell probe (a subprocess) off the
            // UI thread.
            let result = match api::resolve_api_key() {
                Ok(api_key) => api::fetch(&endpoint, &api_key),
                Err(message) => Err(message),
            };
            // A closed loop (window gone) just drops the result.
            let _ = proxy.send_event(UserEvent::Balance(result));
        });
    }

    /// Queries the OpenCode Go usage endpoint on a worker thread.
    ///
    /// Same shape as [`App::spawn_balance_fetch`], with the key coming from the
    /// opencode `auth.json` first (see [`crate::go::resolve_api_key`]).
    fn spawn_go_fetch(&self) {
        let proxy = self.proxy.clone();
        let endpoint = go::endpoint();
        self.trace(&format!("发起请求 {endpoint}"));
        std::thread::spawn(move || {
            let result = match go::resolve_api_key() {
                Ok(api_key) => go::fetch(&endpoint, &api_key),
                Err(message) => Err(message),
            };
            let _ = proxy.send_event(UserEvent::GoUsage(result));
        });
    }

    /// Self-check narration; silent in a normal session. `QUILL_TRACE=1` turns
    /// it on for an interactive run, which is how a placement question gets
    /// answered without reaching for a screenshot.
    fn trace(&self, message: &str) {
        if self.self_check {
            println!("[自检] {message}");
        } else if std::env::var_os("QUILL_TRACE").is_some() {
            println!("[trace] {message}");
        }
    }
}

/// Keeps the status item current while the panel is closed.
///
/// `winit` is parked in `ControlFlow::Wait`, so without this nothing would
/// advance the view's state machine — and therefore nothing would start the
/// first fetch. `about_to_wait` runs once per event batch, which covers both
/// the initial kick and every reply that wakes the loop.
#[cfg(target_os = "macos")]
impl App {
    fn service_menu_bar(&mut self, event_loop: &ActiveEventLoop) {
        self.trace_tray(event_loop);
        if self.menu.open {
            self.reposition_panel(event_loop);
        }

        // The view advances on *every* batch, panel open or not. This is what
        // makes a wake-up worth anything: the cooldown ticks, the status item is
        // mirrored, and `tick` asks for the frame that shows the new label.
        //
        // Leaving this to the paint path instead — where it used to live, behind
        // `render` — made the countdown advance only on the batches that
        // happened to schedule a frame, which while the panel is up are the
        // ones carrying pointer events. The panel then looked frozen until the
        // mouse moved over it.
        self.tick();

        let now = Instant::now();
        // A paused timer is left alone entirely: `next` is `None`, so nothing is
        // due and no new deadline is seeded. Resuming (see `toggle_pause`) starts
        // the interval over.
        if !self.menu.paused {
            if let Some(interval) = self.menu.interval {
                let due = *self.menu.next.get_or_insert(now + interval);
                if now >= due {
                    self.menu.next = Some(now + interval);
                    self.view.request_refresh();
                    self.tick();
                    self.request_redraw();
                }
            }
        }
        self.update_countdown();

        // The footer countdown only exists with a timer; the button counts its
        // cooldown down either way, so a run with `--every 0` still has something
        // to animate while the panel is open.
        let ticking = self.menu.open
            && ((self.menu.interval.is_some() && !self.menu.paused) || !self.view.refresh_ready());
        let tick_at = ticking.then(|| next_tick(now, self.menu.tick_at));
        self.menu.tick_at = tick_at;

        event_loop.set_control_flow(match wake_at(self.menu.next, tick_at) {
            Some(at) => ControlFlow::WaitUntil(at),
            None => ControlFlow::Wait,
        });
    }

    /// Pushes the time left before the next automatic refresh into the view, and
    /// schedules a frame when the line moved.
    ///
    /// The view does the formatting; this only owns the clock. Opening the panel
    /// seeds the deadline early — before the first frame — so the panel does not
    /// open without a line that shows up one batch later. A closed panel gets
    /// `None`, which is also what a run without a timer shows, so the line never
    /// claims a refresh that will not happen.
    fn update_countdown(&mut self) {
        // A paused timer has no deadline to count down to; the line says so
        // instead of disappearing, so a paused panel is not mistaken for one
        // whose timer simply has not been configured.
        let changed = if self.menu.paused && self.menu.interval.is_some() {
            self.view.set_countdown_paused()
        } else {
            let left = match (self.menu.open, self.menu.interval) {
                (true, Some(interval)) => {
                    let next = *self
                        .menu
                        .next
                        .get_or_insert_with(|| Instant::now() + interval);
                    Some(next.saturating_duration_since(Instant::now()))
                }
                _ => None,
            };
            self.view.set_countdown(left)
        };
        if changed {
            // Narrating each move is the only way to see the countdown tick in a
            // run that takes no screenshots.
            self.trace(&format!(
                "倒计时 → {}",
                self.view.countdown_text().unwrap_or("（无）")
            ));
            self.request_redraw();
        }
    }

    /// A click on the status item.
    fn on_tray(&mut self, event_loop: &ActiveEventLoop, event: tray_icon::TrayIconEvent) {
        use tray_icon::{MouseButton, MouseButtonState, TrayIconEvent};
        // Both the press and the release arrive, and the press is also the one
        // that opens the menu on a right click; acting on the release keeps a
        // single left click from toggling twice.
        let TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
            rect,
            position,
            ..
        } = event
        else {
            return;
        };
        // The one reading that cannot be stale: the click was delivered, so the
        // pointer was inside the item. The panel is centred on that `x`.
        self.menu.anchor = Some(TrayAnchor {
            pointer: Vec2::new(position.x as f32, position.y as f32),
        });
        self.trace(&format!(
            "点击状态栏项 @ ({:.0}, {:.0})；事件中的项 {:.2?}",
            position.x, position.y, rect
        ));
        self.toggle_panel(event_loop);
    }

    /// A menu action.
    fn on_menu(&mut self, event_loop: &ActiveEventLoop, event: tray_icon::menu::MenuEvent) {
        match event.id().0.as_str() {
            menubar::MENU_REFRESH => {
                self.view.request_refresh();
                self.tick();
                self.request_redraw();
            }
            menubar::MENU_PAUSE => self.toggle_pause(),
            menubar::MENU_PANEL => self.toggle_panel(event_loop),
            menubar::MENU_QUIT => event_loop.exit(),
            _ => {}
        }
    }

    /// Flips the automatic timer between running and held.
    ///
    /// Resuming starts the interval over rather than firing at once, so
    /// `继续自动刷新` never produces a request the user did not ask for; the
    /// next automatic query is one full interval away. A run with no timer at
    /// all (`--every 0`) has nothing to toggle and the menu entry is greyed out.
    fn toggle_pause(&mut self) {
        let Some(interval) = self.menu.interval else {
            return;
        };
        self.menu.paused = !self.menu.paused;
        self.menu.next = if self.menu.paused {
            None
        } else {
            Some(Instant::now() + interval)
        };
        if let Some(item) = self.menu.item.as_ref() {
            item.set_paused(self.menu.paused);
        }
        self.trace(&format!(
            "自动刷新 → {}{}",
            if self.menu.paused {
                "已暂停"
            } else {
                "继续"
            },
            self.menu
                .item
                .as_ref()
                .map(|item| format!("（菜单项 {}）", item.pause_label()))
                .unwrap_or_default(),
        ));
        // The countdown line has to move now, not on the next batch: with the
        // panel open and the pointer still, nothing else schedules a frame.
        self.update_countdown();
        self.request_redraw();
    }
}

impl ApplicationHandler<UserEvent> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        match self.mode {
            #[cfg(target_os = "macos")]
            Mode::MenuBar => self.start_menu_bar(event_loop),
            _ => self.init_window(event_loop, Mode::Window),
        }
        // The second window comes up next to whatever the mode opened; its
        // events are routed by `WindowId`, so nothing else has to know it
        // exists.
        if self.badge_enabled {
            self.init_badge(event_loop);
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::Balance(result) => {
                // `--until-result`: report what landed and stop. This checks the
                // whole window + worker + apply path without any screenshot.
                if self.exit_on_result {
                    report(&result);
                    self.apply_result(result);
                    event_loop.exit();
                    return;
                }
                self.apply_result(result);
                #[cfg(target_os = "macos")]
                self.sync_menubar();
            }
            UserEvent::GoUsage(result) => {
                self.view.apply_go_result(result);
                self.refresh_badge();
                #[cfg(target_os = "macos")]
                self.sync_menubar();
            }
            #[cfg(target_os = "macos")]
            UserEvent::Tray(event) => self.on_tray(event_loop, event),
            #[cfg(target_os = "macos")]
            UserEvent::Menu(event) => self.on_menu(event_loop, event),
        }
        self.request_redraw();
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        #[cfg(target_os = "macos")]
        if self.mode == Mode::MenuBar {
            self.service_menu_bar(event_loop);
        }
        #[cfg(not(target_os = "macos"))]
        let _ = event_loop;
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        // The badge window is a self-contained surface: its events never touch
        // the main window's view, the worker or the menu bar.
        if self.badge.as_ref().map(|badge| badge.window.id()) == Some(window_id) {
            self.on_badge_event(event);
            return;
        }
        let _ = window_id;

        // A redraw *is* the render itself: run it, then honour `--frames`.
        if matches!(event, WindowEvent::RedrawRequested) {
            if !self.panel_open() {
                return;
            }
            self.render();
            if let Some(left) = self.frames_left.as_mut() {
                *left = left.saturating_sub(1);
                if *left == 0 {
                    event_loop.exit();
                }
            }
            return;
        }

        #[cfg(target_os = "macos")]
        if self.mode == Mode::MenuBar {
            // Closing the panel is not closing the app: the status item is the
            // app's home, and only the menu's 退出 leaves.
            if matches!(event, WindowEvent::CloseRequested) {
                self.close_panel("窗口关闭请求");
                return;
            }
            // Clicking anywhere else dismisses the panel, the way a popover
            // behaves. This also fires for the click that targets the status
            // item itself; `toggle_panel` guards against that pair.
            if matches!(event, WindowEvent::Focused(false)) {
                if !self.menu.focus_flap.is_dismissal() {
                    // The one `winit` queues when it creates a window, not a
                    // click outside — swallowing the panel here is what made
                    // the first click on the status item appear to do nothing.
                    self.trace("忽略窗口创建时的合成失焦");
                    return;
                }
                if self.menu.open {
                    self.close_panel("失去焦点");
                    self.menu.closed_at = Some(Instant::now());
                }
                return;
            }
            if is_escape(&event) {
                self.close_panel("Esc");
                return;
            }
        }

        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
                return;
            }
            WindowEvent::Resized(size) => self.resize(size.width, size.height),
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.scale_factor = scale_factor;
                if let Some(backend) = self.backend.as_mut() {
                    backend.set_scale_factor(scale_factor as f32);
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = self.to_logical(position);
                self.feed(&InputEvent::PointerMove {
                    position: self.cursor,
                });
            }
            WindowEvent::CursorLeft { .. } => self.feed(&InputEvent::PointerLeave),
            WindowEvent::MouseInput { state, button, .. } => {
                let position = self.cursor;
                let button = pointer_button(button);
                let event = match state {
                    ElementState::Pressed => InputEvent::PointerDown { position, button },
                    ElementState::Released => InputEvent::PointerUp { position, button },
                };
                self.feed(&event);
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let Some(key) = map_key(&event.logical_key) else {
                    return;
                };
                let input = match event.state {
                    ElementState::Pressed => InputEvent::KeyDown { key },
                    ElementState::Released => InputEvent::KeyUp { key },
                };
                self.feed(&input);
            }
            _ => {}
        }

        // Any handled event may have changed the UI: schedule exactly one frame.
        self.request_redraw();
    }
}

impl App {
    fn to_logical(&self, position: winit::dpi::PhysicalPosition<f64>) -> Vec2 {
        Vec2::new(
            position.x as f32 / self.scale_factor as f32,
            position.y as f32 / self.scale_factor as f32,
        )
    }
}

/// The monitor containing `point`, or the primary one when `point` is unknown
/// or off every monitor.
fn monitor_for(event_loop: &ActiveEventLoop, point: Option<Vec2>) -> Option<MonitorHandle> {
    let monitors: Vec<MonitorHandle> = event_loop.available_monitors().collect();
    let hit = point.and_then(|point| {
        monitors
            .iter()
            .find(|monitor| monitor_rect(monitor).contains(point))
            .cloned()
    });
    hit.or_else(|| monitors.first().cloned())
}

/// A monitor's bounds in screen space (physical pixels, top-left origin).
fn monitor_rect(monitor: &MonitorHandle) -> Rect {
    let position = monitor.position();
    let size = monitor.size();
    Rect::from_min_size(
        Vec2::new(position.x as f32, position.y as f32),
        Size::new(size.width as f32, size.height as f32),
    )
}

/// Whether a status item reports a real, usable rectangle.
///
/// Two ways AppKit lies about this. Before the menu bar is laid out the item's
/// window has no size *and* sits in the screen's bottom-left corner; later, and
/// more confusingly, the frame goes stale: it keeps the size it had while the
/// item still shows the old title, parked wherever the window happened to be.
/// So a reading only counts if it is a sensible size **and** inside the menu
/// bar strip at the top of its monitor.
fn is_laid_out(tray: &Rect, monitor: Rect, scale: f32) -> bool {
    let in_strip = tray.top() >= monitor.top() - 2.0
        && tray.bottom() <= monitor.top() + MENU_BAR_MAX * scale
        && tray.left() >= monitor.left() - 2.0
        && tray.right() <= monitor.right() + 2.0;
    tray.size.width > 0.0 && tray.size.height > 0.0 && in_strip
}

/// A stand-in for an item that has not been laid out yet: the top-right corner
/// of `monitor`, which is where macOS puts the newest status item.
fn fallback_tray(monitor: Rect, scale: f32) -> Rect {
    let (width, inset, height) = FALLBACK_ITEM;
    let right = monitor.right() - inset * scale;
    Rect::from_min_max(
        Vec2::new(right - width * scale, monitor.top()),
        Vec2::new(right, monitor.top() + height * scale),
    )
}

/// The item geometry to hang the panel off, and where it came from.
///
/// Split out of [`App::tray_geometry`] so the precedence — a click beats a
/// polled frame, which beats the stand-in — can be tested without a run loop.
#[cfg(target_os = "macos")]
fn panel_anchor(
    anchor: Option<TrayAnchor>,
    polled: Option<Rect>,
    bounds: Rect,
    scale: f32,
) -> (Rect, Source) {
    if let Some(anchor) = anchor {
        // A reading the click actually landed inside was taken as the click
        // arrived, so it is fresh: trust it whole. That centres the panel — and
        // with it the arrow — on the item, rather than on whichever part of the
        // icon the pointer happened to be over.
        if let Some(polled) = polled.filter(|tray| tray.contains(anchor.pointer)) {
            return (polled, Source::Polled);
        }
        // Otherwise the frame is stale — AppKit parks the item's window in a
        // screen corner until it lays the menu bar out — and the click is the
        // one trustworthy thing: centre on it. The shape (menu bar height, and
        // so how far below the bar the panel hangs) still comes from the frame
        // when that part of it looks real, else from the stand-in.
        let shape = polled
            .filter(|tray| is_laid_out(tray, bounds, scale))
            .unwrap_or_else(|| fallback_tray(bounds, scale));
        let tray = Rect::from_min_size(
            Vec2::new(anchor.pointer.x - shape.size.width / 2.0, shape.top()),
            shape.size,
        );
        return (tray, Source::Click);
    }
    match polled {
        Some(tray) => (tray, Source::Polled),
        None => (fallback_tray(bounds, scale), Source::Fallback),
    }
}

/// The surface alpha mode to configure.
///
/// A transparent window needs a non-opaque mode, or the compositor drops the
/// alpha channel and the panel's rounded corners come out as black squares.
/// `wgpu-hal` reports `[Opaque, PostMultiplied]` for Metal and maps the second
/// to `CAMetalLayer.opaque = false`; the IR's blend equation already leaves
/// premultiplied colours in the framebuffer, which is what such a layer wants.
/// The ordinary window keeps the first (opaque) entry.
fn surface_alpha_mode(
    available: &[wgpu::CompositeAlphaMode],
    transparent: bool,
) -> wgpu::CompositeAlphaMode {
    let opaque = available
        .first()
        .copied()
        .unwrap_or(wgpu::CompositeAlphaMode::Opaque);
    if !transparent {
        return opaque;
    }
    available
        .iter()
        .copied()
        .find(|mode| *mode != wgpu::CompositeAlphaMode::Opaque)
        .unwrap_or(opaque)
}

/// The colour the backend clears to, which is the panel's own backdrop token.
///
/// Opaque in a normal window; in a transparent window it keeps the colour but
/// drops to zero alpha, so everything outside the view's rounded fill — the
/// four corners — is genuinely transparent. Matching the view's token means the
/// rounded fill and the clear colour around it are the same colour, which is
/// what keeps the ordinary window looking exactly as it did.
fn clear_color(transparent: bool, background: draw_core::Color) -> draw_core::Color {
    if transparent {
        draw_core::Color::new(background.r, background.g, background.b, 0.0)
    } else {
        background
    }
}

/// Prints an applied reply to stdout (`--until-result`).
fn report(result: &Result<Balance, String>) {
    match result {
        Ok(balance) => {
            println!("[自检] 状态栏标题 {}", balance.headline());
            println!("[自检] is_available={}", balance.is_available);
            for info in &balance.balance_infos {
                println!(
                    "[自检]   {} total={} granted={} topped_up={}",
                    info.currency, info.total_balance, info.granted_balance, info.topped_up_balance
                );
            }
        }
        Err(message) => println!("[自检] 刷新失败 — {message}"),
    }
}

fn cursor_icon(cursor: draw_core::Cursor) -> CursorIcon {
    match cursor {
        draw_core::Cursor::Default => CursorIcon::Default,
        draw_core::Cursor::Pointer => CursorIcon::Pointer,
        draw_core::Cursor::Text => CursorIcon::Text,
        draw_core::Cursor::ColResize => CursorIcon::ColResize,
        draw_core::Cursor::RowResize => CursorIcon::RowResize,
        draw_core::Cursor::Grab => CursorIcon::Grab,
        draw_core::Cursor::Grabbing => CursorIcon::Grabbing,
    }
}

fn pointer_button(button: MouseButton) -> PointerButton {
    match button {
        MouseButton::Right => PointerButton::Right,
        MouseButton::Middle => PointerButton::Middle,
        _ => PointerButton::Left,
    }
}

/// Whether `event` is the press of `Esc` (the panel's dismiss key).
fn is_escape(event: &WindowEvent) -> bool {
    matches!(
        event,
        WindowEvent::KeyboardInput { event, .. }
            if event.state == ElementState::Pressed
                && matches!(event.logical_key, WinitKey::Named(NamedKey::Escape))
    )
}

fn map_key(key: &WinitKey) -> Option<Key> {
    match key {
        WinitKey::Named(NamedKey::Enter) => Some(Key::Enter),
        WinitKey::Named(NamedKey::Escape) => Some(Key::Escape),
        WinitKey::Named(NamedKey::Backspace) => Some(Key::Backspace),
        WinitKey::Named(NamedKey::Delete) => Some(Key::Delete),
        WinitKey::Named(NamedKey::Tab) => Some(Key::Tab),
        WinitKey::Named(NamedKey::Space) => Some(Key::Space),
        WinitKey::Named(NamedKey::Home) => Some(Key::Home),
        WinitKey::Named(NamedKey::End) => Some(Key::End),
        WinitKey::Named(NamedKey::ArrowUp) => Some(Key::ArrowUp),
        WinitKey::Named(NamedKey::ArrowDown) => Some(Key::ArrowDown),
        WinitKey::Named(NamedKey::ArrowLeft) => Some(Key::ArrowLeft),
        WinitKey::Named(NamedKey::ArrowRight) => Some(Key::ArrowRight),
        WinitKey::Named(NamedKey::F5) => Some(Key::F5),
        WinitKey::Character(text) => text.chars().next().map(Key::Character),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_window_is_the_fallback_mode() {
        assert_eq!(Mode::default_for(true), Mode::Window);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn the_menu_bar_is_the_default_on_macos() {
        assert_eq!(Mode::default_for(false), Mode::MenuBar);
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn the_menu_bar_is_not_offered_elsewhere() {
        assert_eq!(Mode::default_for(false), Mode::Window);
    }

    #[test]
    fn a_monitor_rect_keeps_its_origin() {
        // Guards the coordinate space the panel is placed in: top-left origin,
        // physical pixels, matching what `tray-icon` reports.
        let rect = Rect::from_min_size(Vec2::new(-1920.0, 0.0), Size::new(1920.0, 1080.0));
        assert!(rect.contains(Vec2::new(-100.0, 500.0)));
        assert!(!rect.contains(Vec2::new(100.0, 500.0)));
    }

    /// A 2560×1664 display at the origin, drawn at 2×.
    fn screen() -> (Rect, f32) {
        (
            Rect::from_min_size(Vec2::ZERO, Size::new(2560.0, 1664.0)),
            2.0,
        )
    }

    #[test]
    fn an_unlaid_out_status_item_is_rejected() {
        // Exactly what AppKit reported before the menu bar was laid out: the
        // item's window parked in the screen's bottom-left corner.
        let (monitor, scale) = screen();
        let corner = Rect::from_min_size(Vec2::new(0.0, 1664.0), Size::new(64.0, 58.0));
        assert!(!is_laid_out(&corner, monitor, scale));
        assert!(!is_laid_out(
            &Rect::from_min_size(Vec2::new(0.0, 1672.0), Size::new(64.0, 0.0)),
            monitor,
            scale
        ));

        // A real item: in the menu bar strip, inside the screen.
        let real = Rect::from_min_size(Vec2::new(2484.0, 0.0), Size::new(60.0, 48.0));
        assert!(is_laid_out(&real, monitor, scale));
    }

    #[test]
    fn a_stale_frame_off_the_screen_is_rejected() {
        // The other way AppKit lies: a frame that still has its old size, but
        // sits on a stretch of screen no menu bar is on.
        let (monitor, scale) = screen();
        let below = Rect::from_min_size(Vec2::new(1506.0, 1200.0), Size::new(120.0, 58.0));
        assert!(!is_laid_out(&below, monitor, scale));

        let beside = Rect::from_min_size(Vec2::new(2700.0, 0.0), Size::new(120.0, 58.0));
        assert!(!is_laid_out(&beside, monitor, scale));
    }

    #[test]
    fn a_click_centres_the_panel_under_the_item() {
        // The click reading wins over the polled one, and only its `x` is used:
        // the pointer was inside the item, while the frame may be stale.
        let (monitor, scale) = screen();
        let polled = Rect::from_min_size(Vec2::new(1506.0, 0.0), Size::new(120.0, 58.0));
        let size = panel_device_size(scale);

        // An item with room on both sides: the panel's centre lands on it.
        let anchor = TrayAnchor {
            pointer: Vec2::new(1200.0, 24.0),
        };
        let (tray, source) = panel_anchor(Some(anchor), Some(polled), monitor, scale);
        assert_eq!(source, Source::Click);
        assert_eq!(tray.center().x, 1200.0, "centred on the pointer");
        assert_eq!(tray.size.height, 58.0, "shape comes from the polled frame");

        let at = menubar::anchor(size, tray, monitor, PANEL_GAP * scale);
        assert_eq!(at.x + size.width / 2.0, 1200.0, "the item marks the middle");
        assert_eq!(at.y, 58.0 + PANEL_GAP * scale, "hangs below the bar");
    }

    #[test]
    fn an_item_in_the_corner_pushes_the_panel_onto_the_screen() {
        // Status items live in a corner, where a 300-point panel cannot be
        // centred: the clamp decides, and the panel ends up flush with the edge.
        let (monitor, scale) = screen();
        let polled = Rect::from_min_size(Vec2::new(2484.0, 0.0), Size::new(60.0, 48.0));
        let anchor = TrayAnchor {
            pointer: Vec2::new(2514.0, 24.0),
        };
        let size = panel_device_size(scale);

        let (tray, source) = panel_anchor(Some(anchor), Some(polled), monitor, scale);
        assert_eq!(
            source,
            Source::Polled,
            "the click landed inside the reading"
        );
        assert!(
            tray.contains(anchor.pointer),
            "the item is inside the panel"
        );

        let at = menubar::anchor(size, tray, monitor, PANEL_GAP * scale);
        assert_eq!(
            at.x + size.width,
            monitor.right(),
            "flush with the screen edge"
        );
        assert_eq!(at.y, 48.0 + PANEL_GAP * scale);
    }

    /// The pointer can land anywhere inside the icon, but the arrow has to
    /// point at the icon: when the status item reports a rectangle the click
    /// sits inside, that rectangle wins and the panel centres on *it*.
    #[test]
    fn a_fresh_reading_centres_the_panel_on_the_item_not_on_the_click() {
        let (monitor, scale) = screen();
        let polled = Rect::from_min_size(Vec2::new(1506.0, 0.0), Size::new(120.0, 58.0));
        let anchor = TrayAnchor {
            // 35 physical pixels left of the item's middle.
            pointer: Vec2::new(1531.0, 24.0),
        };

        let (tray, source) = panel_anchor(Some(anchor), Some(polled), monitor, scale);
        assert_eq!(source, Source::Polled);
        assert_eq!(tray, polled, "the reading stands as it is");

        let size = panel_device_size(scale);
        let at = menubar::anchor(size, tray, monitor, PANEL_GAP * scale);
        assert_eq!(
            at.x + size.width / 2.0,
            1566.0,
            "the item's middle, not the pointer's 1531"
        );
    }

    #[test]
    fn a_click_without_a_usable_frame_still_lands_in_the_menu_bar() {
        // The window was parked in the corner: the click still knows where the
        // item is, and the stand-in supplies the bar's height.
        let (monitor, scale) = screen();
        let anchor = TrayAnchor {
            pointer: Vec2::new(2514.0, 24.0),
        };

        let (tray, source) = panel_anchor(Some(anchor), None, monitor, scale);
        assert_eq!(source, Source::Click);
        assert_eq!(tray.center().x, 2514.0);
        assert_eq!(tray.top(), monitor.top(), "at the top of the menu bar");
        assert_eq!(tray.size.height, FALLBACK_ITEM.2 * scale);
    }

    #[test]
    fn without_a_click_the_polled_frame_is_used() {
        let (monitor, scale) = screen();
        let polled = Rect::from_min_size(Vec2::new(2484.0, 0.0), Size::new(60.0, 48.0));

        let (tray, source) = panel_anchor(None, Some(polled), monitor, scale);
        assert_eq!(source, Source::Polled);
        assert_eq!(tray, polled);
    }

    #[test]
    fn with_nothing_to_go_on_the_panel_starts_at_the_monitors_top_right() {
        // No status item was ever created, so there is nothing to poll; the
        // fallback is worth retrying once the menu bar has been laid out.
        let (monitor, scale) = screen();
        let (tray, source) = panel_anchor(None, None, monitor, scale);

        assert_eq!(source, Source::Fallback);
        assert_eq!(tray.right(), monitor.right() - FALLBACK_ITEM.1 * scale);
        assert_eq!(tray.top(), monitor.top());
    }

    #[test]
    fn the_fallback_item_sits_in_the_monitors_top_right() {
        let monitor = Rect::from_min_size(Vec2::ZERO, Size::new(2560.0, 1664.0));
        let tray = fallback_tray(monitor, 2.0);

        assert_eq!(
            tray.right(),
            2560.0 - 8.0 * 2.0,
            "inset from the right edge"
        );
        assert_eq!(tray.top(), 0.0, "flush with the menu bar");
        assert_eq!(tray.size.height, 24.0 * 2.0);
    }

    /// The wedges of the wake rule: the loop comes back for whichever of the two
    /// deadlines comes first, and parks in `Wait` when neither is pending.
    #[cfg(target_os = "macos")]
    #[test]
    fn the_loop_wakes_for_whichever_deadline_comes_first() {
        let now = Instant::now();
        let soon = now + Duration::from_millis(400);
        let far = now + Duration::from_secs(300);
        let tick = now + COUNTDOWN_TICK;

        assert_eq!(
            wake_at(Some(far), None),
            Some(far),
            "closed: sleep to the refresh"
        );
        assert_eq!(
            wake_at(Some(far), Some(tick)),
            Some(tick),
            "open: the countdown is nearer"
        );
        assert_eq!(
            wake_at(Some(soon), Some(tick)),
            Some(soon),
            "a refresh due before the tick still wins"
        );
        assert_eq!(
            wake_at(None, Some(tick)),
            Some(tick),
            "`--every 0`: no refresh to wait for, but the button still ticks"
        );
        assert_eq!(wake_at(None, None), None, "nothing on screen to animate");
    }

    /// The bug this rule exists for: recomputing `now + 1s` on every batch means
    /// the deadline is pushed forward as fast as the loop wakes, so it is never
    /// reached and the second hand never moves. A stored deadline has to survive
    /// the batches that happen before it falls due.
    #[cfg(target_os = "macos")]
    #[test]
    fn a_stored_tick_deadline_is_not_pushed_forward() {
        let now = Instant::now();
        let due = now + COUNTDOWN_TICK;

        assert_eq!(next_tick(now, Some(due)), due, "still ahead: keep it");
        assert_eq!(
            next_tick(now + Duration::from_millis(500), Some(due)),
            due,
            "half a second later the same instant is still the deadline"
        );
        assert_eq!(
            next_tick(due, Some(due)),
            due + COUNTDOWN_TICK,
            "due: step to the next second"
        );
        assert_eq!(
            next_tick(due + Duration::from_millis(250), Some(due)),
            due + Duration::from_millis(250) + COUNTDOWN_TICK,
            "late (the loop was busy): count from now, not from the missed one"
        );
        assert_eq!(
            next_tick(now, None),
            now + COUNTDOWN_TICK,
            "nothing stored: start a second from now"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn a_fresh_panel_treats_focus_loss_as_a_dismissal() {
        let mut flap = FocusFlap::default();
        assert!(flap.is_dismissal(), "clicking away from an open panel");
    }

    #[test]
    fn the_event_from_creating_the_window_is_not_a_dismissal() {
        // The first click on the status item: the panel is created and opened,
        // then the synthetic event arrives. Before this was handled, the panel
        // hid itself immediately and the click looked like a no-op.
        let mut flap = FocusFlap::default();
        flap.window_created();

        assert!(!flap.is_dismissal(), "winit's own event is swallowed");
        assert!(flap.is_dismissal(), "the next one is a real dismissal");
    }

    #[test]
    fn a_fresh_panel_treats_focus_loss_as_a_dismissal_after_the_flap_clears() {
        let mut flap = FocusFlap::default();
        flap.window_created();
        assert!(!flap.is_dismissal());
        // The panel is dismissed by a click elsewhere and reopened: no new
        // window is created (the host keeps it alive), so a later focus loss
        // must dismiss again.
        assert!(flap.is_dismissal());
        assert!(flap.is_dismissal());
    }

    #[test]
    fn the_fallback_still_hangs_the_panel_off_the_corner() {
        let monitor = Rect::from_min_size(Vec2::ZERO, Size::new(2560.0, 1664.0));
        let scale = 2.0;
        let size = panel_device_size(scale);
        let tray = fallback_tray(monitor, scale);
        let at = menubar::anchor(size, tray, monitor, PANEL_GAP * scale);

        // The stand-in marks the monitor's top-right, where a real status item
        // ends up. A 300-point panel cannot be centred on that corner, so the
        // clamp keeps it on screen instead.
        assert_eq!(at.x + size.width, monitor.right(), "kept on screen");
        assert_eq!(at.y, tray.bottom() + PANEL_GAP * scale);
        assert!(at.x >= 0.0 && at.y >= 0.0, "and stay on screen: {at:?}");
    }

    /// Metal reports `[Opaque, PostMultiplied]`, and picking the first entry —
    /// which is what the plain window keeps — would make the transparent
    /// panel's rounded corners composite as black squares.
    #[test]
    fn the_panel_surface_avoids_the_opaque_alpha_mode() {
        use wgpu::CompositeAlphaMode::{Opaque, PostMultiplied};

        assert_eq!(
            surface_alpha_mode(&[Opaque, PostMultiplied], true),
            PostMultiplied
        );
        assert_eq!(surface_alpha_mode(&[Opaque, PostMultiplied], false), Opaque);
        // A surface offering nothing else still has to be configured with
        // something, even if the corners then cannot be see-through.
        assert_eq!(surface_alpha_mode(&[Opaque], true), Opaque);
        assert_eq!(surface_alpha_mode(&[], true), Opaque);
    }

    /// The clear colour shares the view's backdrop token so the rounded fill
    /// and the pixels around it are the same colour — and only a transparent
    /// window (the panel, the badge) drops the alpha, which is what lets the
    /// corners see through to the desktop.
    #[test]
    fn only_a_transparent_window_clears_to_a_transparent_backdrop() {
        let background = draw_core::Color::new(0.039, 0.039, 0.039, 1.0);

        let panel = clear_color(true, background);
        assert_eq!((panel.r, panel.g, panel.b), (0.039, 0.039, 0.039));
        assert_eq!(panel.a, 0.0, "the corners must be see-through");

        let window = clear_color(false, background);
        assert_eq!(window, background, "an ordinary window stays opaque");
    }
}
