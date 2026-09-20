//! The balance view, built from the workspace's own layers.
//!
//! Everything here is backend-neutral: the view is a `SceneTree` of
//! `draw_components` components laid out by `draw_ui`. The host (see
//! [`crate::host`]) owns the window, the wgpu backend and the network thread,
//! exactly like `examples/wgpu_demo` owns the window for `demo_app`.
//!
//! ```text
//! Input -> BalanceApp::event -> BalanceApp::update -> BalanceApp::layout
//!                                                    -> BalanceApp::paint
//! ```
//!
//! The same view renders into both of the host's surfaces: a normal window, or
//! the borderless panel that drops out of the macOS menu-bar item. Nothing here
//! knows which one it is in, which is the point.
//!
//! ## Refresh flow
//!
//! The view refreshes on open, and the refresh button (or `R` / `F5`) does the
//! same later on: it only flips a shared flag, so the UI never blocks. The host
//! takes that flag after the frame, queries the endpoint on a worker thread, and
//! feeds the result back through [`BalanceApp::apply_result`], which rewrites the
//! text nodes and re-shows the currency cards. The optional [`TextMeasurer`]
//! comes from the backend's real font, so layout matches rendering.
//!
//! ## Countdown
//!
//! The host owns the clock — it knows the interval and when the next automatic
//! refresh is due — and pushes only the time left into the view
//! ([`BalanceApp::set_countdown`]). The view formats it and hides the line when
//! there is no timer, so a window run (no timer) never shows a countdown for a
//! refresh that will not happen.
//!
//! ## Throttle and button state
//!
//! A click, `R` / `F5`, the menu item and the timer all express an *intent*;
//! [`BalanceApp::update`] is the one place that decides whether it goes out. A
//! request is dropped (not queued) while one is in flight, and while the throttle
//! is holding: at most one refresh per [`DEFAULT_MIN_REFRESH_GAP`], counted from
//! the request rather than from the reply. A refused request costs nothing —
//! either the answer is already on its way or the numbers were fetched seconds
//! ago — and the button is the feedback: it counts the wait down
//! (`刷新 (7s)`) and dims itself ([`RefreshState`]) so it does not read as
//! pressable. A *failed* refresh lifts the throttle, because nothing fresh is on
//! screen and the next press is the retry.

use std::cell::Cell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use draw_components::{Button, Card, Column, Component, Divider, Flex, NodeRef, Ref, Row, Text};
use draw_core::{
    Color, Cursor, Edges, EventResult, InputEvent, Key, NodeId, Rect, Size, Transform2D, Vec2,
    ViewportSize,
};
use draw_render::PaintContext;
use draw_scene::{SceneChild, SceneTree};
use draw_theme::{radius, space, Theme, Tone};
use draw_ui::{fill_rounded_rect, Align, Justify, MouseFilter, TextMeasurer, TextOptions};

use crate::api::{self, Balance};

/// Currency cards the layout reserves up front (extra currencies are ignored).
pub const MAX_CURRENCIES: usize = 2;

/// Radius of the panel backdrop this view paints for itself.
///
/// The host's menu-bar panel is a borderless window made transparent (see
/// `host::init_window`), so this rounded fill *is* the panel's shape: outside
/// the corners nothing is painted and the window composites to whatever is
/// behind it. macOS rounds popovers at about this step, and the host draws its
/// shadow from the same alpha, so the two agree.
pub const PANEL_RADIUS: f32 = radius::PANEL;

/// How far the popover arrow's tip stands above the panel body, in logical
/// pixels.
///
/// The arrow is a square rotated 45° with the body covering its lower half, so
/// the wedge comes out `2 * ARROW_HEIGHT` wide — the 2:1 pointed tail macOS
/// popovers use. The host adds this much to the panel window's height, and
/// [`BalanceApp::new_panel`] folds it into the content's top padding so the
/// rows sit the same distance inside the body as they do inside a window.
pub const ARROW_HEIGHT: f32 = 8.0;

/// Shown in the numeric slots before the first successful fetch.
const PLACEHOLDER: &str = "—";
/// Window / page heading.
const TITLE: &str = "DeepSeek 余额";
/// Idle and busy refresh-button labels.
const REFRESH_LABEL: &str = "刷新 (R)";
const REFRESH_BUSY: &str = "刷新中…";
/// Status-line texts.
const STATUS_IDLE: &str = "尚未刷新；点“刷新”或按 R";
const STATUS_BUSY: &str = "刷新中…";
const STATUS_FAILED: &str = "刷新失败";
/// Footer hint: the two environment overrides.
const FOOTER_HINT: &str = "DEEPSEEK_API_KEY / DEEPSEEK_BALANCE_URL 可覆盖默认值";
/// Prefix of the countdown line, e.g. `自动刷新 04:32` (see
/// [`BalanceApp::set_countdown`]).
const COUNTDOWN_PREFIX: &str = "自动刷新";

/// Fewest seconds between two refresh starts, unless the host overrides it with
/// `--min-gap`.
///
/// The panel refreshes every time it opens, and a menu-bar item gets clicked far
/// more often than the numbers change, so without a floor a curious user would
/// hit the endpoint on every toggle. Ten seconds is comfortably shorter than any
/// interval worth watching (`--every` defaults to five minutes) and long enough
/// that a double click, a key repeat, or a re-open cannot stack requests.
pub const DEFAULT_MIN_REFRESH_GAP: Duration = Duration::from_secs(10);

/// Alpha of the wash painted over the refresh button while it is not pressable.
///
/// The button is drawn [`RefreshState::Busy`] and [`RefreshState::Cooling`] with
/// a translucent black film over its accent fill. A film rather than a token
/// swap because `draw_components::Button` has a fixed variant and a fixed label
/// colour: no palette entry can be both legible on `on_accent` and look disabled
/// in both themes. Keeping the accent's hue means the label keeps the contrast it
/// was designed for, and the wash only has to read as "not now".
const DISABLED_WASH_ALPHA: f32 = 0.25;

/// What the refresh button shows, i.e. whether pressing it would do anything.
///
/// One value drives the label *and* the wash its decor paints, so the two can
/// never disagree about what the button is doing.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum RefreshState {
    /// Ready: a press starts a refresh.
    Idle,
    /// A request is in flight.
    Busy,
    /// Throttled: this many whole seconds until the next refresh is allowed.
    ///
    /// The count is in the button's own "hint" slot — the same parentheses that
    /// hold the `R` shortcut when idle — so it reads as "available in", not as
    /// the footer's *automatic* refresh countdown.
    Cooling { left: u64 },
}

impl RefreshState {
    /// Whether a press would start a refresh (and the button is undimmed).
    fn is_idle(self) -> bool {
        matches!(self, Self::Idle)
    }

    /// The label the button carries in this state.
    fn label(self) -> String {
        match self {
            Self::Idle => REFRESH_LABEL.to_string(),
            Self::Busy => REFRESH_BUSY.to_string(),
            Self::Cooling { left } => format!("刷新 ({left}s)"),
        }
    }
}

/// State shared with the click callbacks.
///
/// Callbacks can only capture `'static` values, so they write into these cells
/// and the app reads them once per frame — the same pattern `demo_app` uses.
/// The button's decor is `'static` for the same reason, which is why the visible
/// state lives here too and not on [`BalanceApp`].
#[derive(Clone)]
struct Feed {
    /// A refresh was asked for and has not been handed to the host yet.
    requested: Rc<Cell<bool>>,
    /// A request is in flight (blocks a second one).
    loading: Rc<Cell<bool>>,
    /// What the button currently shows.
    state: Rc<Cell<RefreshState>>,
}

impl Feed {
    fn new() -> Self {
        Self {
            requested: Rc::new(Cell::new(false)),
            loading: Rc::new(Cell::new(false)),
            state: Rc::new(Cell::new(RefreshState::Idle)),
        }
    }
}

/// Node slots the declarative builders fill in at mount time.
///
/// The builders only *write* these; [`BalanceApp::new`] reads them afterwards.
#[derive(Clone, Default)]
struct SlotRefs {
    card: NodeRef,
    currency: NodeRef,
    total: NodeRef,
    granted: NodeRef,
    topped_up: NodeRef,
}

/// One mounted currency card and the labels the refresh rewrites.
struct CurrencySlot {
    card: NodeId,
    currency: NodeId,
    total: NodeId,
    granted: NodeId,
    topped_up: NodeId,
}

/// The balance view: a header, a status line and one card per currency.
pub struct BalanceApp {
    tree: SceneTree,
    theme: Theme,
    feed: Feed,
    /// Whether the view is the menu-bar panel, i.e. paints the popover arrow
    /// above its body.
    arrow: bool,
    refresh_button: NodeId,
    /// The refresh button's own label node.
    ///
    /// `draw_ui::Widget::set_text` writes `Label` and raw `Button` widgets only,
    /// and the themed `Button` is a flex row *wrapping* a label — so the text has
    /// to be written one level down. Found once at mount ([`label_of`]) instead of
    /// every frame.
    refresh_label_node: NodeId,
    status: NodeId,
    error: NodeId,
    countdown: NodeId,
    /// The countdown line currently on screen, so a repeat of the same second
    /// costs nothing. `None` means the line is hidden.
    countdown_line: Option<String>,
    available_ok: NodeId,
    available_bad: NodeId,
    slots: Vec<CurrencySlot>,
    /// Last successful reply, kept so the tests (and the host) can read it back.
    last_balance: Option<Balance>,
    /// Fewest seconds between two refresh starts (see
    /// [`DEFAULT_MIN_REFRESH_GAP`]); [`Duration::ZERO`] turns the throttle off.
    min_gap: Duration,
    /// When the throttle lifts. `None` means "a refresh may start now", which is
    /// also the state after a failure, so the next press retries immediately.
    blocked_until: Option<Instant>,
    /// Test seam: shifts the view's clock, so the cooldown can be driven past
    /// without sleeping. Stays zero in a real run.
    #[cfg(test)]
    clock_shift: Duration,
    viewport: ViewportSize,
}

impl BalanceApp {
    /// Builds the view for `theme`, showing `endpoint` in the header.
    pub fn new(theme: Theme, endpoint: String) -> Self {
        Self::build(theme, endpoint, false)
    }

    /// Builds the view as the menu-bar panel: the same content, plus the
    /// popover arrow above its body (see [`ARROW_HEIGHT`]).
    pub fn new_panel(theme: Theme, endpoint: String) -> Self {
        Self::build(theme, endpoint, true)
    }

    fn build(theme: Theme, endpoint: String, arrow: bool) -> Self {
        let feed = Feed::new();
        let refs = Refs::with_slots();

        // The panel's body starts one arrow-height down, so the content's top
        // padding grows by exactly that: the rows end up the same distance
        // inside the body as they are inside an ordinary window.
        let top = if arrow {
            space::XXL + ARROW_HEIGHT
        } else {
            space::XXL
        };

        // The layout root's own children are placed by anchors, and flex only
        // starts one level down, so the column that arranges the page is the
        // root's single child (the same shape `demo_app` uses).
        let content = Flex::column()
            .mouse_filter(MouseFilter::Ignore)
            .gap(space::LG)
            .padding(Edges::new(space::XXL, top, space::XXL, space::XXL))
            .child(header(theme, &endpoint, &refs, &feed))
            .child(status_row(theme, &refs))
            .child(
                Text::small("", theme)
                    .color(theme.palette.error)
                    .max_lines(2)
                    .ref_(&refs.error),
            )
            .child(currencies(theme, &refs))
            .child(footer(theme, &refs));

        let tree = Flex::column()
            .mouse_filter(MouseFilter::Ignore)
            .child(content)
            .into_tree();

        let slots = refs
            .slots
            .iter()
            .map(|slot| CurrencySlot {
                card: slot.card.get().expect("currency card mounted"),
                currency: slot.currency.get().expect("currency label mounted"),
                total: slot.total.get().expect("total label mounted"),
                granted: slot.granted.get().expect("granted label mounted"),
                topped_up: slot.topped_up.get().expect("topped-up label mounted"),
            })
            .collect();

        let refresh_button = refs.refresh.get().expect("refresh button mounted");
        let refresh_label_node =
            label_of(&tree, refresh_button).expect("the refresh button carries a label");

        let mut app = Self {
            tree,
            theme,
            feed,
            arrow,
            refresh_button,
            refresh_label_node,
            status: refs.status.get().expect("status label mounted"),
            error: refs.error.get().expect("error label mounted"),
            countdown: refs.countdown.get().expect("countdown label mounted"),
            countdown_line: None,
            available_ok: refs.available_ok.get().expect("availability label mounted"),
            available_bad: refs
                .available_bad
                .get()
                .expect("availability label mounted"),
            slots,
            last_balance: None,
            min_gap: DEFAULT_MIN_REFRESH_GAP,
            blocked_until: None,
            #[cfg(test)]
            clock_shift: Duration::ZERO,
            viewport: ViewportSize::new(Size::new(520.0, 460.0)),
        };

        // The button dims itself while it cannot be pressed. Chrome attached
        // after mount rather than a builder method because the themed `Button`
        // resolves its own surface, and because the closure needs the prepared
        // node's id. `paint_front` runs before the label child is painted, so the
        // fill is dimmed and the text stays at full contrast.
        draw_ui::add_decor(
            &mut app.tree,
            app.refresh_button,
            draw_ui::foreground_decor({
                let state = app.feed.state.clone();
                move |ctx, rect, _state| {
                    if state.get().is_idle() {
                        return;
                    }
                    fill_rounded_rect(
                        ctx,
                        rect,
                        radius::MD,
                        Color::BLACK.with_alpha(DISABLED_WASH_ALPHA),
                    );
                }
            }),
        );

        // Before the first fetch: the availability marks are hidden and only the
        // first currency card is on screen, filled with placeholders.
        let (ok, bad) = (app.available_ok, app.available_bad);
        app.set_visible(ok, false);
        app.set_visible(bad, false);
        // The countdown line waits for the host to report a timer.
        let countdown = app.countdown;
        app.set_visible(countdown, false);
        let hidden: Vec<NodeId> = app.slots[1..].iter().map(|slot| slot.card).collect();
        for card in hidden {
            app.set_visible(card, false);
        }

        // Refresh on open, so the window shows real numbers without a click:
        // the first `update` flips to the loading state and the host answers it.
        app.feed.requested.set(true);
        app
    }

    // -- accessors ---------------------------------------------------------

    /// Installs `measurer` so layout measures text with the backend's real font.
    pub fn set_text_measurer(&mut self, measurer: Rc<dyn TextMeasurer>) {
        draw_ui::set_text_measurer(&mut self.tree, measurer);
    }

    /// Whether a request is in flight.
    #[cfg(test)]
    pub fn is_loading(&self) -> bool {
        self.feed.loading.get()
    }

    /// The last successful reply, if any.
    ///
    /// The host mirrors this into the menu-bar title, so it stays readable
    /// after the panel is closed.
    pub fn last_balance(&self) -> Option<&Balance> {
        self.last_balance.as_ref()
    }

    /// One line describing where the view stands: the last error if there is
    /// one, otherwise the status line. The host uses it as the hover tooltip.
    pub fn summary(&self) -> String {
        if let Some(error) = self.error_text().filter(|error| !error.is_empty()) {
            return error.to_string();
        }
        self.status_text().unwrap_or(STATUS_IDLE).to_string()
    }

    /// Current status-line text.
    pub fn status_text(&self) -> Option<&str> {
        text(&self.tree, self.status)
    }

    /// Current error-line text (empty when the last refresh succeeded).
    pub fn error_text(&self) -> Option<&str> {
        text(&self.tree, self.error)
    }

    /// The refresh button's current label (`刷新 (R)` / `刷新中…` / `刷新 (7s)`).
    ///
    /// The host narrates it when it moves, which is how the throttle is visible
    /// in a self-check run that takes no screenshots.
    pub fn refresh_label(&self) -> Option<&str> {
        text(&self.tree, self.refresh_label_node)
    }

    /// Seconds left before the next refresh is allowed, `None` when the button is
    /// ready to be pressed.
    #[cfg(test)]
    pub fn cooldown_left(&self) -> Option<u64> {
        self.blocked_for(self.now())
    }

    /// Whether a refresh would go out right now.
    ///
    /// The host mirrors this onto the status item's `刷新余额` entry, which is the
    /// only surface that can explain a refusal while the panel is closed.
    pub fn refresh_ready(&self) -> bool {
        self.can_refresh(self.now())
    }

    /// Whether the button is dimmed, i.e. shows [`RefreshState::Busy`] or
    /// [`RefreshState::Cooling`].
    #[cfg(test)]
    pub fn is_throttled(&self) -> bool {
        !self.feed.state.get().is_idle()
    }

    /// Current countdown-line text (`None` while the line is hidden).
    ///
    /// The host reads it back to narrate the timer in a self-check run.
    pub fn countdown_text(&self) -> Option<&str> {
        text(&self.tree, self.countdown)
    }

    /// Whether the countdown line is on screen.
    #[cfg(test)]
    pub fn countdown_visible(&self) -> Option<bool> {
        self.tree.is_visible(self.countdown)
    }

    /// Total for currency card `index`.
    #[cfg(test)]
    pub fn total_text(&self, index: usize) -> Option<&str> {
        self.slots
            .get(index)
            .and_then(|slot| text(&self.tree, slot.total))
    }

    /// Whether currency card `index` is on screen.
    #[cfg(test)]
    pub fn card_visible(&self, index: usize) -> Option<bool> {
        self.slots
            .get(index)
            .and_then(|slot| self.tree.is_visible(slot.card))
    }

    /// Center of the refresh button in logical viewport coordinates.
    #[cfg(test)]
    pub fn button_center(&self) -> Option<Vec2> {
        draw_ui::control(&self.tree, self.refresh_button).map(|control| control.rect.center())
    }

    /// Rect of the refresh button.
    #[cfg(test)]
    pub fn button_rect(&self) -> Option<Rect> {
        draw_ui::control(&self.tree, self.refresh_button).map(|control| control.rect)
    }

    // -- pipeline ----------------------------------------------------------

    /// Advances the frame, settles whether the pending refresh may go out, and
    /// refreshes the button's label.
    ///
    /// Returns whether that label moved, so the host can narrate the throttle in
    /// a self-check run (the same contract as [`BalanceApp::set_countdown`]).
    pub fn update(&mut self, viewport: ViewportSize, dt: f32) -> bool {
        let _ = dt;
        self.viewport = viewport;
        let now = self.now();

        // A request that the throttle refuses is dropped here rather than left
        // for the host: `take_refresh_request` is what starts the worker thread,
        // so clearing the flag is what keeps a refused click off the network.
        if self.feed.requested.get() && !self.can_refresh(now) {
            self.feed.requested.set(false);
        }
        if self.feed.requested.get() {
            self.begin_refresh(now);
        }
        self.sync_button(now)
    }

    /// Resolves layout for `viewport`.
    pub fn layout(&mut self, viewport: ViewportSize) {
        self.viewport = viewport;
        draw_ui::layout(&mut self.tree, viewport);
        self.tree.update();
    }

    /// Emits this frame's `DrawList` into `ctx`: panel shape, then UI content.
    ///
    /// The backdrop is a rounded rect rather than a plain fill so the panel's
    /// corners stay transparent on a transparent window; in an ordinary window
    /// the clear colour matches the fill, so the corners are simply invisible.
    /// As a panel there is also an arrow: the same rounded body, one arrow
    /// height lower, with the wedge sticking up above it.
    pub fn paint(&self, ctx: &mut PaintContext) {
        let window = Rect::from_min_size(Vec2::ZERO, self.viewport.logical_size());
        let body = self.body(window);
        let fill = self.theme.palette.background;

        // The arrow first, so the body hides the half of the rotated square
        // that would stick back down into it.
        if self.arrow {
            arrow(ctx, window, fill);
        }
        fill_rounded_rect(ctx, body, PANEL_RADIUS, fill);
        draw_ui::paint(&self.tree, ctx);
    }

    /// The part of the window the panel's rounded body covers: everything, or
    /// everything below the arrow.
    pub fn body(&self, window: Rect) -> Rect {
        if self.arrow {
            Rect::from_min_max(
                Vec2::new(window.left(), window.top() + ARROW_HEIGHT),
                Vec2::new(window.right(), window.bottom()),
            )
        } else {
            window
        }
    }

    /// Routes an input event: `R` / `F5` ask for a refresh, everything else goes
    /// through the normal UI input path.
    ///
    /// The key only records the intent — [`BalanceApp::update`] decides whether it
    /// survives the throttle, so the shortcut and the button are gated by exactly
    /// the same rule.
    pub fn event(&mut self, event: &InputEvent) -> EventResult {
        if let InputEvent::KeyDown { key } = event {
            if matches!(key, Key::F5 | Key::Character('r')) {
                self.request_refresh();
                return EventResult::Handled;
            }
        }
        draw_ui::route_input(&mut self.tree, event)
    }

    /// Controls in the view. The self-check feeds this into the frame
    /// counters, so it is a plain read-only accessor, not test-only.
    pub fn control_count(&self) -> usize {
        draw_ui::control_count(&self.tree)
    }

    /// Read-only view of the scene tree, for the self-check's dump.
    pub fn tree(&self) -> &draw_scene::SceneTree {
        &self.tree
    }

    /// Cursor the host should show for the current pointer position.
    pub fn cursor(&self) -> Cursor {
        draw_ui::hovered_cursor(&self.tree)
    }

    /// The theme the view was built with. The host reads it to match its own
    /// clear colour to the panel backdrop.
    pub fn theme(&self) -> &Theme {
        &self.theme
    }

    // -- refresh -----------------------------------------------------------

    /// The view's clock.
    ///
    /// The throttle has to be judged where requests are decided *and* be testable
    /// without sleeping, so the one read of the clock lives here, with a
    /// test-only shift. `std::time` is deliberate: the view already reads the
    /// wall clock for the "refreshed at" stamp.
    fn now(&self) -> Instant {
        let now = Instant::now();
        #[cfg(test)]
        let now = now + self.clock_shift;
        now
    }

    /// Whether a refresh may start right now: nothing in flight and the throttle
    /// is not holding.
    fn can_refresh(&self, now: Instant) -> bool {
        !self.feed.loading.get() && self.blocked_for(now).is_none()
    }

    /// Whole seconds left of the throttle, `None` when a refresh is allowed.
    ///
    /// Rounded up, like [`clock`], so the button never reads `(0s)` while a
    /// sliver of the gap is still left.
    fn blocked_for(&self, now: Instant) -> Option<u64> {
        let left = self.blocked_until?.checked_duration_since(now)?;
        let seconds = left.as_secs() + u64::from(left.subsec_millis() > 0);
        (seconds > 0).then_some(seconds)
    }

    /// Marks the view as loading and arms the throttle.
    ///
    /// The gap starts with the request, not with the reply: it is about how often
    /// the endpoint is hit, and a round trip is a fraction of the wait anyway.
    fn begin_refresh(&mut self, now: Instant) {
        self.feed.loading.set(true);
        self.blocked_until = Some(now + self.min_gap);
        draw_components::set_text(&mut self.tree, self.status, STATUS_BUSY);
        draw_components::set_text(&mut self.tree, self.error, "");
    }

    /// Writes the button's state into its label and into the cell its decor
    /// reads. Returns whether the label moved.
    fn sync_button(&mut self, now: Instant) -> bool {
        let state = if self.feed.loading.get() {
            RefreshState::Busy
        } else {
            match self.blocked_for(now) {
                Some(left) => RefreshState::Cooling { left },
                None => RefreshState::Idle,
            }
        };
        if self.feed.state.get() == state {
            return false;
        }
        self.feed.state.set(state);
        draw_components::set_text(&mut self.tree, self.refresh_label_node, state.label());
        true
    }

    /// Asks for a refresh from outside the view (the menu bar's timer, or the
    /// menu item). This only records the intent — press, key, menu and timer all
    /// funnel through [`BalanceApp::update`], which is where the throttle is
    /// applied — and nothing is sent until [`BalanceApp::take_refresh_request`].
    pub fn request_refresh(&mut self) {
        self.feed.requested.set(true);
    }

    /// Sets the minimum gap between two refresh starts. [`Duration::ZERO`] turns
    /// the throttle off; the host maps `--min-gap` onto this.
    pub fn set_min_refresh_gap(&mut self, gap: Duration) {
        self.min_gap = gap;
    }

    /// Shows the time left before the host's next automatic refresh, or hides
    /// the line when there is no timer (`None`).
    ///
    /// The host owns the clock, so it calls this once per event batch; the same
    /// second formatted twice is a no-op. Returns whether the line changed, so
    /// the host only schedules a frame when the countdown actually moved.
    pub fn set_countdown(&mut self, remaining: Option<Duration>) -> bool {
        let line = remaining.map(|left| format!("{COUNTDOWN_PREFIX} {}", clock(left)));
        if line == self.countdown_line {
            return false;
        }
        match line.as_deref() {
            Some(text) => {
                draw_components::set_text(&mut self.tree, self.countdown, text);
                self.set_visible(self.countdown, true);
            }
            None => self.set_visible(self.countdown, false),
        }
        self.countdown_line = line;
        true
    }

    /// Takes the pending refresh request. The host calls this after `update` and
    /// queries the endpoint off-thread when it returns `true`.
    pub fn take_refresh_request(&mut self) -> bool {
        self.feed.requested.replace(false)
    }

    /// Applies a finished request: on success the cards are rewritten, on
    /// failure the error line explains why (the last numbers stay on screen).
    pub fn apply_result(&mut self, result: Result<Balance, String>) {
        self.feed.loading.set(false);
        self.feed.requested.set(false);

        match result {
            Ok(balance) => self.show_balance(balance),
            Err(message) => {
                // A failed attempt left nothing fresh on screen, so it does not
                // hold the throttle: the button is pressable again at once, and
                // that press is the retry. There is still only one request in
                // flight at a time, which is what the `loading` gate is for.
                self.blocked_until = None;
                draw_components::set_text(&mut self.tree, self.status, STATUS_FAILED);
                draw_components::set_text(&mut self.tree, self.error, message);
            }
        }

        self.sync_button(self.now());
    }

    /// Writes `balance` into the labels and shows exactly the cards it has data
    /// for.
    fn show_balance(&mut self, balance: Balance) {
        let stamp = format!(
            "更新于 {} · {} 种币种",
            api::timestamp(),
            balance.balance_infos.len()
        );
        draw_components::set_text(&mut self.tree, self.status, stamp);
        draw_components::set_text(&mut self.tree, self.error, "");

        let (ok, bad) = (self.available_ok, self.available_bad);
        self.set_visible(ok, balance.is_available);
        self.set_visible(bad, !balance.is_available);

        // Copy the ids out first: `set_visible` needs `&mut self`.
        let slots: Vec<(NodeId, NodeId, NodeId, NodeId, NodeId)> = self
            .slots
            .iter()
            .map(|slot| {
                (
                    slot.card,
                    slot.currency,
                    slot.total,
                    slot.granted,
                    slot.topped_up,
                )
            })
            .collect();

        for (index, (card, currency, total, granted, topped_up)) in slots.into_iter().enumerate() {
            match balance.balance_infos.get(index) {
                Some(info) => {
                    draw_components::set_text(&mut self.tree, currency, info.currency.clone());
                    draw_components::set_text(&mut self.tree, total, info.total_balance.clone());
                    draw_components::set_text(
                        &mut self.tree,
                        granted,
                        info.granted_balance.clone(),
                    );
                    draw_components::set_text(
                        &mut self.tree,
                        topped_up,
                        info.topped_up_balance.clone(),
                    );
                    self.set_visible(card, true);
                }
                None => self.set_visible(card, false),
            }
        }

        self.last_balance = Some(balance);
    }

    /// Shows or hides `id`, invalidating layout exactly when the value changes
    /// (the pattern `draw_components::Router` uses).
    fn set_visible(&mut self, id: NodeId, visible: bool) {
        if self.tree.is_visible(id) != Some(visible) {
            self.tree.set_visible(id, visible);
            draw_ui::mark_dirty(&mut self.tree, id);
        }
    }
}

/// Node slots the declarative builders write into.
#[derive(Clone, Default)]
struct Refs {
    refresh: NodeRef,
    status: NodeRef,
    error: NodeRef,
    countdown: NodeRef,
    available_ok: NodeRef,
    available_bad: NodeRef,
    slots: Vec<SlotRefs>,
}

/// `Refs::default()` cannot size the slot vector, so it is patched here.
impl Refs {
    fn with_slots() -> Self {
        Self {
            slots: (0..MAX_CURRENCIES).map(|_| SlotRefs::default()).collect(),
            ..Self::default()
        }
    }
}

/// The header: page title, endpoint, and the refresh button.
fn header(theme: Theme, endpoint: &str, refs: &Refs, feed: &Feed) -> Row {
    let requested = feed.requested.clone();

    let title = Column::new()
        .gap(space::XXS)
        .grow(1.0)
        .child(Text::title(TITLE, theme).max_lines(1).ellipsis(true))
        .child(
            Text::caption(endpoint, theme)
                .tone(Tone::Subtle)
                .text_options(TextOptions::no_wrap())
                .max_lines(1)
                .ellipsis(true),
        );

    let button = Button::primary(REFRESH_LABEL, theme)
        // A press is an intent, not a command: whether it goes out is decided in
        // `update`, which owns the clock and therefore the throttle.
        .on_click(move || requested.set(true))
        .ref_(&refs.refresh);

    Row::new()
        .align(Align::Center)
        .gap(space::LG)
        .child(title)
        .child(button)
}

/// The status line: availability mark plus "refreshed at" text.
fn status_row(theme: Theme, refs: &Refs) -> Row {
    Row::new()
        .align(Align::Center)
        .gap(space::MD)
        .child(
            Text::small("账户可用", theme)
                .color(theme.palette.success)
                .ref_(&refs.available_ok),
        )
        .child(
            Text::small("账户不可用", theme)
                .color(theme.palette.error)
                .ref_(&refs.available_bad),
        )
        .child(
            Text::caption(STATUS_IDLE, theme)
                .tone(Tone::Muted)
                .ref_(&refs.status),
        )
}

/// The card column: one card per currency, filling the remaining height.
fn currencies(theme: Theme, refs: &Refs) -> Column {
    let cards = refs.slots.iter().map(|slot| currency_card(theme, slot));

    Column::new()
        .gap(space::MD)
        .grow(1.0)
        .mouse_filter(MouseFilter::Ignore)
        .children(cards)
}

/// The footer: what the environment can override, then how long until the next
/// automatic refresh. The countdown node is hidden unless the host starts a
/// timer, so this reads as one line in window mode.
fn footer(theme: Theme, refs: &Refs) -> Column {
    Column::new()
        .gap(space::XXS)
        .child(Text::caption(FOOTER_HINT, theme).tone(Tone::Subtle))
        .child(
            Text::caption("", theme)
                .tone(Tone::Subtle)
                .ref_(&refs.countdown),
        )
}

/// A countdown as `MM:SS`, or `H:MM:SS` past an hour.
///
/// Rounded up, so a countdown never reads `00:00` while a second is still left:
/// with truncation the final value would be on screen for two seconds.
fn clock(remaining: Duration) -> String {
    let seconds = remaining.as_secs() + u64::from(remaining.subsec_millis() > 0);
    let (hours, minutes, seconds) = (seconds / 3_600, (seconds % 3_600) / 60, seconds % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes:02}:{seconds:02}")
    }
}

/// One currency card: total on top, then the granted / topped-up breakdown.
fn currency_card(theme: Theme, refs: &SlotRefs) -> Ref<Card> {
    Card::new(theme)
        .gap(space::SM)
        .child(
            Row::new()
                .align(Align::Center)
                .justify(Justify::SpaceBetween)
                .child(Text::subheading(PLACEHOLDER, theme).ref_(&refs.currency))
                .child(Text::caption("总余额", theme).tone(Tone::Muted)),
        )
        .child(Text::display(PLACEHOLDER, theme).ref_(&refs.total))
        .child(Divider::horizontal(theme))
        .child(stat_row(theme, "赠送余额", &refs.granted))
        .child(stat_row(theme, "充值余额", &refs.topped_up))
        .ref_(&refs.card)
}

/// A "label … value" row inside a currency card.
fn stat_row(theme: Theme, label: &str, value: &NodeRef) -> Row {
    Row::new()
        .align(Align::Center)
        .justify(Justify::SpaceBetween)
        .gap(space::SM)
        .child(Text::small(label, theme).tone(Tone::Muted))
        .child(Text::small(PLACEHOLDER, theme).ref_(value))
}

/// Reads a label's text back out of the tree.
fn text(tree: &SceneTree, id: NodeId) -> Option<&str> {
    draw_ui::widget(tree, id).and_then(|widget| widget.text())
}

/// The first text-bearing child of `control`, i.e. the label a composite
/// component wraps.
///
/// `draw_components::Button` builds a flex row and puts the caption in a child
/// label, and `Widget::set_text` only writes `Label` and raw `Button` widgets —
/// so writing the caption means writing to this node, not to the button.
fn label_of(tree: &SceneTree, control: NodeId) -> Option<NodeId> {
    tree.children(control)?
        .iter()
        .copied()
        .find(|child| text(tree, *child).is_some())
}

/// Paints the popover arrow: a square rotated 45° whose tip touches the top of
/// the window, so the visible part — the half above the body — is a wedge
/// `2 * ARROW_HEIGHT` wide pointing at the status item.
///
/// The render IR has no triangle, but `FillRect` under a rotation is one, and
/// `Save`/`Restore` keeps the rotation away from the content painted after it.
/// This is only correct because the host centres the panel on the item, so the
/// window's middle is where the item is.
fn arrow(ctx: &mut PaintContext, window: Rect, fill: Color) {
    let centre = Vec2::new(window.center().x, window.top() + ARROW_HEIGHT);
    // Rotating the square by 45° turns its half-diagonal into the wedge's half
    // width, so the side is the half-diagonal over cos(45°).
    let side = ARROW_HEIGHT * std::f32::consts::SQRT_2;
    ctx.save();
    ctx.set_transform(Transform2D::from_rotation_origin(
        std::f32::consts::FRAC_PI_4,
        centre,
    ));
    ctx.fill_rect(
        Rect::from_center_size(Vec2::ZERO, Size::new(side, side)),
        fill,
    );
    ctx.restore();
}

/// Left padding used by the test helpers to place a pointer.
#[cfg(test)]
fn viewport(width: f32, height: f32) -> ViewportSize {
    ViewportSize::new(Size::new(width, height))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::BalanceInfo;
    use draw_core::PointerButton;
    use draw_render::DrawCommand;

    /// Endpoint used by the headless tests (never fetched).
    const TEST_ENDPOINT: &str = "https://api.deepseek.com/user/balance";

    /// The body size the host asks for when it wants the panel (mirrors
    /// `host::PANEL_WIDTH` / `host::PANEL_HEIGHT`).
    const PANEL_WIDTH_TEST: f32 = 300.0;
    const PANEL_HEIGHT_TEST: f32 = 420.0;

    fn sample() -> Balance {
        Balance {
            is_available: true,
            balance_infos: vec![
                BalanceInfo {
                    currency: "CNY".to_string(),
                    total_balance: "110.00".to_string(),
                    granted_balance: "10.00".to_string(),
                    topped_up_balance: "100.00".to_string(),
                },
                BalanceInfo {
                    currency: "USD".to_string(),
                    total_balance: "7.00".to_string(),
                    granted_balance: "0.00".to_string(),
                    topped_up_balance: "7.00".to_string(),
                },
            ],
        }
    }

    /// A view laid out at `width` x `height`, with the on-open refresh still
    /// pending (the state the first frame sees).
    fn laid_out(width: f32, height: f32) -> BalanceApp {
        let mut app = BalanceApp::new(Theme::dark(), TEST_ENDPOINT.to_string());
        let viewport = viewport(width, height);
        app.update(viewport, 0.016);
        app.layout(viewport);
        app
    }

    /// A view whose on-open refresh has already been answered, so the tests
    /// below start from a settled screen. The throttle is still armed: this is
    /// what the panel looks like a second after it opens.
    fn settled(width: f32, height: f32) -> BalanceApp {
        let mut app = laid_out(width, height);
        assert!(app.take_refresh_request(), "the view refreshes on open");
        app.apply_result(Ok(sample()));
        app.layout(viewport(width, height));
        app
    }

    /// [`settled`], with the throttle run out: the button is ready to be pressed.
    ///
    /// Time is shifted rather than slept through — the view reads its clock in one
    /// place (`BalanceApp::now`), and this is that seam.
    fn ready(width: f32, height: f32) -> BalanceApp {
        let mut app = settled(width, height);
        app.clock_shift = DEFAULT_MIN_REFRESH_GAP;
        app.update(viewport(width, height), 0.016);
        app.layout(viewport(width, height));
        assert_eq!(app.cooldown_left(), None, "past the gap");
        assert_eq!(app.refresh_label(), Some(REFRESH_LABEL));
        app
    }

    /// Whether the draw list washes `rect`, i.e. paints the disabled film over it.
    fn washed(app: &BalanceApp, rect: Rect) -> bool {
        let mut ctx = PaintContext::new();
        app.paint(&mut ctx);
        let list = ctx.into_draw_list();
        list.commands().iter().any(|command| {
            matches!(
                command,
                DrawCommand::FillRoundedRect { rect: fill, paint, .. }
                    if *fill == rect
                        && paint.color == Color::BLACK.with_alpha(DISABLED_WASH_ALPHA)
            )
        })
    }

    fn click(app: &mut BalanceApp, position: Vec2) {
        app.event(&InputEvent::PointerDown {
            position,
            button: PointerButton::Left,
        });
        app.event(&InputEvent::PointerUp {
            position,
            button: PointerButton::Left,
        });
    }

    #[test]
    fn the_view_mounts_with_placeholders() {
        // Before the first frame: idle status, dashes, one card on screen.
        let app = BalanceApp::new(Theme::dark(), TEST_ENDPOINT.to_string());
        assert_eq!(app.total_text(0), Some(PLACEHOLDER));
        assert_eq!(app.card_visible(0), Some(true));
        assert_eq!(app.card_visible(1), Some(false));
        assert_eq!(app.status_text(), Some(STATUS_IDLE));
        assert_eq!(app.error_text(), Some(""));
        assert!(!app.is_loading());
    }

    #[test]
    fn the_view_refreshes_on_open() {
        let mut app = laid_out(520.0, 460.0);
        assert!(app.is_loading(), "the first frame starts a refresh");
        assert_eq!(app.status_text(), Some(STATUS_BUSY));
        assert!(app.take_refresh_request(), "and hands it to the host");
        assert!(!app.take_refresh_request(), "only once");
        assert!(app.control_count() > 0);
    }

    #[test]
    fn the_refresh_button_requests_a_refresh() {
        let mut app = ready(520.0, 460.0);
        let center = app.button_center().expect("button rect");

        click(&mut app, center);
        assert!(!app.is_loading(), "a click only records the request");

        app.update(viewport(520.0, 460.0), 0.016);
        assert!(app.is_loading(), "update enters the loading state");
        assert_eq!(app.status_text(), Some(STATUS_BUSY));
        assert!(app.take_refresh_request(), "the host takes the request");
        assert!(!app.take_refresh_request(), "and only once");
    }

    #[test]
    fn the_r_key_requests_a_refresh() {
        let mut app = ready(520.0, 460.0);
        app.event(&InputEvent::KeyDown {
            key: Key::Character('r'),
        });
        app.update(viewport(520.0, 460.0), 0.016);
        assert!(app.is_loading());
        assert!(app.take_refresh_request());
    }

    #[test]
    fn a_second_request_is_ignored_while_loading() {
        let mut app = ready(520.0, 460.0);
        let center = app.button_center().expect("button rect");

        click(&mut app, center);
        app.update(viewport(520.0, 460.0), 0.016);
        assert!(app.take_refresh_request());

        click(&mut app, center);
        app.update(viewport(520.0, 460.0), 0.016);
        assert!(
            !app.take_refresh_request(),
            "no parallel request while busy"
        );
    }

    /// The panel refreshes when it opens, and that request arms the throttle:
    /// toggling the item again a second later shows the numbers it already has
    /// instead of hitting the endpoint twice.
    #[test]
    fn the_open_refresh_arms_the_throttle() {
        let app = laid_out(520.0, 460.0);
        assert!(app.is_loading(), "the first frame starts a refresh");
        assert_eq!(app.refresh_label(), Some(REFRESH_BUSY));
        assert_eq!(
            app.cooldown_left(),
            Some(DEFAULT_MIN_REFRESH_GAP.as_secs()),
            "the gap starts with the request"
        );
        assert!(app.is_throttled());
    }

    /// The wait is counted in the button's own hint slot, one second at a time,
    /// and the button goes back to `刷新 (R)` when it runs out.
    #[test]
    fn the_button_counts_the_throttle_down() {
        let mut app = settled(520.0, 460.0);
        assert_eq!(app.refresh_label(), Some("刷新 (10s)"));

        app.clock_shift = Duration::from_secs(4);
        app.update(viewport(520.0, 460.0), 0.016);
        assert_eq!(app.refresh_label(), Some("刷新 (6s)"));

        app.clock_shift = Duration::from_secs(9);
        app.update(viewport(520.0, 460.0), 0.016);
        assert_eq!(
            app.refresh_label(),
            Some("刷新 (1s)"),
            "rounded up, never 0s"
        );

        app.clock_shift = DEFAULT_MIN_REFRESH_GAP;
        app.update(viewport(520.0, 460.0), 0.016);
        assert_eq!(app.refresh_label(), Some(REFRESH_LABEL));
        assert_eq!(app.cooldown_left(), None);
        assert!(!app.is_throttled());
    }

    /// A press the throttle refuses must not reach the host — `take_refresh_request`
    /// is what spawns the worker thread — and the button keeps saying why.
    #[test]
    fn a_click_inside_the_throttle_is_dropped() {
        let mut app = settled(520.0, 460.0);
        let center = app.button_center().expect("button rect");

        click(&mut app, center);
        app.update(viewport(520.0, 460.0), 0.016);

        assert!(!app.is_loading(), "the throttle holds the request back");
        assert!(!app.take_refresh_request(), "and nothing goes out");
        assert_eq!(app.refresh_label(), Some("刷新 (10s)"));
        assert_eq!(
            app.status_text().map(|text| text.starts_with("更新于 ")),
            Some(true),
            "the stamp of the last good refresh is untouched"
        );
    }

    #[test]
    fn the_r_key_inside_the_throttle_is_dropped() {
        let mut app = settled(520.0, 460.0);
        app.event(&InputEvent::KeyDown {
            key: Key::Character('r'),
        });
        app.update(viewport(520.0, 460.0), 0.016);
        assert!(!app.is_loading());
        assert!(!app.take_refresh_request());
    }

    #[test]
    fn a_press_goes_through_once_the_throttle_runs_out() {
        let mut app = settled(520.0, 460.0);
        let center = app.button_center().expect("button rect");

        app.clock_shift = DEFAULT_MIN_REFRESH_GAP;
        click(&mut app, center);
        app.update(viewport(520.0, 460.0), 0.016);

        assert!(app.is_loading(), "the wait is over, so the press counts");
        assert!(app.take_refresh_request());
    }

    /// The host's timer funnels through the same gate: a manual refresh seconds
    /// before it is due makes the automatic one unnecessary, not queued.
    #[test]
    fn a_timer_tick_inside_the_throttle_is_dropped() {
        let mut app = settled(520.0, 460.0);
        app.request_refresh();
        app.update(viewport(520.0, 460.0), 0.016);
        assert!(!app.is_loading());
        assert!(!app.take_refresh_request());
    }

    /// A failed attempt leaves nothing fresh on screen, so it must not hold the
    /// throttle: the next press is the retry.
    #[test]
    fn a_failure_lifts_the_throttle() {
        let mut app = settled(520.0, 460.0);
        assert_eq!(app.cooldown_left(), Some(10));

        app.apply_result(Err("HTTP 500: internal error".to_string()));

        assert_eq!(app.cooldown_left(), None);
        assert_eq!(app.refresh_label(), Some(REFRESH_LABEL));
        assert!(!app.is_throttled());
    }

    /// `--min-gap 0` is "no wait", not "no gate": two requests still never run at
    /// the same time, because the second one is refused while the first is in
    /// flight. The gap has to be set before the refresh that arms it.
    #[test]
    fn a_zero_gap_turns_the_throttle_off() {
        let mut app = BalanceApp::new(Theme::dark(), TEST_ENDPOINT.to_string());
        app.set_min_refresh_gap(Duration::ZERO);
        let viewport = viewport(520.0, 460.0);
        app.update(viewport, 0.016);
        assert!(
            app.take_refresh_request(),
            "the open refresh still goes out"
        );
        app.apply_result(Ok(sample()));
        app.layout(viewport);
        assert_eq!(app.cooldown_left(), None, "nothing to wait for");

        let center = app.button_center().expect("button rect");
        click(&mut app, center);
        app.update(viewport, 0.016);
        assert!(app.is_loading(), "the press goes straight out");
        assert!(app.take_refresh_request());
        assert_eq!(app.refresh_label(), Some(REFRESH_BUSY));
    }

    /// The wash is what makes "cannot press" visible: without it the button keeps
    /// its full accent while the throttle silently ignores the click.
    #[test]
    fn the_button_is_washed_while_it_cannot_be_pressed() {
        let mut app = settled(520.0, 460.0);
        let rect = app.button_rect().expect("button rect");
        assert!(app.is_throttled());
        assert!(washed(&app, rect), "the cooling button is dimmed");

        // And undimmed again once the wait is over.
        app.clock_shift = DEFAULT_MIN_REFRESH_GAP;
        app.update(viewport(520.0, 460.0), 0.016);
        app.layout(viewport(520.0, 460.0));
        let rect = app.button_rect().expect("button rect");
        assert!(!app.is_throttled());
        assert!(!washed(&app, rect), "a ready button is left alone");
    }

    /// The wash is chrome, not a backdrop: it covers the button and nothing else,
    /// and it is drawn before the label child, so the text keeps its contrast.
    #[test]
    fn the_wash_covers_the_button_and_not_the_page() {
        let app = laid_out(520.0, 460.0);
        let button = app.button_rect().expect("button rect");
        assert!(washed(&app, button));

        let mut ctx = PaintContext::new();
        app.paint(&mut ctx);
        let list = ctx.into_draw_list();
        let washes = list
            .commands()
            .iter()
            .filter(|command| {
                matches!(
                    command,
                    DrawCommand::FillRoundedRect { paint, .. }
                        if paint.color == Color::BLACK.with_alpha(DISABLED_WASH_ALPHA)
                )
            })
            .count();
        assert_eq!(washes, 1, "one film over one button");
    }

    #[test]
    fn applying_a_balance_rewrites_the_cards() {
        let mut app = settled(520.0, 460.0);
        app.apply_result(Ok(sample()));

        assert_eq!(app.total_text(0), Some("110.00"));
        assert_eq!(app.total_text(1), Some("7.00"));
        assert_eq!(app.card_visible(1), Some(true));
        assert_eq!(app.error_text(), Some(""));
        assert!(!app.is_loading());
        let status = app.status_text().expect("status");
        assert!(status.starts_with("更新于 "), "{status}");
        // The stamp is wall-clock Shanghai time, not UTC.
        assert!(status.contains(" UTC+8 · "), "{status}");
        assert_eq!(app.last_balance(), Some(&sample()));
    }

    #[test]
    fn one_currency_hides_the_spare_card() {
        let mut app = settled(520.0, 460.0);
        let mut single = sample();
        single.balance_infos.truncate(1);

        app.apply_result(Ok(single));
        assert_eq!(app.total_text(0), Some("110.00"));
        assert_eq!(app.card_visible(1), Some(false));
    }

    #[test]
    fn a_failure_keeps_the_last_numbers_and_shows_the_reason() {
        let mut app = settled(520.0, 460.0);
        app.apply_result(Err("HTTP 401: unauthorized".to_string()));

        assert_eq!(app.status_text(), Some(STATUS_FAILED));
        assert_eq!(app.error_text(), Some("HTTP 401: unauthorized"));
        assert_eq!(app.total_text(0), Some("110.00"), "last good values stay");
        assert!(!app.is_loading());
    }

    /// Until the host reports an interval there is nothing to count down, so the
    /// line stays out of the layout instead of promising a refresh that a window
    /// run (no timer) would never make.
    #[test]
    fn the_countdown_line_is_hidden_until_the_host_starts_a_timer() {
        let mut app = settled(520.0, 460.0);
        assert_eq!(app.countdown_visible(), Some(false));

        assert!(app.set_countdown(Some(Duration::from_secs(300))));
        assert_eq!(app.countdown_visible(), Some(true));
        assert_eq!(app.countdown_text(), Some("自动刷新 05:00"));
    }

    /// The host calls this every batch; only a move of the displayed second is
    /// worth a frame.
    #[test]
    fn the_countdown_line_only_moves_once_a_second() {
        let mut app = settled(520.0, 460.0);
        assert!(app.set_countdown(Some(Duration::from_millis(4_500))));
        assert_eq!(app.countdown_text(), Some("自动刷新 00:05"));

        assert!(
            !app.set_countdown(Some(Duration::from_millis(4_100))),
            "the same second on screen does not need a frame"
        );
        assert!(app.set_countdown(Some(Duration::from_millis(3_900))));
        assert_eq!(app.countdown_text(), Some("自动刷新 00:04"));
    }

    #[test]
    fn the_countdown_line_goes_away_when_the_timer_stops() {
        let mut app = settled(520.0, 460.0);
        app.set_countdown(Some(Duration::from_secs(12)));

        assert!(app.set_countdown(None));
        assert_eq!(app.countdown_visible(), Some(false));
        assert!(!app.set_countdown(None), "and stays put");
    }

    #[test]
    fn the_countdown_reads_out_mm_ss() {
        let cases = [
            (Duration::ZERO, "00:00"),
            (Duration::from_millis(1), "00:01"),
            (Duration::from_secs(5), "00:05"),
            (Duration::from_secs(59), "00:59"),
            (Duration::from_secs(60), "01:00"),
            (Duration::from_secs(300), "05:00"),
            (Duration::from_secs(3_599), "59:59"),
            (Duration::from_secs(3_600), "1:00:00"),
            (Duration::from_secs(3_661), "1:01:01"),
            // Rounded up: 4.0s is `00:04` for one second, not two.
            (Duration::from_millis(4_000), "00:04"),
            (Duration::from_millis(4_001), "00:05"),
        ];
        for (remaining, expected) in cases {
            assert_eq!(clock(remaining), expected, "for {remaining:?}");
        }
    }

    /// The countdown is one more row in the panel, and the panel is sized for
    /// it: the row has to land below the cards and stay inside the window. A
    /// transparent, non-scrolling window clips whatever crosses its edge, so a
    /// row that fell off would simply never be seen.
    #[test]
    fn the_countdown_line_sits_inside_the_panel() {
        let mut app = panel();
        app.set_countdown(Some(Duration::from_secs(300)));
        let viewport = viewport(PANEL_WIDTH_TEST, PANEL_HEIGHT_TEST + ARROW_HEIGHT);
        app.layout(viewport);

        let window = Rect::from_min_size(
            Vec2::ZERO,
            Size::new(PANEL_WIDTH_TEST, PANEL_HEIGHT_TEST + ARROW_HEIGHT),
        );
        let line = draw_ui::control(&app.tree, app.countdown)
            .expect("countdown control")
            .rect;
        let card = draw_ui::control(&app.tree, app.slots[0].card)
            .expect("card control")
            .rect;

        assert!(
            line.top() >= card.bottom(),
            "the row follows the cards: {line:?} vs {card:?}"
        );
        assert!(
            window.contains(Vec2::new(line.left(), line.top()))
                && window.contains(Vec2::new(line.left(), line.bottom() - 0.5)),
            "the row must stay inside the panel: {line:?}"
        );
        assert_eq!(app.countdown_text(), Some("自动刷新 05:00"));
    }

    #[test]
    fn the_layout_fits_the_viewport() {
        let app = laid_out(520.0, 460.0);
        let button = app.button_rect().expect("button rect");
        assert!(
            button.left() >= 0.0 && button.right() <= 520.0,
            "{button:?}"
        );

        // The page column must actually arrange its children: the cards sit
        // below the header, not on top of it (the flex-vs-anchor regression).
        let first = draw_ui::control(&app.tree, app.slots[0].card)
            .expect("card control")
            .rect;
        assert!(
            first.top() >= button.bottom(),
            "card {first:?} overlaps the header button {button:?}"
        );

        for (index, slot) in app.slots.iter().enumerate() {
            let card = draw_ui::control(&app.tree, slot.card)
                .expect("card control")
                .rect;
            assert!(
                card.left() >= -0.5 && card.right() <= 520.5,
                "card {index} escapes the viewport: {card:?}"
            );
            assert!(
                card.size.height < 460.0,
                "card {index} fills the window instead of its content: {card:?}"
            );
        }
    }

    #[test]
    fn a_narrow_viewport_keeps_the_cards_inside() {
        let app = laid_out(360.0, 420.0);
        for slot in &app.slots {
            let card = draw_ui::control(&app.tree, slot.card)
                .expect("card control")
                .rect;
            assert!(card.right() <= 360.5, "{card:?}");
        }
    }

    #[test]
    fn the_pointer_shows_a_pointer_cursor_over_the_button() {
        let mut app = laid_out(520.0, 460.0);
        let center = app.button_center().expect("button rect");
        app.event(&InputEvent::PointerMove { position: center });
        assert_eq!(app.cursor(), Cursor::Pointer);
    }

    /// The panel's shape lives in the first draw command: a rounded fill the
    /// size of the window. Drop the radius and the transparent window shows
    /// square corners, which no headless test would otherwise notice.
    #[test]
    fn the_backdrop_is_a_rounded_fill_the_size_of_the_panel() {
        for (width, height) in [(300.0, 400.0), (360.0, 420.0)] {
            let app = laid_out(width, height);
            let mut ctx = PaintContext::new();
            app.paint(&mut ctx);
            let list = ctx.into_draw_list();
            let commands = list.commands();

            let window = Rect::from_min_size(Vec2::ZERO, Size::new(width, height));
            let Some(DrawCommand::FillRoundedRect {
                rect,
                corners,
                paint,
            }) = commands.first()
            else {
                panic!(
                    "the first command should be the rounded backdrop, found {:?}",
                    commands.first()
                );
            };
            assert_eq!(*rect, window, "the backdrop covers the whole window");
            assert_eq!(corners.top_left, PANEL_RADIUS);
            assert_eq!(corners.top_right, PANEL_RADIUS);
            assert_eq!(corners.bottom_left, PANEL_RADIUS);
            assert_eq!(corners.bottom_right, PANEL_RADIUS);
            assert_eq!(
                paint.color, app.theme.palette.background,
                "the fill is the theme's backdrop token"
            );
            // Opaque: only the four corners outside the radius may be see-through.
            assert_eq!(app.theme.palette.background.a, 1.0);

            // Nothing may square those corners off again further down the list.
            assert!(
                !commands.iter().any(|command| matches!(
                    command,
                    DrawCommand::FillRect { rect, .. } | DrawCommand::ClipRect(rect) if *rect == window
                )),
                "a square fill or clip would cover the rounded corners"
            );
        }
    }

    /// A panel view laid out at its real size: body plus arrow.
    fn panel() -> BalanceApp {
        let mut app = BalanceApp::new_panel(Theme::dark(), TEST_ENDPOINT.to_string());
        let viewport = viewport(PANEL_WIDTH_TEST, PANEL_HEIGHT_TEST + ARROW_HEIGHT);
        app.update(viewport, 0.016);
        app.layout(viewport);
        app
    }

    /// The arrow is a rotated square with the body over its lower half, so what
    /// shows is a wedge pointing at the status item. Drawn wrong — not rotated,
    /// or centred on the body instead of the window — the panel would grow a
    /// pale square above its top edge on the transparent window.
    #[test]
    fn the_panel_arrow_points_up_at_the_status_item() {
        let app = panel();
        let window = Rect::from_min_size(
            Vec2::ZERO,
            Size::new(PANEL_WIDTH_TEST, PANEL_HEIGHT_TEST + ARROW_HEIGHT),
        );
        let mut ctx = PaintContext::new();
        app.paint(&mut ctx);
        let list = ctx.into_draw_list();
        let commands = list.commands();

        // Save, SetTransform, FillRect, Restore — the rotation must be scoped,
        // or every later command would come out skewed.
        let (
            Some(DrawCommand::Save),
            Some(DrawCommand::SetTransform(transform)),
            Some(DrawCommand::FillRect { rect, paint }),
            Some(DrawCommand::Restore),
        ) = (
            commands.first(),
            commands.get(1),
            commands.get(2),
            commands.get(3),
        )
        else {
            panic!("the arrow should be a scoped rotated fill: {commands:?}");
        };
        assert_eq!(paint.color, app.theme.palette.background);

        // Where the square's corners land in window space.
        let corners = [
            rect.min(),
            Vec2::new(rect.right(), rect.top()),
            rect.max(),
            Vec2::new(rect.left(), rect.bottom()),
        ]
        .map(|corner| transform.transform_point(corner));
        let tip = corners
            .iter()
            .copied()
            .min_by(|a, b| a.y.total_cmp(&b.y))
            .expect("a topmost corner");
        assert!(
            (tip.x - window.center().x).abs() < 0.01 && (tip.y - window.top()).abs() < 0.01,
            "the tip points at the item, from the window's top edge: {tip:?}"
        );

        let flank = corners
            .iter()
            .copied()
            .filter(|corner| (corner.y - (window.top() + ARROW_HEIGHT)).abs() < 0.01)
            .count();
        assert_eq!(flank, 2, "the wedge is as tall as the arrow: {corners:?}");
        let (left, right) = corners
            .iter()
            .fold((f32::MAX, f32::MIN), |(low, high), corner| {
                (low.min(corner.x), high.max(corner.x))
            });
        assert!(
            ((right - left) - 2.0 * ARROW_HEIGHT).abs() < 0.01,
            "and twice as wide as it is tall: {corners:?}"
        );

        // The body starts below the arrow and covers the square's lower half.
        let body = app.body(window);
        assert_eq!(
            body,
            Rect::from_min_size(
                Vec2::new(0.0, ARROW_HEIGHT),
                Size::new(PANEL_WIDTH_TEST, PANEL_HEIGHT_TEST)
            )
        );
        let lowest = corners
            .iter()
            .copied()
            .max_by(|a, b| a.y.total_cmp(&b.y))
            .expect("a bottom corner");
        assert!(
            lowest.y > body.top() && body.contains(Vec2::new(lowest.x, lowest.y + 0.5)),
            "the body hides the square's lower half: {lowest:?} vs {body:?}"
        );
    }

    /// The arrow must not shift the rows: as a panel, a control's offset inside
    /// the body is the one it has inside an ordinary window.
    #[test]
    fn the_arrow_does_not_move_the_rows() {
        let window = laid_out(PANEL_WIDTH_TEST, PANEL_HEIGHT_TEST);
        let panel = panel();
        let body = panel.body(Rect::from_min_size(
            Vec2::ZERO,
            Size::new(PANEL_WIDTH_TEST, PANEL_HEIGHT_TEST + ARROW_HEIGHT),
        ));

        let in_window = window.button_rect().expect("button rect");
        let in_panel = panel.button_rect().expect("button rect");
        assert_eq!(in_panel.size, in_window.size, "same button");
        assert_eq!(
            in_panel.left() - body.left(),
            in_window.left(),
            "same distance from the body's left edge"
        );
        assert_eq!(
            in_panel.top() - body.top(),
            in_window.top(),
            "same distance from the body's top edge"
        );
    }

    /// An ordinary window has no arrow, and its body is the whole viewport.
    #[test]
    fn a_window_has_no_arrow() {
        let app = laid_out(PANEL_WIDTH_TEST, PANEL_HEIGHT_TEST);
        let window =
            Rect::from_min_size(Vec2::ZERO, Size::new(PANEL_WIDTH_TEST, PANEL_HEIGHT_TEST));
        assert_eq!(app.body(window), window);

        let mut ctx = PaintContext::new();
        app.paint(&mut ctx);
        let list = ctx.into_draw_list();
        assert!(
            !list
                .commands()
                .iter()
                .any(|command| matches!(command, DrawCommand::SetTransform(transform) if !transform.is_identity())),
            "nothing in a window is rotated"
        );
    }
}
