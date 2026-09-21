use super::build::*;
use super::*;
use crate::api::BalanceInfo;
use crate::go;
use draw_core::PointerButton;
use draw_render::DrawCommand;

/// Endpoint used by the headless tests (never fetched).
const TEST_ENDPOINT: &str = "https://api.deepseek.com/user/balance";

/// The body size the host asks for when it wants the panel (mirrors
/// `host::PANEL_WIDTH` / `host::PANEL_HEIGHT`).
const PANEL_WIDTH_TEST: f32 = 300.0;
const PANEL_HEIGHT_TEST: f32 = 470.0;

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

/// A canned OpenCode Go reply: all three windows healthy, no reset stamps so
/// the value cells stay stable for assertions.
fn sample_go() -> GoUsage {
    let window = |percent| go::UsageWindow {
        status: "ok".to_string(),
        percent,
        resets_at: None,
    };
    GoUsage {
        usage: go::Usage {
            rolling: Some(window(12.0)),
            weekly: Some(window(40.0)),
            monthly: Some(window(100.0)),
        },
    }
}

/// A view laid out at `width` x `height`, with the on-open refresh still
/// pending (the state the first frame sees).
fn laid_out(width: f32, height: f32) -> BalanceApp {
    let mut app = BalanceApp::new(default_theme(Mode::Dark), TEST_ENDPOINT.to_string());
    let viewport = viewport(width, height);
    app.update(viewport, 0.016);
    app.layout(viewport);
    app
}

/// A view whose on-open refresh has already been answered, so the tests
/// below start from a settled screen. Both sources reply, because one
/// refresh queries both; the throttle is still armed, so this is what the
/// panel looks like a second after it opens.
fn settled(width: f32, height: f32) -> BalanceApp {
    let mut app = laid_out(width, height);
    assert!(app.take_refresh_request(), "the view refreshes on open");
    app.apply_result(Ok(sample()));
    app.apply_go_result(Ok(sample_go()));
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
    let app = BalanceApp::new(default_theme(Mode::Dark), TEST_ENDPOINT.to_string());
    assert_eq!(app.total_text(0), Some(PLACEHOLDER));
    assert_eq!(app.card_visible(0), Some(true));
    assert_eq!(app.card_visible(1), Some(false));
    assert_eq!(app.status_text(), Some(STATUS_IDLE));
    assert_eq!(app.error_text(), Some(""));
    assert!(!app.is_loading());
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
fn the_open_refresh_goes_out_and_arms_the_throttle() {
    let mut app = laid_out(520.0, 460.0);
    assert!(app.is_loading(), "the first frame starts a refresh");
    assert_eq!(app.status_text(), Some(STATUS_BUSY));
    assert_eq!(app.refresh_label(), Some(REFRESH_BUSY));
    assert!(app.take_refresh_request(), "and hands it to the host");
    assert!(!app.take_refresh_request(), "only once");
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
    let mut app = BalanceApp::new(default_theme(Mode::Dark), TEST_ENDPOINT.to_string());
    app.set_min_refresh_gap(Duration::ZERO);
    let viewport = viewport(520.0, 460.0);
    app.update(viewport, 0.016);
    assert!(
        app.take_refresh_request(),
        "the open refresh still goes out"
    );
    app.apply_result(Ok(sample()));
    app.apply_go_result(Ok(sample_go()));
    app.layout(viewport);
    assert_eq!(app.cooldown_left(), None, "nothing to wait for");

    let center = app.button_center().expect("button rect");
    click(&mut app, center);
    app.update(viewport, 0.016);
    assert!(app.is_loading(), "the press goes straight out");
    assert!(app.take_refresh_request());
    assert_eq!(app.refresh_label(), Some(REFRESH_BUSY));
}

/// The wash is what makes "cannot press" visible: a film over the button —
/// and only over it, drawn before the label so the text keeps contrast — gone
/// again once the wait is over.
#[test]
fn the_button_is_washed_only_while_it_cannot_be_pressed() {
    let mut app = settled(520.0, 460.0);
    let button = app.button_rect().expect("button rect");
    assert!(app.is_throttled());
    assert!(washed(&app, button), "the cooling button is dimmed");

    let mut ctx = PaintContext::new();
    app.paint(&mut ctx);
    let list = ctx.into_draw_list();
    let washes = list
        .commands()
        .iter()
        .filter(|command| {
            matches!(
                command,
                DrawCommand::FillRoundedRect { rect, paint, .. }
                    if *rect == button
                        && paint.color == Color::BLACK.with_alpha(DISABLED_WASH_ALPHA)
            )
        })
        .count();
    assert_eq!(washes, 1, "one film, and only over the button");

    // And undimmed again once the wait is over.
    app.clock_shift = DEFAULT_MIN_REFRESH_GAP;
    app.update(viewport(520.0, 460.0), 0.016);
    app.layout(viewport(520.0, 460.0));
    let button = app.button_rect().expect("button rect");
    assert!(!app.is_throttled());
    assert!(!washed(&app, button), "a ready button is left alone");
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

    // A one-currency reply hides the spare card.
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

/// The line follows the host's timer: hidden until there is an interval (a
/// window run has no timer), shown with the countdown, and hidden again when
/// the timer stops.
#[test]
fn the_countdown_line_follows_the_host_timer() {
    let mut app = settled(520.0, 460.0);
    assert_eq!(app.countdown_visible(), Some(false), "no timer yet");

    assert!(app.set_countdown(Some(Duration::from_secs(300))));
    assert_eq!(app.countdown_visible(), Some(true));
    assert_eq!(app.countdown_text(), Some("自动刷新 05:00"));

    assert!(app.set_countdown(None));
    assert_eq!(app.countdown_visible(), Some(false));
    assert!(!app.set_countdown(None), "and stays put");
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

/// Pausing replaces the countdown with a line that says so, and it is a
/// one-shot move: the host calls this every batch, so a repeat must not ask
/// for another frame.
#[test]
fn a_paused_timer_says_so_instead_of_counting_down() {
    let mut app = settled(520.0, 460.0);
    app.set_countdown(Some(Duration::from_secs(300)));

    assert!(app.set_countdown_paused());
    assert_eq!(app.countdown_visible(), Some(true));
    assert_eq!(app.countdown_text(), Some("自动刷新已暂停"));
    assert!(!app.set_countdown_paused(), "and stays put");

    // Resuming goes back to a countdown.
    assert!(app.set_countdown(Some(Duration::from_secs(300))));
    assert_eq!(app.countdown_text(), Some("自动刷新 05:00"));
}

// -- tabs and the OpenCode Go page -------------------------------------

/// A Go reply fills each window row with the *remaining* percent; a spent
/// window reads `已用尽`.
#[test]
fn applying_a_go_reply_fills_the_window_rows() {
    let app = settled(520.0, 460.0);
    assert_eq!(app.go_value_text(WindowKind::Rolling), Some("剩余 88%"));
    assert_eq!(app.go_value_text(WindowKind::Weekly), Some("剩余 60%"));
    assert_eq!(app.go_value_text(WindowKind::Monthly), Some("已用尽"));
    assert!(app.last_go().is_some());
}

/// A reset stamp adds "… 后重置" to the value cell.
#[test]
fn a_go_window_shows_the_time_to_reset() {
    let mut app = settled(520.0, 460.0);
    let usage = GoUsage {
        usage: go::Usage {
            rolling: Some(go::UsageWindow {
                status: "ok".to_string(),
                percent: 12.0,
                resets_at: Some("2099-01-01T00:00:00Z".to_string()),
            }),
            ..Default::default()
        },
    };
    app.apply_go_result(Ok(usage));
    app.layout(viewport(520.0, 460.0));

    let value = app.go_value_text(WindowKind::Rolling).expect("row");
    assert!(value.starts_with("剩余 88%"), "{value}");
    assert!(value.contains("后重置"), "{value}");
}

/// The view opens on DeepSeek; a tab click queues the choice, `update`
/// applies it, and only one page stays visible.
#[test]
fn the_tab_bar_switches_the_page() {
    let mut app = settled(520.0, 460.0);
    assert_eq!(app.tab(), Tab::DeepSeek);
    assert_eq!(app.page_visible(Tab::DeepSeek), Some(true));
    assert_eq!(app.page_visible(Tab::Go), Some(false));

    let point = app.tab_center(Tab::Go).expect("the tab is laid out");
    app.event(&InputEvent::PointerDown {
        position: point,
        button: PointerButton::Left,
    });
    app.event(&InputEvent::PointerUp {
        position: point,
        button: PointerButton::Left,
    });
    assert_eq!(app.tab(), Tab::DeepSeek, "the click only queues");

    app.update(viewport(520.0, 460.0), 0.016);
    assert_eq!(app.tab(), Tab::Go);
    assert_eq!(app.page_visible(Tab::DeepSeek), Some(false));
    assert_eq!(app.page_visible(Tab::Go), Some(true));
}

/// A failed Go fetch explains itself on the Go page without touching the
/// DeepSeek numbers.
#[test]
fn a_failed_go_fetch_shows_on_the_go_page() {
    let mut app = settled(520.0, 460.0);
    app.apply_go_result(Err("OpenCode Go 密钥被拒绝（401）".to_string()));

    assert_eq!(app.go_error_text(), Some("OpenCode Go 密钥被拒绝（401）"));
    assert_eq!(app.go_status_text(), Some(GO_STATUS_FAILED));
    assert!(
        !app.error_text().unwrap_or_default().contains("OpenCode Go"),
        "the DeepSeek error line stays untouched"
    );
    // The last good numbers stay, like the DeepSeek cards on a failure.
    assert_eq!(app.go_value_text(WindowKind::Rolling), Some("剩余 88%"));
    assert!(app.last_go().is_some());
}

/// One refresh queries both sources; the button stays busy until the second
/// reply lands and only then starts cooling.
#[test]
fn one_refresh_waits_for_both_sources() {
    let mut app = ready(520.0, 460.0);
    app.request_refresh();
    app.update(viewport(520.0, 460.0), 0.016);
    assert!(app.is_loading(), "both fetches are out");

    app.apply_result(Ok(sample()));
    assert!(app.is_loading(), "the Go fetch is still out");

    app.apply_go_result(Ok(sample_go()));
    assert!(!app.is_loading(), "both replied");
    assert!(!app.refresh_ready(), "the throttle is armed again");
}

/// The endpoint display drops the scheme and path, which is what keeps the
/// button on screen in the narrow panel.
#[test]
fn the_endpoint_label_is_just_the_host() {
    assert_eq!(
        endpoint_label("https://api.deepseek.com/user/balance"),
        "api.deepseek.com"
    );
    assert_eq!(
        endpoint_label("https://opencode.ai/zen/go/v1/usage"),
        "opencode.ai"
    );
    assert_eq!(endpoint_label("http://localhost:8080/v1"), "localhost:8080");
    assert_eq!(endpoint_label("no-scheme.example/x"), "no-scheme.example");
}

/// The tab bar must not push the panels wider than the window: a flex chain
/// that overflows shoves the refresh button off a transparent, non-scrolling
/// panel, where it is simply never seen.
#[test]
fn the_panel_fits_its_window() {
    let app = panel();
    let button = app.button_rect().expect("button rect");
    assert!(
        button.right() <= PANEL_WIDTH_TEST + 0.5,
        "the refresh button overflows the panel: {button:?}"
    );
    for slot in &app.slots {
        let card = draw_ui::control(&app.tree, slot.card)
            .expect("card control")
            .rect;
        assert!(
            card.right() <= PANEL_WIDTH_TEST + 0.5,
            "a card overflows the panel: {card:?}"
        );
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
    // The window height the host asks for, so the footer's button fits.
    const HEIGHT: f32 = 500.0;
    let app = laid_out(520.0, HEIGHT);
    let button = app.button_rect().expect("button rect");
    assert!(
        button.left() >= 0.0 && button.right() <= 520.0 && button.bottom() <= HEIGHT,
        "{button:?}"
    );

    // The page column must actually arrange its children: the cards sit
    // below the endpoint line and above the footer's button, not on top of
    // either (the flex-vs-anchor regression).
    let first = draw_ui::control(&app.tree, app.slots[0].card)
        .expect("card control")
        .rect;
    assert!(
        first.top() >= 0.0 && first.bottom() <= button.top(),
        "card {first:?} overlaps the footer button {button:?}"
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
            card.size.height < HEIGHT,
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
            paint.color, app.theme.palette().background,
            "the fill is the theme's backdrop token"
        );
        // Opaque: only the four corners outside the radius may be see-through.
        assert_eq!(app.theme.palette().background.a, 1.0);

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
    let mut app = BalanceApp::new_panel(default_theme(Mode::Dark), TEST_ENDPOINT.to_string());
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
    assert_eq!(paint.color, app.theme.palette().background);

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
    let window = Rect::from_min_size(Vec2::ZERO, Size::new(PANEL_WIDTH_TEST, PANEL_HEIGHT_TEST));
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
