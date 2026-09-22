//! Shared, backend-neutral **component gallery**: a catalog of every
//! `draw_components` widget and core capability, grouped and previewed live.
//!
//! ```text
//! ┌──────────────┬───────────────────────────────────────────────┐
//! │ sidebar      │ preview (a `Router`, one page per group)      │
//! │ groups       │ group header                                  │
//! │ theme toggle │ two-column grid of live component cards       │
//! └──────────────┴───────────────────────────────────────────────┘
//! ```
//!
//! The catalog lives in [`catalog`] (pure data); [`previews`] builds the live
//! cards; [`sidebar`] builds the navigation. The theme is a value passed to the
//! constructors, so switching light/dark rebuilds the scene with the same shared
//! state (the dark/light story is a token swap, not a second code path).
//!
//! Hosts drive it through the usual pipeline:
//!
//! ```text
//! Input -> DemoApp::event -> DemoApp::update -> DemoApp::layout
//!                                                -> DemoApp::paint -> DrawList
//! ```
//!
//! The app owns no window/backend/browser API. Both `wgpu_demo` and the WASM
//! `web_demo` build and drive this exact app.

mod catalog;
mod previews;
mod sidebar;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use draw_components::{
    Component, Flex, ListState, NodeRef, Overlays, Panel, Router, ScrollViewState,
};
use draw_core::{
    Color, Cursor, Edges, EventResult, InputEvent, NodeId, Rect, Size, Vec2, ViewportSize,
};
use draw_render::PaintContext;
use draw_scene::{SceneChild, SceneTree};
use draw_theme::{default_theme, Mode, Theme, Tone};
use draw_ui::{MouseFilter, TextMeasurer, Widget};

/// Shared, mutable gallery state.
///
/// The cells are cheap to clone, so callbacks (which cannot borrow the app)
/// write requests here and [`DemoApp::update`] drains them.
#[derive(Clone)]
pub(crate) struct GalleryState {
    /// Selected group, shared with the preview [`Router`].
    pub(crate) group: Rc<Cell<usize>>,
    /// Clicks on the tracked primary button (the theme toggle).
    pub(crate) clicks: Rc<Cell<u32>>,
    /// A requested light/dark switch, applied by [`DemoApp::update`].
    pub(crate) theme_request: Rc<Cell<Option<Mode>>>,
    /// The menu preview's button (anchor for the drop-down).
    pub(crate) menu_anchor: NodeRef,
    pub(crate) menu_request: Rc<Cell<bool>>,
    pub(crate) confirm_request: Rc<Cell<bool>>,
    pub(crate) message_request: Rc<Cell<bool>>,
}

impl GalleryState {
    fn new() -> Self {
        Self {
            group: Rc::new(Cell::new(0)),
            clicks: Rc::new(Cell::new(0)),
            theme_request: Rc::new(Cell::new(None)),
            menu_anchor: NodeRef::new(),
            menu_request: Rc::new(Cell::new(false)),
            confirm_request: Rc::new(Cell::new(false)),
            message_request: Rc::new(Cell::new(false)),
        }
    }
}

/// The built scene and the handles the app keeps from it.
struct SceneParts {
    tree: SceneTree,
    router: Router,
    sidebar: NodeId,
    primary: NodeId,
    lists: Vec<ListState>,
    routers: Vec<Router>,
    scrolls: Vec<ScrollViewState>,
}

/// Builds the whole scene for `theme` against the shared `state`.
fn build_scene(theme: &'static dyn Theme, state: &GalleryState) -> SceneParts {
    let sidebar_slot = NodeRef::new();
    let primary_slot = NodeRef::new();
    let content = NodeRef::new();
    let routers = Rc::new(RefCell::new(Vec::new()));
    let mut lists = Vec::new();
    let mut scrolls = Vec::new();

    let mut tree = Flex::column()
        .mouse_filter(MouseFilter::Ignore)
        .child(
            Flex::row()
                .gap(0.0)
                .padding(Edges::ZERO)
                .mouse_filter(MouseFilter::Ignore)
                .child(sidebar::build(theme, state, &primary_slot).ref_(&sidebar_slot))
                .child(
                    Panel::new()
                        .color(Color::TRANSPARENT)
                        .flat()
                        .grow(1.0)
                        .clip(true)
                        .ref_(&content),
                ),
        )
        .into_tree();

    let content_root = content.get().expect("content pane mounted");
    let mut router = Router::with_route(content_root, state.group.clone());
    for group in 0..catalog::GROUPS.len() {
        let view = previews::group_view(group, theme, state, &mut lists, &routers, &mut scrolls);
        let node = tree.add_child(content_root, view);
        router.add_node(node);
    }
    router.sync(&mut tree);

    let routers = std::mem::take(&mut *routers.borrow_mut());
    SceneParts {
        tree,
        router,
        sidebar: sidebar_slot.get().expect("sidebar mounted"),
        primary: primary_slot.get().expect("primary button mounted"),
        lists,
        routers,
        scrolls,
    }
}

/// Application state shared by every demo host.
pub struct DemoApp {
    tree: SceneTree,
    theme: &'static dyn Theme,
    state: GalleryState,
    router: Router,
    overlays: Overlays,
    sidebar: NodeId,
    primary: NodeId,
    titlebar_inset: f32,
    viewport: ViewportSize,
    measurer: Option<Rc<dyn TextMeasurer>>,
    lists: Vec<ListState>,
    routers: Vec<Router>,
    scrolls: Vec<ScrollViewState>,
}

impl Default for DemoApp {
    fn default() -> Self {
        Self::new()
    }
}

impl DemoApp {
    /// Builds the app with the dark theme.
    pub fn new() -> Self {
        Self::with_mode(Mode::Dark)
    }

    /// Builds the app with a built-in light/dark theme.
    pub fn with_mode(mode: Mode) -> Self {
        Self::with_theme(default_theme(mode))
    }

    /// Builds the app with an explicit theme.
    pub fn with_theme(theme: &'static dyn Theme) -> Self {
        let state = GalleryState::new();
        let parts = build_scene(theme, &state);
        Self {
            tree: parts.tree,
            theme,
            state,
            router: parts.router,
            overlays: Overlays::new(theme),
            sidebar: parts.sidebar,
            primary: parts.primary,
            titlebar_inset: 0.0,
            viewport: ViewportSize::new(Size::new(1200.0, 760.0)),
            measurer: None,
            lists: parts.lists,
            routers: parts.routers,
            scrolls: parts.scrolls,
        }
    }

    /// Rebuilds the scene against the built-in theme for `mode`, preserving the
    /// shared state (selection, click count, list/router handles).
    pub fn set_mode(&mut self, mode: Mode) {
        self.theme = default_theme(mode);
        let parts = build_scene(self.theme, &self.state);
        self.tree = parts.tree;
        self.router = parts.router;
        self.sidebar = parts.sidebar;
        self.primary = parts.primary;
        self.lists = parts.lists;
        self.routers = parts.routers;
        self.scrolls = parts.scrolls;
        self.overlays = Overlays::new(self.theme);

        if let Some(measurer) = self.measurer.clone() {
            self.set_text_measurer(measurer);
        }
        let inset = self.titlebar_inset;
        self.titlebar_inset = 0.0;
        self.set_titlebar_inset(inset);
    }

    // -- accessors ---------------------------------------------------------

    /// The scene tree shared by world and UI nodes.
    pub fn tree(&self) -> &SceneTree {
        &self.tree
    }

    /// The active theme.
    pub fn theme(&self) -> &'static dyn Theme {
        self.theme
    }

    /// The overlay layer (menus, dialogs, toasts).
    pub fn overlays(&self) -> &Overlays {
        &self.overlays
    }

    pub fn viewport(&self) -> ViewportSize {
        self.viewport
    }

    /// The sidebar root (its padding reserves the title-bar safe area).
    pub fn sidebar(&self) -> NodeId {
        self.sidebar
    }

    /// The selected group index.
    pub fn group(&self) -> usize {
        self.state.group.get()
    }

    /// The number of catalog groups (the router's view count).
    pub fn group_count() -> usize {
        catalog::GROUPS.len()
    }

    /// Selects a group and applies it to the preview router.
    pub fn show_group(&mut self, index: usize) {
        let index = index.min(catalog::GROUPS.len().saturating_sub(1));
        self.state.group.set(index);
        self.router.sync(&mut self.tree);
    }

    /// The preview router (one view per group).
    pub fn router(&self) -> &Router {
        &self.router
    }

    /// Scroll offset of the active group's preview page (0 when it fits).
    pub fn preview_scroll(&self) -> f32 {
        self.scrolls
            .get(self.group())
            .map(|state| state.offset())
            .unwrap_or(0.0)
    }

    /// Number of times the tracked primary button (the theme toggle) was
    /// clicked.
    pub fn clicks(&self) -> u32 {
        self.state.clicks.get()
    }

    /// Center of the tracked primary button in logical viewport coordinates.
    pub fn button_center(&self) -> Option<Vec2> {
        draw_ui::control(&self.tree, self.primary).map(|control| control.rect.center())
    }

    /// Extra top padding currently reserved on the sidebar (0 unless a
    /// transparent title bar requested a safe area).
    pub fn titlebar_inset(&self) -> f32 {
        self.titlebar_inset
    }

    /// Reserves `inset` extra logical pixels of top padding on the **sidebar
    /// only**, so its content clears a transparent native title bar (e.g. the
    /// macOS traffic lights). Zero means no safe area, which is what browser and
    /// WASM hosts want.
    pub fn set_titlebar_inset(&mut self, inset: f32) {
        let inset = inset.max(0.0);
        if (self.titlebar_inset - inset).abs() <= f32::EPSILON {
            return;
        }
        self.titlebar_inset = inset;
        if let Some(control) = draw_components::control_mut(&mut self.tree, self.sidebar) {
            if let Widget::Flex(flex) = &mut control.widget {
                flex.padding.top = sidebar::SIDEBAR_PADDING_TOP + inset;
            }
        }
        draw_ui::mark_dirty(&mut self.tree, self.sidebar);
    }

    /// Installs `measurer` for both the main UI and the overlay layer.
    pub fn set_text_measurer(&mut self, measurer: Rc<dyn TextMeasurer>) {
        self.measurer = Some(measurer.clone());
        draw_ui::set_text_measurer(&mut self.tree, measurer.clone());
        self.overlays.set_text_measurer(measurer);
    }

    // -- pipeline ----------------------------------------------------------

    /// Drains requests (theme switch, overlay opens) and applies routers and
    /// overlay timers for the frame.
    pub fn update(&mut self, viewport: ViewportSize, dt: f32) {
        self.viewport = viewport;
        self.overlays.update(dt);

        if let Some(mode) = self.state.theme_request.replace(None) {
            self.set_mode(mode);
            return;
        }

        self.router.sync(&mut self.tree);
        for router in &mut self.routers {
            router.sync(&mut self.tree);
        }

        if self.state.menu_request.replace(false) {
            if let Some(anchor) = self.state.menu_anchor.get() {
                let theme = self.theme;
                self.overlays.menu(anchor, move |tree, node| {
                    previews::menu_content(tree, node, theme);
                });
            }
        }
        if self.state.confirm_request.replace(false) {
            let id = self
                .overlays
                .confirm("Delete item?", "This action cannot be undone.");
            self.overlays.destructive(id, true);
        }
        if self.state.message_request.replace(false) {
            self.overlays.message_tone("Saved", Tone::Success);
        }
    }

    /// Resolves UI layout for `viewport`, syncs the virtualized lists, then
    /// positions the overlays.
    pub fn layout(&mut self, viewport: ViewportSize) {
        self.viewport = viewport;
        draw_ui::layout(&mut self.tree, viewport);
        self.tree.update();

        let mut changed = false;
        for state in &mut self.lists {
            changed |= state.sync(&mut self.tree);
        }
        for state in &mut self.scrolls {
            changed |= state.sync(&mut self.tree);
        }
        if changed {
            draw_ui::layout(&mut self.tree, viewport);
        }
        self.overlays.layout(&self.tree, viewport);
    }

    /// Emits this frame's `DrawList` into `ctx`.
    ///
    /// Order: window background, UI content (+ decor), then overlays.
    pub fn paint(&self, ctx: &mut PaintContext) {
        let size = self.viewport.logical_size();
        ctx.fill_rect(
            Rect::from_min_size(Vec2::ZERO, size),
            self.theme.palette().background,
        );
        draw_ui::paint(&self.tree, ctx);
        self.overlays.paint(ctx);
    }

    /// Routes an event to the overlays first, then UI interactions.
    pub fn event(&mut self, event: &InputEvent) -> EventResult {
        if self.overlays.handle_input(event).is_handled() {
            return EventResult::Handled;
        }
        draw_ui::route_input(&mut self.tree, event)
    }

    /// Controls in the gallery UI.
    pub fn control_count(&self) -> usize {
        draw_ui::control_count(&self.tree)
    }

    /// Whether the pointer is over anything clickable. Hosts use this for
    /// cursor feedback.
    pub fn pointer_over_clickable(&self) -> bool {
        draw_ui::hovered(&self.tree).is_some_and(|id| draw_ui::is_interactive(&self.tree, id))
    }

    /// Cursor the host should show for the current pointer position.
    pub fn cursor(&self) -> Cursor {
        draw_ui::hovered_cursor(&self.tree)
    }
}

#[cfg(test)]
mod tests;
