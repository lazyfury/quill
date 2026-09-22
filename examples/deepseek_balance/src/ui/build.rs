//! Page and component builders for the balance view.
//!
//! Split out of `mod.rs`: the view's state machine and the tree it builds are
//! read separately.

use draw_core::FontWeight;

use super::*;
use crate::go;

/// The tab bar: the two page tabs, always visible.
pub(super) fn tab_bar(theme: &'static dyn Theme, refs: &Refs, feed: &Feed) -> Row {
    Row::new()
        .align(Align::Center)
        .gap(space::XS)
        .child(tab_button(theme, Tab::DeepSeek, feed).ref_(&refs.tab_deepseek))
        .child(tab_button(theme, Tab::Go, feed).ref_(&refs.tab_go))
}

/// One tab: a label whose background marks the active page.
///
/// The highlight is resolved each frame from the shared tab cell, so switching
/// pages never rebuilds the tree; the click only queues the choice for
/// [`BalanceApp::update`] to drain.
pub(super) fn tab_button(theme: &'static dyn Theme, tab: Tab, feed: &Feed) -> Flex {
    let clicked = feed.tab_request.clone();
    let active = feed.tab.clone();
    Flex::row()
        .gap(0.0)
        .padding(Edges::new(space::SM, space::XS, space::SM, space::XS))
        .on_click(move || clicked.set(Some(tab)))
        .dynamic_background(move |_| {
            if active.get() == tab {
                SurfaceStyle::new(theme.surface(SurfaceLevel::Raised))
            } else {
                SurfaceStyle::new(Color::TRANSPARENT)
            }
        })
        .child(Text::small(tab.label(), theme).size(draw_theme::TextSize::Caption).weight(FontWeight::new(600)))
}

/// The DeepSeek page: endpoint header, status, error, currency cards, footer.
///
/// Zero padding: the tabbed container already insets everything, and `Flex`'s
/// own default (16px) would double it.
pub(super) fn deepseek_page(
    theme: &'static dyn Theme,
    endpoint: &str,
    refs: &Refs,
    feed: &Feed,
) -> Flex {
    Flex::column()
        .mouse_filter(MouseFilter::Ignore)
        .gap(space::LG)
        .padding(Edges::ZERO)
        .grow(1.0)
        .child(endpoint_line(theme, endpoint))
        .child(status_row(theme, refs))
        .child(
            Text::small("", theme)
                .color(theme.palette().error)
                .max_lines(2)
                .wrap(true)
                .ellipsis(true)
                .ref_(&refs.error),
        )
        .child(Divider::horizontal(theme).ref_(&refs.divider))
        .child(currencies(theme, refs))
        .child(footer(theme, refs, feed))
}

/// The OpenCode Go page: endpoint header, status, error, the three quota rows.
pub(super) fn go_page(theme: &'static dyn Theme, refs: &Refs, feed: &Feed) -> Flex {
    let rows = WindowKind::ALL
        .iter()
        .enumerate()
        .map(|(index, kind)| go_window_row(theme, *kind, &refs.go_values[index]));

    Flex::column()
        .mouse_filter(MouseFilter::Ignore)
        .gap(space::LG)
        .padding(Edges::ZERO)
        .grow(1.0)
        .child(endpoint_line(theme, &go::endpoint()))
        .child(
            Text::caption("", theme)
                .tone(Tone::Muted)
                .ref_(&refs.go_status),
        )
        .child(
            Text::small("", theme)
                .color(theme.palette().error)
                .max_lines(2)
                .wrap(true)
                .ellipsis(true)
                .ref_(&refs.go_error),
        )
        .child(Divider::horizontal(theme).ref_(&refs.go_error_divider))
        .child(
            Card::new(theme)
                .gap(space::SM)
                .child(
                    Row::new()
                        .align(Align::Center)
                        .justify(Justify::SpaceBetween)
                        .child(Text::subheading(GO_TAB_LABEL, theme))
                        .child(Text::caption("订阅窗口", theme).tone(Tone::Muted)),
                )
                .child(Divider::horizontal(theme))
                .children(rows),
        )
        .child(go_footer(theme, refs, feed))
}

/// The Go page's footer: the key hint, then the refresh button.
pub(super) fn go_footer(theme: &'static dyn Theme, refs: &Refs, feed: &Feed) -> Column {
    Column::new()
        .gap(space::SM)
        .child(Text::caption(GO_HINT, theme).tone(Tone::Subtle))
        .child(refresh_button(theme, &refs.go_refresh, feed))
}

/// A "window … value" row inside the Go card.
pub(super) fn go_window_row(theme: &'static dyn Theme, kind: WindowKind, value: &NodeRef) -> Row {
    Row::new()
        .align(Align::Center)
        .justify(Justify::SpaceBetween)
        .gap(space::SM)
        .child(Text::small(kind.label(), theme).tone(Tone::Muted))
        .child(Text::small(PLACEHOLDER, theme).ref_(value))
}

/// The primary refresh action, shared by both pages' footers.
///
/// A press is an intent, not a command: whether it goes out is decided in
/// `update`, which owns the clock and therefore the throttle. Both pages build
/// one and share [`Feed`], so their labels and dimming agree.
pub(super) fn refresh_button(
    theme: &'static dyn Theme,
    slot: &NodeRef,
    feed: &Feed,
) -> Ref<Button> {
    let requested = feed.requested.clone();
    Button::primary(REFRESH_LABEL, theme)
        .on_click(move || requested.set(true))
        .ref_(slot)
}

/// Attaches the dimming film a refresh button wears while it cannot be pressed.
///
/// `paint_front` runs before the label child is painted, so the fill is dimmed
/// and the text stays at full contrast.
pub(super) fn dim_while_busy(tree: &mut SceneTree, button: NodeId, state: Rc<Cell<RefreshState>>) {
    draw_ui::add_decor(
        tree,
        button,
        draw_ui::foreground_decor(move |ctx, rect, _state| {
            if state.get().is_idle() {
                return;
            }
            fill_rounded_rect(
                ctx,
                rect,
                radius::MD,
                Color::BLACK.with_alpha(DISABLED_WASH_ALPHA),
            );
        }),
    );
}

/// The page's endpoint line.
///
/// The page title lives on the tab above it, so this is only the endpoint the
/// environment can override.
pub(super) fn endpoint_line(theme: &'static dyn Theme, endpoint: &str) -> Row {
    Row::new().align(Align::Center).child(
        Text::caption(endpoint_label(endpoint), theme)
            .tone(Tone::Subtle)
            .max_lines(1)
            .wrap(true)
            .ellipsis(true),
    )
}

/// The endpoint's display label: scheme and path dropped.
///
/// The full URL is long and mostly noise in a 300 px panel, where a
/// non-wrapping label reports its whole run as its minimum; only the host is
/// shown. The override variable and the full URL stay in the help text.
pub(super) fn endpoint_label(endpoint: &str) -> String {
    let authority = endpoint.split("://").last().unwrap_or(endpoint);
    authority.split('/').next().unwrap_or(authority).to_string()
}

/// The status line: availability mark plus "refreshed at" text.
pub(super) fn status_row(theme: &'static dyn Theme, refs: &Refs) -> Row {
    Row::new()
        .align(Align::Center)
        .gap(space::MD)
        .child(
            Text::small("账户可用", theme)
                .color(theme.palette().success)
                .ref_(&refs.available_ok),
        )
        .child(
            Text::small("账户不可用", theme)
                .color(theme.palette().error)
                .ref_(&refs.available_bad),
        )
        .child(
            Text::caption(STATUS_IDLE, theme)
                .tone(Tone::Muted)
                .ref_(&refs.status),
        )
}

/// The card column: one card per currency, filling the remaining height.
pub(super) fn currencies(theme: &'static dyn Theme, refs: &Refs) -> Column {
    let cards = refs.slots.iter().map(|slot| currency_card(theme, slot));

    Column::new()
        .gap(space::MD)
        .grow(1.0)
        .mouse_filter(MouseFilter::Ignore)
        .children(cards)
}

/// The DeepSeek page's footer: the hint, then a row with the countdown and the
/// debug button, then the refresh button.
///
/// The countdown and the debug button share a row so the footer still fits a
/// short panel once the button moves here. The countdown node is hidden unless
/// the host starts a timer, which leaves the row to the debug button.
pub(super) fn footer(theme: &'static dyn Theme, refs: &Refs, feed: &Feed) -> Column {
    let test_error = feed.test_error.clone();

    Column::new()
        .gap(space::SM)
        .child(Text::caption(FOOTER_HINT, theme).tone(Tone::Subtle))
        .child(
            Row::new()
                .align(Align::Center)
                .justify(Justify::SpaceBetween)
                .gap(space::SM)
                .child(
                    Text::caption("", theme)
                        .tone(Tone::Subtle)
                        .ref_(&refs.countdown),
                )
                .child(
                    // Debug aid: stages the test error (or clears it) so the error
                    // line's layout can be eyeballed without a real failure.
                    Button::secondary(TEST_ERROR_BUTTON_LABEL, theme)
                        .on_click(move || {
                            let next = match test_error.get() {
                                Some(_) => None,
                                None => Some(TEST_ERROR),
                            };
                            test_error.set(next);
                        })
                        .ref_(&refs.test_error),
                ),
        )
        .child(refresh_button(theme, &refs.refresh, feed))
}

/// One currency card: total on top, then the granted / topped-up breakdown.
pub(super) fn currency_card(theme: &'static dyn Theme, refs: &SlotRefs) -> Ref<Card> {
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
pub(super) fn stat_row(theme: &'static dyn Theme, label: &str, value: &NodeRef) -> Row {
    Row::new()
        .align(Align::Center)
        .justify(Justify::SpaceBetween)
        .gap(space::SM)
        .child(Text::small(label, theme).tone(Tone::Muted))
        .child(Text::small(PLACEHOLDER, theme).ref_(value))
}
