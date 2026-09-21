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

use deepseek_util::time::{clock, now_epoch, timestamp};
use draw_components::{Button, Card, Column, Component, Divider, Flex, NodeRef, Ref, Row, Text};
use draw_core::{
    Color, Cursor, Edges, EventResult, InputEvent, Key, NodeId, Rect, Size, Transform2D, Vec2,
    ViewportSize,
};
use draw_render::PaintContext;
use draw_scene::{SceneChild, SceneTree};
use draw_theme::{radius, space, SurfaceLevel, Theme, Tone};
use draw_ui::{fill_rounded_rect, Align, Justify, MouseFilter, SurfaceStyle, TextMeasurer};

use crate::api::Balance;
use crate::go::{GoUsage, WindowKind};

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
/// The DeepSeek tab's label (also the window's heading).
const TITLE: &str = "DeepSeek 余额";
/// The OpenCode Go tab's label.
const GO_TAB_LABEL: &str = "OpenCode Go";
/// Footer hint on the Go page: where its key comes from.
const GO_HINT: &str = "密钥来自 ~/.local/share/opencode/auth.json 或 OPENCODE_GO_API_KEY";
/// How many sources one refresh queries; the button stays busy until all answer.
const SOURCES_PER_REFRESH: u32 = 2;
/// Idle and busy refresh-button labels.
const REFRESH_LABEL: &str = "刷新 (R)";
const REFRESH_BUSY: &str = "刷新中…";
/// Status-line texts.
const STATUS_IDLE: &str = "尚未刷新；点“刷新”或按 R";
const STATUS_BUSY: &str = "刷新中…";
const STATUS_FAILED: &str = "刷新失败";
const GO_STATUS_FAILED: &str = "刷新失败";
/// Footer hint: the two environment overrides.
const FOOTER_HINT: &str = "未配置 DEEPSEEK_API_KEY 时会提示；每次刷新重新读取环境变量";
/// Prefix of the countdown line, e.g. `自动刷新 04:32` (see
/// [`BalanceApp::set_countdown`]).
const COUNTDOWN_PREFIX: &str = "自动刷新";
/// The countdown line while the automatic timer is held by the menu's
/// `暂停自动刷新` (see [`BalanceApp::set_countdown_paused`]).
const COUNTDOWN_PAUSED: &str = "自动刷新已暂停";

/// The error the debug button stages, long enough to wrap and exercise the
/// error line's `max_lines(2)` layout.
const TEST_ERROR: &str = "请求失败：HTTP 500 内部错误，端点暂时不可用，请稍后重试（测试文案）";
/// Label of the debug button that stages / clears the test error.
const TEST_ERROR_BUTTON_LABEL: &str = "测试错误";

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

/// Which tab page is on screen: the DeepSeek balance, or the OpenCode Go quota.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    DeepSeek,
    Go,
}

impl Tab {
    /// The text on the tab.
    pub fn label(self) -> &'static str {
        match self {
            Self::DeepSeek => TITLE,
            Self::Go => GO_TAB_LABEL,
        }
    }

    /// Stable index, for the settings file.
    pub fn index(self) -> u8 {
        match self {
            Self::DeepSeek => 0,
            Self::Go => 1,
        }
    }

    /// [`Tab::index`] in reverse; `None` for an unknown value.
    pub fn from_index(index: u8) -> Option<Self> {
        match index {
            0 => Some(Self::DeepSeek),
            1 => Some(Self::Go),
            _ => None,
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
    /// How many fetches of the current refresh are still outstanding. One
    /// refresh queries every source, so this starts at
    /// [`SOURCES_PER_REFRESH`] and runs down as replies land.
    inflight: Rc<Cell<u32>>,
    /// What the button currently shows.
    state: Rc<Cell<RefreshState>>,
    /// A pending test error the debug button wants on screen, or `None` to clear
    /// it. Read by [`BalanceApp::update`] and reset, like the refresh intent.
    test_error: Rc<Cell<Option<&'static str>>>,
    /// The tab currently selected; read by the tab buttons' `dynamic_background`.
    tab: Rc<Cell<Tab>>,
    /// A tab the user clicked, drained by [`BalanceApp::update`] (the click
    /// callback cannot borrow the app).
    tab_request: Rc<Cell<Option<Tab>>>,
}

impl Feed {
    fn new() -> Self {
        Self {
            requested: Rc::new(Cell::new(false)),
            inflight: Rc::new(Cell::new(0)),
            state: Rc::new(Cell::new(RefreshState::Idle)),
            test_error: Rc::new(Cell::new(None)),
            tab: Rc::new(Cell::new(Tab::DeepSeek)),
            tab_request: Rc::new(Cell::new(None)),
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
    /// The DeepSeek page's refresh button (its decor hangs off this id).
    refresh_button: NodeId,
    /// The label nodes of every refresh button, one per page.
    ///
    /// `draw_ui::Widget::set_text` writes `Label` and raw `Button` widgets only,
    /// and the themed `Button` is a flex row *wrapping* a label — so the text has
    /// to be written one level down. Found once at mount ([`label_of`]) instead of
    /// every frame.
    refresh_labels: Vec<NodeId>,
    status: NodeId,
    error: NodeId,
    /// The rule between the error line and the cards, hidden when the error is.
    divider: NodeId,
    countdown: NodeId,
    /// The countdown line currently on screen, so a repeat of the same second
    /// costs nothing. `None` means the line is hidden.
    countdown_line: Option<String>,
    available_ok: NodeId,
    available_bad: NodeId,
    slots: Vec<CurrencySlot>,
    /// Last successful reply, kept so the tests (and the host) can read it back.
    last_balance: Option<Balance>,
    /// The two pages, only one of which is visible at a time.
    deepseek_page: NodeId,
    go_page: NodeId,
    /// The tab buttons. Only the tests need their geometry (a click is routed
    /// through the tree, not by id), so they are kept on the test build only.
    #[cfg(test)]
    deepseek_tab: NodeId,
    #[cfg(test)]
    go_tab: NodeId,
    /// The Go page's own status / error / window rows.
    go_status: NodeId,
    go_error: NodeId,
    go_error_divider: NodeId,
    /// One value cell per [`WindowKind`], in [`WindowKind::ALL`] order.
    go_values: Vec<NodeId>,
    /// Last successful Go reply, so the host (and tests) can read it back.
    last_go: Option<GoUsage>,
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
        // root's single child (the same shape `demo_app` uses). The tab bar sits
        // above two pages, only one of which is visible at a time.
        let content = Flex::column()
            .mouse_filter(MouseFilter::Ignore)
            .gap(space::LG)
            .padding(Edges::new(space::XXL, top, space::XXL, space::XXL))
            .child(tab_bar(theme, &refs, &feed))
            .child(deepseek_page(theme, &endpoint, &refs, &feed).ref_(&refs.deepseek_page))
            .child(go_page(theme, &refs, &feed).ref_(&refs.go_page));

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
        let go_refresh_button = refs.go_refresh.get().expect("go refresh button mounted");
        let refresh_labels: Vec<NodeId> = [refresh_button, go_refresh_button]
            .into_iter()
            .map(|button| label_of(&tree, button).expect("the refresh button carries a label"))
            .collect();
        #[cfg(test)]
        let deepseek_tab = refs.tab_deepseek.get().expect("deepseek tab mounted");
        #[cfg(test)]
        let go_tab = refs.tab_go.get().expect("go tab mounted");

        let mut app = Self {
            tree,
            theme,
            feed,
            arrow,
            refresh_button,
            refresh_labels,
            status: refs.status.get().expect("status label mounted"),
            error: refs.error.get().expect("error label mounted"),
            divider: refs.divider.get().expect("divider mounted"),
            countdown: refs.countdown.get().expect("countdown label mounted"),
            countdown_line: None,
            available_ok: refs.available_ok.get().expect("availability label mounted"),
            available_bad: refs
                .available_bad
                .get()
                .expect("availability label mounted"),
            slots,
            last_balance: None,
            deepseek_page: refs.deepseek_page.get().expect("deepseek page mounted"),
            go_page: refs.go_page.get().expect("go page mounted"),
            #[cfg(test)]
            deepseek_tab,
            #[cfg(test)]
            go_tab,
            go_status: refs.go_status.get().expect("go status mounted"),
            go_error: refs.go_error.get().expect("go error mounted"),
            go_error_divider: refs
                .go_error_divider
                .get()
                .expect("go error divider mounted"),
            go_values: refs
                .go_values
                .iter()
                .map(|value| value.get().expect("go value mounted"))
                .collect(),
            last_go: None,
            min_gap: DEFAULT_MIN_REFRESH_GAP,
            blocked_until: None,
            #[cfg(test)]
            clock_shift: Duration::ZERO,
            viewport: ViewportSize::new(Size::new(520.0, 460.0)),
        };

        // Each button dims itself while it cannot be pressed. Chrome attached
        // after mount rather than a builder method because the themed `Button`
        // resolves its own surface, and because the closure needs the prepared
        // node's id. `paint_front` runs before the label child is painted, so the
        // fill is dimmed and the text stays at full contrast.
        let state = app.feed.state.clone();
        for button in [app.refresh_button, go_refresh_button] {
            dim_while_busy(&mut app.tree, button, state.clone());
        }

        // Before the first fetch: the availability marks are hidden and only the
        // first currency card is on screen, filled with placeholders.
        let (ok, bad) = (app.available_ok, app.available_bad);
        app.set_visible(ok, false);
        app.set_visible(bad, false);
        // The countdown line waits for the host to report a timer.
        let countdown = app.countdown;
        app.set_visible(countdown, false);
        // The error line starts empty, so its divider is hidden too.
        app.set_error("");
        app.set_go_error("");
        let hidden: Vec<NodeId> = app.slots[1..].iter().map(|slot| slot.card).collect();
        for card in hidden {
            app.set_visible(card, false);
        }
        // The Go tab starts hidden; the DeepSeek page is the opening view.
        app.select_tab(Tab::DeepSeek);

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

    /// Whether a request is in flight. The host asks before the badge exists
    /// (menu-bar mode opens the view before the second window), so it starts the
    /// badge on the state the view is already in.
    pub fn is_loading(&self) -> bool {
        self.feed.inflight.get() > 0
    }

    /// The last successful DeepSeek reply, if any.
    ///
    /// The host mirrors this into the menu-bar title, so it stays readable
    /// after the panel is closed.
    pub fn last_balance(&self) -> Option<&Balance> {
        self.last_balance.as_ref()
    }

    /// The last successful OpenCode Go reply, if any.
    pub fn last_go(&self) -> Option<&GoUsage> {
        self.last_go.as_ref()
    }

    /// Which tab is selected. The host mirrors it into the menu bar and the
    /// badge, and persists it between runs.
    pub fn tab(&self) -> Tab {
        self.feed.tab.get()
    }

    /// The value cell for `kind` on the Go page (`剩余 88% · 1:23 后重置`).
    #[cfg(test)]
    pub fn go_value_text(&self, kind: WindowKind) -> Option<&str> {
        let index = WindowKind::ALL
            .iter()
            .position(|candidate| *candidate == kind)?;
        self.go_values
            .get(index)
            .and_then(|id| text(&self.tree, *id))
    }

    /// Whether the page for `tab` is on screen (the other is hidden).
    #[cfg(test)]
    pub fn page_visible(&self, tab: Tab) -> Option<bool> {
        let page = match tab {
            Tab::DeepSeek => self.deepseek_page,
            Tab::Go => self.go_page,
        };
        self.tree.is_visible(page)
    }

    /// Center of a tab in logical viewport coordinates.
    #[cfg(test)]
    pub fn tab_center(&self, tab: Tab) -> Option<Vec2> {
        let id = match tab {
            Tab::DeepSeek => self.deepseek_tab,
            Tab::Go => self.go_tab,
        };
        draw_ui::control(&self.tree, id).map(|control| control.rect.center())
    }

    /// The Go status line's text.
    pub fn go_status_text(&self) -> Option<&str> {
        text(&self.tree, self.go_status)
    }

    /// The Go error line's text. The host reads it to decide whether the badge
    /// shows a failure.
    pub fn go_error_text(&self) -> Option<&str> {
        text(&self.tree, self.go_error)
    }

    /// One line describing where the active tab stands: the last error if there
    /// is one, otherwise its status line. The host uses it as the tooltip.
    pub fn summary(&self) -> String {
        let (error, status) = match self.tab() {
            Tab::DeepSeek => (self.error_text(), self.status_text()),
            Tab::Go => (self.go_error_text(), self.go_status_text()),
        };
        if let Some(error) = error.filter(|error| !error.is_empty()) {
            return error.to_string();
        }
        status.unwrap_or(STATUS_IDLE).to_string()
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
        self.refresh_labels
            .first()
            .and_then(|node| text(&self.tree, *node))
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

        // A tab click only queued the choice; apply it here, where `&mut self`
        // is available to move the pages.
        if let Some(tab) = self.feed.tab_request.replace(None) {
            self.select_tab(tab);
        }

        // A request that the throttle refuses is dropped here rather than left
        // for the host: `take_refresh_request` is what starts the worker thread,
        // so clearing the flag is what keeps a refused click off the network.
        if self.feed.requested.get() && !self.can_refresh(now) {
            self.feed.requested.set(false);
        }
        if self.feed.requested.get() {
            self.begin_refresh(now);
        }
        // The debug button stages a test error (or clears it); consume it here
        // like the refresh intent so the view stays the single writer of the
        // error line.
        if let Some(message) = self.feed.test_error.replace(None) {
            self.set_error(message);
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
        self.feed.inflight.get() == 0 && self.blocked_for(now).is_none()
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
        // One refresh queries every source; the button stays busy until the last
        // reply lands.
        self.feed.inflight.set(SOURCES_PER_REFRESH);
        self.blocked_until = Some(now + self.min_gap);
        draw_components::set_text(&mut self.tree, self.status, STATUS_BUSY);
        self.set_error("");
        draw_components::set_text(&mut self.tree, self.go_status, STATUS_BUSY);
        self.set_go_error("");
    }

    /// Writes the button's state into its label and into the cell its decor
    /// reads. Returns whether the label moved.
    fn sync_button(&mut self, now: Instant) -> bool {
        let state = if self.feed.inflight.get() > 0 {
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
        let label = state.label();
        for node in &self.refresh_labels {
            draw_components::set_text(&mut self.tree, *node, label.clone());
        }
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
        self.set_countdown_line(line)
    }

    /// Shows the line the timer is held on, in place of the countdown.
    ///
    /// The host calls this while the menu's `暂停自动刷新` holds: the line stays
    /// visible so a paused panel reads as paused rather than as one with no
    /// timer. Same change contract as [`BalanceApp::set_countdown`].
    pub fn set_countdown_paused(&mut self) -> bool {
        self.set_countdown_line(Some(COUNTDOWN_PAUSED.to_string()))
    }

    /// Writes `line` (or hides the label for `None`) and reports whether it moved.
    fn set_countdown_line(&mut self, line: Option<String>) -> bool {
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
        self.note_reply();
        self.feed.requested.set(false);

        match result {
            Ok(balance) => self.show_balance(balance),
            Err(message) => {
                // A failed attempt left nothing fresh on screen, so it does not
                // hold the throttle: the button is pressable again at once, and
                // that press is the retry. There is still only one request in
                // flight at a time, which is what the `inflight` gate is for.
                self.blocked_until = None;
                draw_components::set_text(&mut self.tree, self.status, STATUS_FAILED);
                self.set_error(&message);
            }
        }

        self.sync_button(self.now());
    }

    /// Applies a finished OpenCode Go request to the Go tab.
    ///
    /// Mirrors [`BalanceApp::apply_result`], including the "a failure lifts the
    /// throttle" rule: a failed Go fetch is not a reason to lock the button.
    pub fn apply_go_result(&mut self, result: Result<GoUsage, String>) {
        self.note_reply();

        match result {
            Ok(usage) => self.show_go_usage(usage),
            Err(message) => {
                self.blocked_until = None;
                draw_components::set_text(&mut self.tree, self.go_status, GO_STATUS_FAILED);
                self.set_go_error(&message);
            }
        }

        self.sync_button(self.now());
    }

    /// Marks one outstanding fetch as answered. The button leaves
    /// [`RefreshState::Busy`] only when the last source has replied.
    fn note_reply(&mut self) {
        let left = self.feed.inflight.get().saturating_sub(1);
        self.feed.inflight.set(left);
    }

    /// Switches pages: the clicked tab becomes the visible one. The hidden page
    /// leaves the layout entirely, so the open one gets its full height.
    pub fn select_tab(&mut self, tab: Tab) {
        self.feed.tab.set(tab);
        let (deepseek, go) = (self.deepseek_page, self.go_page);
        let deepseek_visible = tab == Tab::DeepSeek;
        self.set_visible(deepseek, deepseek_visible);
        self.set_visible(go, !deepseek_visible);
    }

    /// Writes each Go window row: remainder first, then the time to reset.
    fn show_go_usage(&mut self, usage: GoUsage) {
        draw_components::set_text(
            &mut self.tree,
            self.go_status,
            format!("更新于 {}", timestamp()),
        );
        self.set_go_error("");

        let now = now_epoch();
        let rows: Vec<(NodeId, String)> = WindowKind::ALL
            .iter()
            .enumerate()
            .map(|(index, kind)| {
                let value = match kind.of(&usage.usage) {
                    Some(window) => {
                        let mut value = window.remaining_label();
                        if let Some(left) = window.reset_in(now) {
                            value.push_str(&format!(" · {} 后重置", clock(left)));
                        }
                        value
                    }
                    None => PLACEHOLDER.to_string(),
                };
                (self.go_values[index], value)
            })
            .collect();
        for (node, value) in rows {
            draw_components::set_text(&mut self.tree, node, value);
        }

        self.last_go = Some(usage);
    }

    /// Writes `balance` into the labels and shows exactly the cards it has data
    /// for.
    fn show_balance(&mut self, balance: Balance) {
        let stamp = format!(
            "更新于 {} · {} 种币种",
            timestamp(),
            balance.balance_infos.len()
        );
        draw_components::set_text(&mut self.tree, self.status, stamp);
        self.set_error("");

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

    /// Writes the error line and keeps it (and its divider) collapsible: an
    /// empty message hides both so they take no height, a non-empty one shows
    /// them and puts the text in place.
    fn set_error(&mut self, message: &str) {
        let visible = !message.is_empty();
        self.set_visible(self.error, visible);
        self.set_visible(self.divider, visible);
        draw_components::set_text(&mut self.tree, self.error, message);
    }

    /// The Go page's error line, with the same collapse rule as [`set_error`].
    fn set_go_error(&mut self, message: &str) {
        let visible = !message.is_empty();
        self.set_visible(self.go_error, visible);
        self.set_visible(self.go_error_divider, visible);
        draw_components::set_text(&mut self.tree, self.go_error, message);
    }
}

/// Node slots the declarative builders write into.
#[derive(Clone, Default)]
struct Refs {
    refresh: NodeRef,
    status: NodeRef,
    error: NodeRef,
    divider: NodeRef,
    countdown: NodeRef,
    test_error: NodeRef,
    available_ok: NodeRef,
    available_bad: NodeRef,
    slots: Vec<SlotRefs>,
    tab_deepseek: NodeRef,
    tab_go: NodeRef,
    deepseek_page: NodeRef,
    go_page: NodeRef,
    go_refresh: NodeRef,
    go_status: NodeRef,
    go_error: NodeRef,
    go_error_divider: NodeRef,
    go_values: Vec<NodeRef>,
}

/// `Refs::default()` cannot size the slot vectors, so they are patched here.
impl Refs {
    fn with_slots() -> Self {
        Self {
            slots: (0..MAX_CURRENCIES).map(|_| SlotRefs::default()).collect(),
            go_values: (0..WindowKind::ALL.len())
                .map(|_| NodeRef::default())
                .collect(),
            ..Self::default()
        }
    }
}

mod build;
#[cfg(test)]
mod tests;

use build::*;
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
