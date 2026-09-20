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

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

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

/// State shared with the click callbacks.
///
/// Callbacks can only capture `'static` values, so they write into these cells
/// and the app reads them once per frame — the same pattern `demo_app` uses.
#[derive(Clone)]
struct Feed {
    /// A refresh was asked for and has not been handed to the host yet.
    requested: Rc<Cell<bool>>,
    /// A request is in flight (blocks a second one).
    loading: Rc<Cell<bool>>,
}

impl Feed {
    fn new() -> Self {
        Self {
            requested: Rc::new(Cell::new(false)),
            loading: Rc::new(Cell::new(false)),
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

        let mut app = Self {
            tree,
            theme,
            feed,
            arrow,
            refresh_button: refs.refresh.get().expect("refresh button mounted"),
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
            viewport: ViewportSize::new(Size::new(520.0, 460.0)),
        };

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

    /// Advances the frame (no animation yet) and enters the loading state when a
    /// refresh was requested.
    pub fn update(&mut self, viewport: ViewportSize, dt: f32) {
        let _ = dt;
        self.viewport = viewport;
        if self.feed.requested.get() && !self.feed.loading.get() {
            self.begin_refresh();
        }
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

    /// Routes an input event: `R` / `F5` request a refresh, everything else goes
    /// through the normal UI input path.
    pub fn event(&mut self, event: &InputEvent) -> EventResult {
        if let InputEvent::KeyDown { key } = event {
            if matches!(key, Key::F5 | Key::Character('r')) {
                if !self.feed.loading.get() {
                    self.feed.requested.set(true);
                }
                return EventResult::Handled;
            }
        }
        draw_ui::route_input(&mut self.tree, event)
    }

    /// Controls in the view.
    #[cfg(test)]
    pub fn control_count(&self) -> usize {
        draw_ui::control_count(&self.tree)
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

    /// Marks the view as loading and labels the button accordingly.
    fn begin_refresh(&mut self) {
        self.feed.loading.set(true);
        draw_components::set_text(&mut self.tree, self.refresh_button, REFRESH_BUSY);
        draw_components::set_text(&mut self.tree, self.status, STATUS_BUSY);
        draw_components::set_text(&mut self.tree, self.error, "");
    }

    /// Asks for a refresh from outside the view (the menu bar's timer, or the
    /// menu item). Mirrors what the button and `R` / `F5` do; nothing is sent
    /// until the next [`BalanceApp::update`], which is where the request is
    /// picked up.
    pub fn request_refresh(&mut self) {
        if !self.feed.loading.get() {
            self.feed.requested.set(true);
        }
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
        draw_components::set_text(&mut self.tree, self.refresh_button, REFRESH_LABEL);

        match result {
            Ok(balance) => self.show_balance(balance),
            Err(message) => {
                draw_components::set_text(&mut self.tree, self.status, STATUS_FAILED);
                draw_components::set_text(&mut self.tree, self.error, message);
            }
        }
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
    let loading = feed.loading.clone();

    let title = Column::new()
        .gap(space::XXS)
        .grow(1.0)
        .child(Text::title(TITLE, theme))
        .child(
            Text::caption(endpoint, theme)
                .tone(Tone::Subtle)
                .text_options(TextOptions::no_wrap())
                .max_lines(1)
                .ellipsis(true),
        );

    let button = Button::primary(REFRESH_LABEL, theme)
        .on_click(move || {
            if !loading.get() {
                requested.set(true);
            }
        })
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
    /// below start from a settled screen.
    fn settled(width: f32, height: f32) -> BalanceApp {
        let mut app = laid_out(width, height);
        assert!(app.take_refresh_request(), "the view refreshes on open");
        app.apply_result(Ok(sample()));
        app.layout(viewport(width, height));
        app
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
        let mut app = settled(520.0, 460.0);
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
        let mut app = settled(520.0, 460.0);
        app.event(&InputEvent::KeyDown {
            key: Key::Character('r'),
        });
        app.update(viewport(520.0, 460.0), 0.016);
        assert!(app.is_loading());
        assert!(app.take_refresh_request());
    }

    #[test]
    fn a_second_request_is_ignored_while_loading() {
        let mut app = settled(520.0, 460.0);
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
