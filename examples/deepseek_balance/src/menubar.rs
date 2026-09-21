//! The macOS menu-bar item: `NSStatusBar` / `NSStatusItem` behind `tray-icon`.
//!
//! `winit` deliberately stops at windows and the event loop — its macOS
//! platform module exposes an activation policy and window decorations, no
//! status item — so the menu bar comes from `tray-icon`, which on macOS is a
//! thin wrapper over `NSStatusBar`/`NSStatusItem` (via `objc2-app-kit`).
//! `muda`, re-exported by `tray-icon`, supplies the dropdown menu.
//!
//! ## Three rules the platform imposes
//!
//! 1. **Create it on the main thread, with the loop already running.** AppKit
//!    needs a live run loop; in `winit` terms the earliest safe point is
//!    [`winit::application::ApplicationHandler::resumed`] (the first resume
//!    immediately follows `StartCause::Init`). Creating it earlier, before
//!    `run_app`, misbehaves around fullscreen apps.
//! 2. **The callbacks do not run inside `winit`'s dispatch.** They run on
//!    AppKit's main run loop, and with [`ControlFlow::Wait`] `winit` is parked
//!    at that moment. Every event therefore has to be posted through an
//!    [`EventLoopProxy`] — see [`forward_events`] — or a click would sit
//!    unnoticed until some unrelated event woke the loop.
//! 3. **`set_title(Some(""))` is how you blank the title.** On macOS the
//!    platform code only touches `NSStatusBarButton.title` when the value is
//!    `Some`, so `None` leaves the previous text in place.
//!
//! ## Left click vs. right click
//!
//! `menu_on_left_click(false)` + `menu_on_right_click(true)` gives the usual
//! macOS split: a left click toggles the balance panel, a right click drops the
//! menu. A left click still produces a [`TrayIconEvent::Click`], because
//! `tray-icon` emits those from its `NSResponder` handlers before it consults
//! the menu flags.
//!
//! ## The item's rectangle is only half trustworthy
//!
//! [`MenuBar::rect`] is `NSStatusItem.button.window.frame`, and AppKit moves
//! that window around: before the menu bar has been laid out it reports the
//! item parked in the screen's bottom-left corner, and it keeps reporting a
//! previous frame for a while after the title changes width. A [`TrayIconEvent`]
//! is better evidence, because the pointer had to be *inside* the item for the
//! click to happen, so `position` is a centre the panel can trust — see
//! [`crate::host`]'s panel placement.
//!
//! [`ControlFlow::Wait`]: winit::event_loop::ControlFlow::Wait

use draw_core::{Rect, Size, Vec2};
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{TrayIcon, TrayIconBuilder, TrayIconEvent};
use winit::event_loop::EventLoopProxy;

use crate::host::UserEvent;

/// `刷新余额` — re-queries the endpoint.
pub const MENU_REFRESH: &str = "quill.deepseek.refresh";
/// `暂停自动刷新` / `继续自动刷新` — toggles the automatic refresh timer.
pub const MENU_PAUSE: &str = "quill.deepseek.pause";
/// `打开/收起面板` — same as clicking the item.
pub const MENU_PANEL: &str = "quill.deepseek.panel";
/// `退出`.
pub const MENU_QUIT: &str = "quill.deepseek.quit";

/// Label of [`MENU_PAUSE`] while the timer is running.
pub const PAUSE_LABEL_RUNNING: &str = "暂停自动刷新";
/// Label of [`MENU_PAUSE`] while the timer is held.
pub const PAUSE_LABEL_PAUSED: &str = "继续自动刷新";

/// Shown in the menu bar before the first reply lands.
pub const TITLE_IDLE: &str = "—";

/// A live status-bar item. Dropping it removes the item.
pub struct MenuBar {
    tray: TrayIcon,
    /// Kept so the throttle can grey out `刷新余额` while it holds.
    ///
    /// With the panel closed this menu is the only place a refused refresh could
    /// be explained, and an entry that looks live but silently does nothing is
    /// worse than a greyed one.
    refresh: MenuItem,
    /// Kept so [`MenuBar::set_paused`] can swap its label between
    /// [`PAUSE_LABEL_RUNNING`] and [`PAUSE_LABEL_PAUSED`].
    pause: MenuItem,
}

impl MenuBar {
    /// Adds the item with `title` next to it and the actions menu behind it.
    ///
    /// No icon is passed on purpose: the point of this app is the number, and a
    /// text-only status item renders in the system font (crisp at every scale
    /// factor) instead of a scaled bitmap.
    pub fn new(title: &str) -> Result<Self, String> {
        let refresh = MenuItem::with_id(MENU_REFRESH, "刷新余额", true, None);
        let pause = MenuItem::with_id(MENU_PAUSE, PAUSE_LABEL_RUNNING, true, None);
        let panel = MenuItem::with_id(MENU_PANEL, "打开 / 收起面板", true, None);
        let quit = MenuItem::with_id(MENU_QUIT, "退出", true, None);
        let separator = PredefinedMenuItem::separator();

        let menu = Menu::new();
        menu.append_items(&[&refresh, &pause, &panel, &separator, &quit])
            .map_err(|error| format!("构建菜单失败: {error}"))?;

        let tray = TrayIconBuilder::new()
            .with_title(title)
            .with_tooltip(TITLE_IDLE)
            .with_menu(Box::new(menu))
            // Left click drives our own panel; right click is the menu.
            .with_menu_on_left_click(false)
            .with_menu_on_right_click(true)
            .build()
            .map_err(|error| format!("创建状态栏项失败: {error}"))?;

        Ok(Self {
            tray,
            refresh,
            pause,
        })
    }

    /// Greys out or re-enables `刷新余额`, mirroring the view's throttle.
    pub fn set_refresh_enabled(&self, enabled: bool) {
        self.refresh.set_enabled(enabled);
    }

    /// Swaps `暂停自动刷新` / `继续自动刷新` to match the timer's state.
    pub fn set_paused(&self, paused: bool) {
        self.pause.set_text(if paused {
            PAUSE_LABEL_PAUSED
        } else {
            PAUSE_LABEL_RUNNING
        });
    }

    /// The current text of `暂停自动刷新`, read back from the platform.
    ///
    /// The self-check narrates this: a native menu's label is otherwise only
    /// visible in a screenshot, which this project does not take.
    pub fn pause_label(&self) -> String {
        self.pause.text()
    }

    /// Greys out `暂停自动刷新` when the run has no automatic timer (`--every 0`),
    /// so the entry does not promise a pause that would do nothing.
    pub fn set_pause_enabled(&self, enabled: bool) {
        self.pause.set_enabled(enabled);
    }

    /// Whether `刷新余额` is currently enabled, read back from the platform.
    ///
    /// The self-check narrates this: a screenshot is the only other way to see a
    /// greyed-out native menu item, and this project does not take screenshots.
    pub fn refresh_enabled(&self) -> bool {
        self.refresh.is_enabled()
    }

    /// Replaces the text beside the item.
    pub fn set_title(&self, title: &str) {
        self.tray.set_title(Some(title));
    }

    /// Replaces the hover tooltip.
    pub fn set_tooltip(&self, tooltip: &str) {
        let _ = self.tray.set_tooltip(Some(tooltip));
    }

    /// The item's rectangle in screen space: physical pixels, top-left origin —
    /// the same space as winit's `MonitorHandle` and `Window::set_outer_position`.
    pub fn rect(&self) -> Option<Rect> {
        self.tray.rect().map(tray_rect)
    }
}

/// Converts `tray-icon`'s rectangle into the workspace's own.
///
/// `tray-icon` reports physical pixels with a top-left origin on macOS (it flips
/// AppKit's bottom-left origin), which is the space `winit` places windows in.
pub fn tray_rect(rect: tray_icon::Rect) -> Rect {
    Rect::from_min_size(
        Vec2::new(rect.position.x as f32, rect.position.y as f32),
        Size::new(rect.size.width as f32, rect.size.height as f32),
    )
}

/// Routes status-item and menu callbacks into the `winit` loop.
///
/// Call this once, before building the item. Both handler slots are process
/// globals that can only be filled once, which is why this lives next to
/// [`MenuBar::new`] rather than inside it.
pub fn forward_events(proxy: EventLoopProxy<UserEvent>) {
    let clicks = proxy.clone();
    TrayIconEvent::set_event_handler(Some(move |event| {
        let _ = clicks.send_event(UserEvent::Tray(event));
    }));
    MenuEvent::set_event_handler(Some(move |event| {
        let _ = proxy.send_event(UserEvent::Menu(event));
    }));
}

/// Where to put a panel of `size` so it hangs off the status item.
///
/// The panel is centred on `tray`: the item marks the middle of the panel, the
/// way a popover points back at its anchor. It is then clamped into `monitor`,
/// so an item close to a screen edge pushes the panel inward instead of letting
/// it hang off — with a panel this wide and an item in the menu bar's corner,
/// the clamp is what you see most of the time.
///
/// All four inputs are physical pixels in screen space; the result is the
/// panel's top-left corner.
pub fn anchor(panel: Size, tray: Rect, monitor: Rect, gap: f32) -> Vec2 {
    let max_x = (monitor.right() - panel.width).max(monitor.left());
    let max_y = (monitor.bottom() - panel.height).max(monitor.top());
    Vec2::new(
        (tray.center().x - panel.width / 2.0).clamp(monitor.left(), max_x),
        (tray.bottom() + gap).clamp(monitor.top(), max_y),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 1920×1080 monitor at the origin, and a 24×24 item 8px from its right
    /// edge (macOS draws the item just inside the screen).
    fn screen() -> Rect {
        Rect::from_min_size(Vec2::ZERO, Size::new(1920.0, 1080.0))
    }

    fn item() -> Rect {
        Rect::from_min_size(Vec2::new(1888.0, 0.0), Size::new(24.0, 24.0))
    }

    #[test]
    fn the_panel_is_centred_on_the_item() {
        // An item comfortably inside the menu bar: the panel's centre lands on
        // the item's centre, so the panel reads as pointing at it.
        let panel = Size::new(300.0, 400.0);
        let item = Rect::from_min_size(Vec2::new(900.0, 0.0), Size::new(40.0, 24.0));
        let at = anchor(panel, item, screen(), 6.0);

        assert_eq!(at.x + panel.width / 2.0, item.center().x);
        assert_eq!(at.y, item.bottom() + 6.0, "and it hangs under the item");
    }

    #[test]
    fn an_item_near_the_edge_pushes_the_panel_inward() {
        // The screen's right edge is where status items live: a centred panel
        // would hang off, so the clamp slides it back to the edge instead.
        let panel = Size::new(460.0, 480.0);
        let at = anchor(panel, item(), screen(), 6.0);

        assert_eq!(at.x + panel.width, screen().right(), "flush with the edge");
        assert!(at.x < item().center().x - panel.width / 2.0, "{at:?}");
    }

    #[test]
    fn a_panel_wider_than_the_gap_slides_instead_of_overflowing() {
        // An item at the very left of the screen has no room to its left.
        let far_left = Rect::from_min_size(Vec2::new(4.0, 0.0), Size::new(24.0, 24.0));
        let panel = Size::new(460.0, 480.0);
        let at = anchor(panel, far_left, screen(), 6.0);

        assert_eq!(at.x, 0.0, "clamped to the monitor's left edge");
    }

    #[test]
    fn a_tall_panel_is_clamped_into_the_monitor() {
        let panel = Size::new(460.0, 2000.0);
        let at = anchor(panel, item(), screen(), 6.0);

        assert_eq!(at.y, 0.0, "clamped to the monitor's top edge");
    }

    #[test]
    fn a_panel_taller_than_the_monitor_still_starts_on_screen() {
        // Degenerate case: clamping must not panic when min > max.
        let tiny = Rect::from_min_size(Vec2::ZERO, Size::new(200.0, 100.0));
        let panel = Size::new(460.0, 480.0);
        let at = anchor(panel, item(), tiny, 6.0);

        assert!(at.x.is_finite() && at.y.is_finite(), "{at:?}");
    }
}
