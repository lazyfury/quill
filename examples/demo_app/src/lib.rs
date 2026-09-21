//! Shared, backend-neutral demo application: a three-column, macOS-style notes
//! app built from `draw_components` components on the `draw_components` runtime.
//!
//! Layout is the classic macOS split view:
//!
//! ```text
//! ┌──────────┬────────────────┬──────────────────────────────┐
//! │ sidebar  │  content list  │  detail                      │
//! │ 220px    │  324px         │  fills the rest              │
//! │ app icon │  header        │  toolbar / hero / body       │
//! │ nav      │  note rows     │  actions                     │
//! └──────────┴────────────────┴──────────────────────────────┘
//! ```
//!
//! Icons and images are monochrome rounded squares (placeholders). Every node is
//! composed with [`SceneTree::add_child`](draw_scene::SceneTree::add_child) and
//! components chain `.child()`; the theme is a value passed to the constructors.
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

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use draw_components::{
    Badge, Button, Checkbox, Divider, NodeRef, Overlays, ResizeHandle, Router, Spec, Switch, Text,
};
use draw_components::{Column, Component, Flex, Label, Panel, Row};
use draw_core::{
    Color, Cursor, Edges, EventResult, InputEvent, NodeId, Rect, Size, Vec2, ViewportSize,
};
use draw_render::{CornerRadii, PaintContext};
use draw_scene::{SceneChild, SceneTree};
use draw_theme::{default_theme, radius, space, Mode, TextSize, Theme, Tone};
use draw_ui::{
    fill_rounded_rect, fill_rounded_rect_corners, inset, Align, Control, Justify, MouseFilter,
    SizeBasis, SurfaceStyle, TextMeasurer, TextOptions, Widget,
};

/// Sidebar width in logical pixels.
pub const SIDEBAR_WIDTH: f32 = 220.0;
/// Content-list width in logical pixels.
pub const LIST_WIDTH: f32 = 324.0;
/// Width of a static column separator (a real 1px line in the split view).
pub const SEPARATOR_WIDTH: f32 = 1.0;
/// Top padding of the sidebar before any title-bar safe area is added.
const SIDEBAR_PADDING_TOP: f32 = space::MD;
/// Pointer hit area of the sidebar resize gutter.
pub const RESIZE_GUTTER: f32 = 6.0;
/// Bounds the sidebar can be resized to.
pub const SIDEBAR_MIN: f32 = 140.0;
pub const SIDEBAR_MAX: f32 = 400.0;
/// Bounds the list can be resized to.
pub const LIST_MIN: f32 = 200.0;
pub const LIST_MAX: f32 = 560.0;
/// X offset where the detail pane starts (after both resize gutters).
pub const DETAIL_X: f32 = SIDEBAR_WIDTH + RESIZE_GUTTER + LIST_WIDTH + RESIZE_GUTTER;

/// A note shown in the list/detail panes.
#[derive(Debug, Clone, Copy)]
struct Note {
    title: &'static str,
    snippet: &'static str,
    body: &'static str,
    tag: &'static str,
    modified: &'static str,
}

const NOTES: &[Note] = &[
    Note {
        title: "Design tokens",
        snippet: "Monochrome palette and scales",
        body: "The theme exposes a light and dark palette, a 13-step spacing scale, \
               restrained radii and a compact type scale. Components resolve every \
               color through the palette, so switching modes is a token swap.",
        tag: "Design",
        modified: "2m ago",
    },
    Note {
        title: "Layout engine",
        snippet: "Flex, grid and intrinsic sizing",
        body: "Measure computes intrinsic sizes bottom-up; arrange assigns absolute \
               rectangles top-down. Containers own their children, and non-container \
               controls keep the anchor/offset model.",
        tag: "Rust",
        modified: "1h ago",
    },
    Note {
        title: "Backend notes",
        snippet: "Canvas 2D, wgpu and recording",
        body: "Every backend consumes the same backend-neutral DrawList. The recording \
               backend makes the whole pipeline testable headlessly, without a browser \
               or a GPU.",
        tag: "Docs",
        modified: "Yesterday",
    },
    Note {
        title: "Component kit",
        snippet: "Themed cards, badges and controls",
        body: "draw_components layers themed chrome over draw_ui without extending the core \
               Widget enum. Surfaces paint behind content; indicators paint in front.",
        tag: "Design",
        modified: "2d ago",
    },
    Note {
        title: "Release checklist",
        snippet: "cargo fmt / check / test / bench",
        body: "Run the per-stage gate before merging: formatting, workspace check, the \
               full test suite and a bench compile. Reports wait for approval.",
        tag: "Release",
        modified: "3d ago",
    },
    Note {
        title: "Reading list",
        snippet: "Rendering and UI architecture",
        body: "A short list of references on immediate-mode UI, retained scene graphs \
               and backend abstractions, kept here as a personal collection.",
        tag: "Personal",
        modified: "Last week",
    },
];

const NAV_ITEMS: &[&str] = &["All Notes", "Recent", "Favorites", "Shared"];
const TAG_ITEMS: &[&str] = &["Design", "Rust", "Docs"];

/// Shared, mutable application state.
///
/// The cells are cheap to clone, so the panes and the host read/write the same
/// values without mutexes while each pane still owns its own node handles (see
/// [`Sidebar`], [`NoteList`] and [`DetailPane`]).
#[derive(Clone)]
struct DemoState {
    selected: Rc<Cell<usize>>,
    selected_nav: Rc<Cell<usize>>,
    clicks: Rc<Cell<u32>>,
    sidebar_width: Rc<Cell<f32>>,
    list_width: Rc<Cell<f32>>,
    delete_requested: Rc<Cell<bool>>,
    deleted: Rc<Cell<bool>>,
}

impl DemoState {
    fn new() -> Self {
        Self {
            selected: Rc::new(Cell::new(0)),
            selected_nav: Rc::new(Cell::new(0)),
            clicks: Rc::new(Cell::new(0)),
            sidebar_width: Rc::new(Cell::new(SIDEBAR_WIDTH)),
            list_width: Rc::new(Cell::new(LIST_WIDTH)),
            delete_requested: Rc::new(Cell::new(false)),
            deleted: Rc::new(Cell::new(false)),
        }
    }
}

/// Node ids the panes report back through callback refs at mount time.
///
/// The panes only *write* these slots; [`DemoApp`] reads them after mounting and
/// keeps the resolved values as plain fields.
#[derive(Clone, Default)]
struct Handles {
    sidebar: NodeRef,
    list: NodeRef,
    detail: NodeRef,
    hero: NodeRef,
    detail_title: NodeRef,
    detail_body: NodeRef,
    detail_tag: NodeRef,
    detail_meta: NodeRef,
    primary_button: NodeRef,
    nav_rows: Rc<RefCell<Vec<NodeId>>>,
    list_rows: Rc<RefCell<Vec<NodeId>>>,
    router: Rc<RefCell<Option<Router>>>,
}

/// The left-hand navigation pane: app icon, search, library nav and tag nav.
struct Sidebar {
    inner: Column,
}

/// The middle content list: header, divider and one row per note.
struct NoteList {
    inner: Column,
}

/// The right-hand routed detail pane: note / settings views.
struct DetailPane {
    spec: Spec,
    theme: &'static dyn Theme,
    route: Rc<Cell<usize>>,
    note_view: Column,
    settings: Column,
    handles: Handles,
}

/// Application state shared by every demo host.
pub struct DemoApp {
    tree: SceneTree,
    theme: &'static dyn Theme,
    state: DemoState,
    sidebar: NodeId,
    list: NodeId,
    detail: NodeId,
    hero: NodeId,
    nav_rows: Vec<NodeId>,
    list_rows: Vec<NodeId>,
    detail_title: NodeId,
    detail_body: NodeId,
    detail_tag: NodeId,
    detail_meta: NodeId,
    primary_button: NodeId,
    /// Router for the right-hand pane (0 = note detail, 1 = settings).
    detail_router: Router,
    overlays: Overlays,
    /// Extra top padding reserved on the sidebar to clear a transparent title bar.
    titlebar_inset: f32,
    viewport: ViewportSize,
}

impl Default for DemoApp {
    fn default() -> Self {
        Self::new()
    }
}

impl DemoApp {
    /// Builds the app with the dark theme.
    pub fn new() -> Self {
        Self::with_theme(default_theme(Mode::Dark))
    }

    /// Builds the app with an explicit theme.
    pub fn with_theme(theme: &'static dyn Theme) -> Self {
        // Shared state and node handles are created here and passed into the
        // panes; the panes report their internal node ids back via `ref_`.
        let state = DemoState::new();
        let handles = Handles::default();

        // Top-level control (anchored to the viewport) plus a split row that
        // owns the three panes. The whole scene composes declaratively and is
        // mounted once with `into_tree`; `tree.add_child` stays for runtime
        // additions (overlays, router views).
        let tree = Flex::column()
            .mouse_filter(MouseFilter::Ignore)
            .child(
                Flex::row()
                    .gap(0.0)
                    .padding(Edges::ZERO)
                    .mouse_filter(MouseFilter::Ignore)
                    .child(Sidebar::new(theme, &state, &handles).ref_(&handles.sidebar))
                    .child(
                        ResizeHandle::vertical(theme)
                            .target(handles.sidebar.clone())
                            .width(state.sidebar_width.clone())
                            .min(SIDEBAR_MIN)
                            .max(SIDEBAR_MAX),
                    )
                    .child(NoteList::new(theme, &state, &handles).ref_(&handles.list))
                    .child(
                        ResizeHandle::vertical(theme)
                            .target(handles.list.clone())
                            .width(state.list_width.clone())
                            .min(LIST_MIN)
                            .max(LIST_MAX),
                    )
                    .child(DetailPane::new(theme, &state, &handles).ref_(&handles.detail)),
            )
            .into_tree();

        // Resolve the callback refs into plain values before constructing `Self`.
        let nav_rows = handles.nav_rows.borrow().clone();
        let list_rows = handles.list_rows.borrow().clone();
        let detail_router = handles.router.borrow_mut().take().expect("router mounted");

        Self {
            tree,
            theme,
            state,
            sidebar: handles.sidebar.get().expect("sidebar mounted"),
            list: handles.list.get().expect("list mounted"),
            detail: handles.detail.get().expect("detail mounted"),
            hero: handles.hero.get().expect("detail hero mounted"),
            nav_rows,
            list_rows,
            detail_title: handles.detail_title.get().expect("detail title mounted"),
            detail_body: handles.detail_body.get().expect("detail body mounted"),
            detail_tag: handles.detail_tag.get().expect("detail tag mounted"),
            detail_meta: handles.detail_meta.get().expect("detail meta mounted"),
            primary_button: handles
                .primary_button
                .get()
                .expect("primary button mounted"),
            detail_router,
            overlays: Overlays::new(theme),
            titlebar_inset: 0.0,
            viewport: ViewportSize::new(Size::new(1100.0, 720.0)),
        }
    }

    // -- accessors ---------------------------------------------------------

    /// The scene tree shared by world and UI nodes.
    pub fn tree(&self) -> &SceneTree {
        &self.tree
    }

    /// Installs `measurer` for both the main UI and the overlay layer.
    pub fn set_text_measurer(&mut self, measurer: Rc<dyn TextMeasurer>) {
        draw_ui::set_text_measurer(&mut self.tree, measurer.clone());
        self.overlays.set_text_measurer(measurer);
    }

    /// Reserves `inset` extra logical pixels of top padding on the **sidebar
    /// only**, so its content clears a transparent native title bar (e.g. the
    /// macOS traffic lights). The other panes are left untouched. Zero (the
    /// default) means no safe area, which is what browser/WASM hosts want.
    pub fn set_titlebar_inset(&mut self, inset: f32) {
        let inset = inset.max(0.0);
        if (self.titlebar_inset - inset).abs() <= f32::EPSILON {
            return;
        }
        self.titlebar_inset = inset;
        if let Some(control) = draw_components::control_mut(&mut self.tree, self.sidebar) {
            if let draw_ui::Widget::Flex(flex) = &mut control.widget {
                flex.padding.top = SIDEBAR_PADDING_TOP + inset;
            }
        }
        draw_ui::mark_dirty(&mut self.tree, self.sidebar);
    }

    /// Extra top padding currently reserved on the sidebar (0 unless a
    /// transparent title bar requested a safe area).
    pub fn titlebar_inset(&self) -> f32 {
        self.titlebar_inset
    }

    pub fn overlays(&self) -> &Overlays {
        &self.overlays
    }

    pub fn theme(&self) -> &'static dyn Theme {
        self.theme
    }

    pub fn viewport(&self) -> ViewportSize {
        self.viewport
    }

    pub fn sidebar(&self) -> NodeId {
        self.sidebar
    }

    pub fn list(&self) -> NodeId {
        self.list
    }

    pub fn detail(&self) -> NodeId {
        self.detail
    }

    pub fn hero(&self) -> NodeId {
        self.hero
    }

    pub fn list_rows(&self) -> &[NodeId] {
        &self.list_rows
    }

    pub fn nav_rows(&self) -> &[NodeId] {
        &self.nav_rows
    }

    pub fn selected(&self) -> usize {
        self.state.selected.get()
    }

    pub fn selected_nav(&self) -> usize {
        self.state.selected_nav.get()
    }

    /// Current route of the right-hand detail pane (`0` = note, `1` = settings).
    pub fn detail_route(&self) -> usize {
        self.detail_router.index()
    }

    /// Switches the right-hand detail pane to route `index` and applies it.
    pub fn show_detail_route(&mut self, index: usize) {
        self.detail_router.go(&mut self.tree, index);
    }

    /// The detail router (view `0` = note, view `1` = settings).
    pub fn detail_router(&self) -> &Router {
        &self.detail_router
    }

    /// Current sidebar width, in logical pixels (draggable via the gutter).
    pub fn sidebar_width(&self) -> f32 {
        self.state.sidebar_width.get()
    }

    /// Current list width, in logical pixels (draggable via the gutter).
    pub fn list_width(&self) -> f32 {
        self.state.list_width.get()
    }

    /// Number of times the primary ("New Note") button has been clicked.
    pub fn clicks(&self) -> u32 {
        self.state.clicks.get()
    }

    /// Center of the primary button in logical viewport coordinates.
    pub fn button_center(&self) -> Option<Vec2> {
        draw_ui::control(&self.tree, self.primary_button).map(|control| control.rect.center())
    }

    /// Text of the detail title label (used by tests).
    pub fn detail_title_text(&self) -> Option<&str> {
        draw_ui::widget(&self.tree, self.detail_title).and_then(|widget| widget.text())
    }

    /// Text of the list-item count badge label.
    pub fn detail_body_text(&self) -> Option<&str> {
        draw_ui::widget(&self.tree, self.detail_body).and_then(|widget| widget.text())
    }

    // -- pipeline ----------------------------------------------------------

    /// Updates state-driven text and overlay timers.
    pub fn update(&mut self, viewport: ViewportSize, dt: f32) {
        self.viewport = viewport;
        self.overlays.update(dt);
        // Apply the right-pane route (a click only writes the shared cell).
        self.detail_router.sync(&mut self.tree);

        if self.state.delete_requested.replace(false) {
            let deleted = self.state.deleted.clone();
            let id = self
                .overlays
                .confirm("Delete note?", "This cannot be undone.");
            self.overlays
                .confirm_label(id, "Delete")
                .destructive(id, true)
                .on_confirm(id, move || deleted.set(true));
        }
        if self.state.deleted.replace(false) {
            self.overlays.message_tone("Note deleted", Tone::Success);
        }

        let index = self.state.selected.get().min(NOTES.len() - 1);
        let note = &NOTES[index];
        draw_components::set_text(&mut self.tree, self.detail_title, note.title);
        draw_components::set_text(&mut self.tree, self.detail_body, note.body);
        draw_components::set_text(&mut self.tree, self.detail_tag, note.tag);
        draw_components::set_text(
            &mut self.tree,
            self.detail_meta,
            format!("Edited {} · {}", note.modified, note.tag),
        );
    }

    /// Resolves UI layout for `viewport`, then positions the overlays.
    pub fn layout(&mut self, viewport: ViewportSize) {
        self.viewport = viewport;
        draw_ui::layout(&mut self.tree, viewport);
        self.tree.update();
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

    /// Controls in the demo UI.
    pub fn control_count(&self) -> usize {
        draw_ui::control_count(&self.tree)
    }

    /// Whether the pointer is over anything clickable (a themed component or
    /// core button). Hosts use this for cursor feedback.
    pub fn pointer_over_clickable(&self) -> bool {
        draw_ui::hovered(&self.tree).is_some_and(|id| draw_ui::is_interactive(&self.tree, id))
    }

    /// Cursor the host should show for the current pointer position.
    pub fn cursor(&self) -> Cursor {
        draw_ui::hovered_cursor(&self.tree)
    }
}

/// A compact square placeholder: hover surface + a small inner mark.
fn icon_box(theme: &'static dyn Theme, size: f32) -> Panel {
    Panel::new()
        .color(Color::TRANSPARENT)
        .flat()
        .min_size(size, size)
        .dynamic_background(move |state| {
            let fill = if state.hovered || state.pressed {
                theme.palette().surface_hover
            } else {
                Color::TRANSPARENT
            };
            SurfaceStyle::new(fill).radius(radius::SM)
        })
        .foreground(move |ctx, rect, _| {
            let inner = inset(rect, rect.size.width * 0.32);
            fill_rounded_rect(ctx, inner, 1.5, theme.palette().subtle);
        })
}

/// The app icon: accent square with a light inner mark.
fn app_icon(size: f32, theme: &'static dyn Theme) -> Panel {
    Panel::new()
        .color(Color::TRANSPARENT)
        .flat()
        .min_size(size, size)
        .surface(SurfaceStyle::new(theme.palette().accent).radius(radius::SM))
        .foreground(move |ctx, rect, _| {
            fill_rounded_rect(ctx, inset(rect, 6.0), 1.0, theme.palette().on_accent);
        })
}

/// A note thumbnail: bordered surface with a shaded inner rectangle.
fn thumb(size: f32, theme: &'static dyn Theme, shade: f32) -> Panel {
    Panel::new()
        .color(Color::TRANSPARENT)
        .flat()
        .min_size(size, size)
        .surface(
            SurfaceStyle::new(theme.palette().surface_raised)
                .border(theme.palette().border)
                .radius(radius::MD),
        )
        .foreground(move |ctx, rect, _| {
            fill_rounded_rect(
                ctx,
                inset(rect, 12.0),
                2.0,
                theme.palette().subtle.with_alpha(shade),
            );
        })
}

fn nav_row_view(
    theme: &'static dyn Theme,
    label: &str,
    index: usize,
    selected: &Rc<Cell<usize>>,
) -> Row {
    let held = selected.clone();
    let click = selected.clone();
    Row::new()
        .align(Align::Center)
        .gap(space::SM)
        .padding(Edges::new(space::SM, space::XS, space::SM, space::XS))
        .child(icon_box(theme, 14.0))
        .child(Text::small(label, theme))
        .min_size(0.0, 28.0)
        .dynamic_background(move |interact| {
            let fill = if held.get() == index {
                theme.palette().selection
            } else if interact.hovered {
                theme.palette().surface_hover
            } else {
                Color::TRANSPARENT
            };
            SurfaceStyle::new(fill).radius(radius::SM)
        })
        .on_click(move || click.set(index))
}

impl Sidebar {
    /// Composes the left-hand pane declaratively; the tree is not touched here.
    ///
    /// Leaf components are built first, then the column composes them once at
    /// the end.
    fn new(theme: &'static dyn Theme, state: &DemoState, handles: &Handles) -> Self {
        let header = Row::new()
            .align(Align::Center)
            .gap(space::SM)
            .child(app_icon(20.0, theme))
            .child(Text::subheading("Quill", theme));

        let search = Panel::new()
            .color(Color::TRANSPARENT)
            .flat()
            .min_size(0.0, 30.0)
            .surface(
                SurfaceStyle::new(theme.palette().surface_raised)
                    .border(theme.palette().border)
                    .radius(radius::MD),
            )
            .child(
                Label::new("Search")
                    .font_size(TextSize::Small.px())
                    .color(theme.palette().muted)
                    .text_options(TextOptions::no_wrap())
                    .anchors(Edges::new(0.0, 0.5, 1.0, 0.5))
                    .offsets(Edges::new(space::SM, -8.0, -space::SM, 8.0)),
            );

        let library = Text::caption("Library", theme).tone(Tone::Subtle);
        let tags = Text::caption("Tags", theme).tone(Tone::Subtle);

        let nav_rows = NAV_ITEMS.iter().enumerate().map(|(index, label)| {
            let rows = handles.nav_rows.clone();
            nav_row_view(theme, label, index, &state.selected_nav)
                .with_ref(move |id| rows.borrow_mut().push(id))
        });
        let tag_rows = TAG_ITEMS.iter().enumerate().map(|(index, label)| {
            let rows = handles.nav_rows.clone();
            nav_row_view(theme, label, NAV_ITEMS.len() + index, &state.selected_nav)
                .with_ref(move |id| rows.borrow_mut().push(id))
        });

        let spacer = Flex::new().padding(Edges::ZERO).grow(1.0);
        let footer = Row::new()
            .align(Align::Center)
            .gap(space::SM)
            .child(Badge::new("v0.1.0", theme).tone(Tone::Muted))
            .child(Text::caption("local", theme).tone(Tone::Subtle));

        let inner = Column::new()
            .gap(space::MD)
            .padding(Edges::new(
                space::LG,
                SIDEBAR_PADDING_TOP,
                space::MD,
                space::MD,
            ))
            .basis(SizeBasis::Px(state.sidebar_width.get()))
            .shrink(0.0)
            .surface(SurfaceStyle::new(theme.palette().surface))
            .child(header)
            .child(search)
            .child(library)
            .children(nav_rows)
            .child(tags)
            .children(tag_rows)
            .child(spacer)
            .child(footer);

        Self { inner }
    }
}

impl Component for Sidebar {
    fn spec(&mut self) -> &mut Spec {
        self.inner.spec()
    }

    fn name(&self) -> &'static str {
        "Sidebar"
    }

    fn widget(&self) -> Widget {
        self.inner.widget()
    }

    fn prepare(&mut self) {
        self.inner.prepare();
    }
}

fn note_row_view(
    theme: &'static dyn Theme,
    note: &Note,
    index: usize,
    selected: &Rc<Cell<usize>>,
) -> Row {
    let held = selected.clone();
    let click = selected.clone();
    let bar = selected.clone();
    let shade = 0.18 + index as f32 * 0.05;

    Row::new()
        .align(Align::Start)
        .gap(space::MD)
        .padding(Edges::all(space::SM))
        .child(thumb(44.0, theme, shade))
        .child(
            Column::new()
                .gap(space::XXS)
                .child(Text::small(note.title, theme))
                .child(
                    Text::caption(note.snippet, theme)
                        .tone(Tone::Muted)
                        .max_lines(1)
                        .ellipsis(true),
                )
                .grow(1.0),
        )
        .min_size(0.0, 60.0)
        .dynamic_background(move |interact| {
            let fill = if held.get() == index {
                theme.palette().selection
            } else if interact.hovered {
                theme.palette().surface_hover
            } else {
                Color::TRANSPARENT
            };
            SurfaceStyle::new(fill).corners(CornerRadii::new(0.0, radius::MD, radius::MD, 0.0))
        })
        .foreground(move |ctx, rect, _| {
            if bar.get() == index {
                let bar = Rect::from_min_max(
                    Vec2::new(rect.left(), rect.top()),
                    Vec2::new(rect.left() + 3.0, rect.bottom()),
                );
                fill_rounded_rect_corners(
                    ctx,
                    bar,
                    CornerRadii::new(0.0, 1.5, 1.5, 0.0),
                    theme.palette().accent,
                );
            }
        })
        .on_click(move || click.set(index))
}

impl NoteList {
    /// Composes the middle content list declaratively; no tree here.
    ///
    /// Leaf components are built first, then the column composes them once at
    /// the end.
    fn new(theme: &'static dyn Theme, state: &DemoState, handles: &Handles) -> Self {
        let header = Row::new()
            .align(Align::Center)
            .justify(Justify::SpaceBetween)
            .gap(space::SM)
            .padding(Edges::new(space::SM, space::XS, space::SM, space::XS))
            .min_size(0.0, 32.0)
            .child(Text::heading("All Notes", theme))
            .child(Text::small(format!("{} notes", NOTES.len()), theme).tone(Tone::Muted));

        let divider = Divider::horizontal(theme);

        let rows = NOTES.iter().enumerate().map(|(index, note)| {
            let slots = handles.list_rows.clone();
            note_row_view(theme, note, index, &state.selected)
                .with_ref(move |id| slots.borrow_mut().push(id))
        });

        let inner = Column::new()
            .gap(space::XS)
            .padding(Edges::new(space::MD, space::MD, space::MD, space::MD))
            .basis(SizeBasis::Px(state.list_width.get()))
            .shrink(0.0)
            .surface(SurfaceStyle::new(theme.palette().background))
            .child(header)
            .child(divider)
            .children(rows);

        Self { inner }
    }
}

impl Component for NoteList {
    fn spec(&mut self) -> &mut Spec {
        self.inner.spec()
    }

    fn name(&self) -> &'static str {
        "NoteList"
    }

    fn widget(&self) -> Widget {
        self.inner.widget()
    }

    fn prepare(&mut self) {
        self.inner.prepare();
    }
}

impl DetailPane {
    /// Composes the right-hand pane declaratively; no tree here.
    fn new(theme: &'static dyn Theme, state: &DemoState, handles: &Handles) -> Self {
        // Shared route for the right-hand pane: 0 = note detail, 1 = settings.
        let route = Rc::new(Cell::new(0));

        // Toolbar (view 0).
        let back = state.selected.clone();
        let forward = state.selected.clone();
        let to_settings = route.clone();
        let nav_group = Row::new()
            .align(Align::Center)
            .gap(space::XS)
            .child(icon_box(theme, 28.0).on_click(move || {
                let value = back.get();
                back.set(value.saturating_sub(1));
            }))
            .child(icon_box(theme, 28.0).on_click(move || {
                let value = forward.get();
                forward.set((value + 1).min(NOTES.len() - 1));
            }))
            // Gear placeholder: route to the settings view.
            .child(icon_box(theme, 28.0).on_click(move || to_settings.set(1)));

        let counter = state.clicks.clone();
        let actions = Row::new()
            .align(Align::Center)
            .gap(space::SM)
            .child(Button::ghost("Share", theme))
            .child(
                Button::primary("New Note", theme)
                    .on_click(move || counter.set(counter.get() + 1))
                    .ref_(&handles.primary_button),
            );

        let toolbar = Row::new()
            .align(Align::Center)
            .gap(space::SM)
            .padding(Edges::new(space::LG, space::SM, space::LG, space::SM))
            .min_size(0.0, 48.0)
            .child(nav_group)
            .child(Flex::new().padding(Edges::ZERO).grow(1.0))
            .child(actions);

        // Content (view 0).
        let note = &NOTES[0];

        let hero = Flex::new()
            .padding(Edges::ZERO)
            .min_size(0.0, 220.0)
            .surface(
                SurfaceStyle::new(theme.palette().surface_raised)
                    .border(theme.palette().border)
                    .radius(radius::LG),
            )
            .foreground(move |ctx, rect, _| {
                let side = 64.0f32
                    .min(rect.size.width - 24.0)
                    .min(rect.size.height - 24.0)
                    .max(0.0);
                if side > 0.0 {
                    let inner = Rect::from_center_size(rect.center(), Size::splat(side));
                    fill_rounded_rect(
                        ctx,
                        inner,
                        radius::MD,
                        theme.palette().subtle.with_alpha(0.18),
                    );
                }
            })
            .ref_(&handles.hero);

        let delete_flag = state.delete_requested.clone();
        let content = Column::new()
            .gap(space::LG)
            .padding(Edges::new(space::XXL, space::LG, space::XXL, space::XXL))
            .child(hero)
            .child(
                Row::new()
                    .align(Align::Center)
                    .gap(space::SM)
                    .child(
                        Text::heading(note.title, theme)
                            .grow(1.0)
                            .ref_(&handles.detail_title),
                    )
                    .child(
                        Text::caption(note.tag, theme)
                            .tone(Tone::Accent)
                            .ref_(&handles.detail_tag),
                    ),
            )
            .child(
                Text::small(format!("Edited {} · {}", note.modified, note.tag), theme)
                    .tone(Tone::Muted)
                    .ref_(&handles.detail_meta),
            )
            .child(
                Text::new(note.body, theme)
                    .tone(Tone::Muted)
                    .ref_(&handles.detail_body),
            )
            .child(
                Row::new()
                    .align(Align::Center)
                    .gap(space::XL)
                    .child(Checkbox::new("Pin note", theme))
                    .child(Switch::new(theme).label("Shared")),
            )
            .child(Divider::horizontal(theme))
            .child(
                Row::new()
                    .align(Align::Center)
                    .gap(space::SM)
                    .child(Button::secondary("Open", theme))
                    .child(Button::secondary("Duplicate", theme))
                    .child(Button::ghost("Delete", theme).on_click(move || delete_flag.set(true))),
            );

        let note_view = Column::new()
            .gap(0.0)
            .padding(Edges::ZERO)
            .child(toolbar)
            .child(Divider::horizontal(theme))
            .child(content);

        // View 1: settings.
        let to_note = route.clone();
        let settings = Column::new()
            .child(Text::heading("hello", theme))
            .gap(space::LG)
            .padding(Edges::all(space::XXL))
            .surface(SurfaceStyle::new(theme.palette().background))
            .child(Text::heading("Settings", theme))
            .child(
                Text::new(
                    "Preferences for this workspace. A different view in the same pane.",
                    theme,
                )
                .tone(Tone::Muted),
            )
            .child(Checkbox::new("Enable sync", theme))
            .child(Switch::new(theme).label("Notifications"))
            .child(Divider::horizontal(theme))
            .child(Button::secondary("Back to note", theme).on_click(move || to_note.set(0)));

        // The router container is a transparent panel that fills the split row.
        let mut spec = Spec::default();
        spec.data.layout.grow = 1.0;

        Self {
            spec,
            theme,
            route,
            note_view,
            settings,
            handles: handles.clone(),
        }
    }
}

impl Component for DetailPane {
    fn spec(&mut self) -> &mut Spec {
        &mut self.spec
    }

    fn name(&self) -> &'static str {
        "DetailPane"
    }

    fn widget(&self) -> Widget {
        Widget::Panel {
            color: self.theme.palette().background,
            border: None,
        }
    }

    /// Mounts the two views, wires the router and reports it via [`Handles`].
    fn build(mut self, tree: &mut SceneTree, parent: NodeId) -> NodeId {
        self.prepare();
        let spec = std::mem::take(self.spec());
        let root = tree.add_control(parent, self.name());
        tree.set_data(root, Control::new(spec.data, self.widget()));

        let note_view = tree.add_child(root, self.note_view);
        let settings = tree.add_child(root, self.settings);

        let mut router = Router::with_route(root, self.route.clone());
        router.add_node(note_view);
        router.add_node(settings);
        router.sync(tree);
        self.handles.router.borrow_mut().replace(router);

        draw_components::apply_spec(tree, root, spec);
        root
    }
}

impl SceneChild for Sidebar {
    fn attach(self, tree: &mut SceneTree, parent: NodeId) -> NodeId {
        <Self as Component>::build(self, tree, parent)
    }
}

impl SceneChild for NoteList {
    fn attach(self, tree: &mut SceneTree, parent: NodeId) -> NodeId {
        <Self as Component>::build(self, tree, parent)
    }
}

impl SceneChild for DetailPane {
    fn attach(self, tree: &mut SceneTree, parent: NodeId) -> NodeId {
        <Self as Component>::build(self, tree, parent)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use draw_backend_recording::RecordingBackend;
    use draw_core::PointerButton;
    use draw_render::{DrawCommand, RenderBackend};

    fn laid_out(width: f32, height: f32) -> DemoApp {
        let viewport = ViewportSize::new(Size::new(width, height));
        let mut app = DemoApp::new();
        app.update(viewport, 0.016);
        app.layout(viewport);
        app
    }

    fn rect(app: &DemoApp, id: NodeId) -> Rect {
        draw_ui::control(app.tree(), id).expect("control").rect
    }

    fn click(app: &mut DemoApp, position: Vec2) {
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
    fn pointer_over_clickable_tracks_hover() {
        let mut app = laid_out(1100.0, 720.0);
        assert!(!app.pointer_over_clickable());

        let center = app.button_center().expect("button rect");
        app.event(&InputEvent::PointerMove { position: center });
        assert!(app.pointer_over_clickable());

        // A blank spot in the detail background is not clickable.
        app.event(&InputEvent::PointerMove {
            position: Vec2::new(1000.0, 300.0),
        });
        assert!(!app.pointer_over_clickable());
    }

    #[test]
    fn note_rows_fit_within_the_list_column() {
        let app = laid_out(1100.0, 720.0);
        let list = rect(&app, app.list());
        for &row in app.list_rows() {
            let item = rect(&app, row);
            assert!(
                item.left() >= list.left() - 0.5 && item.right() <= list.right() + 0.5,
                "row {item:?} escapes list {list:?}"
            );
        }
    }

    #[test]
    fn note_rows_fit_with_a_wide_measurer() {
        let viewport = ViewportSize::new(Size::new(1100.0, 720.0));
        let mut app = DemoApp::new();
        app.set_text_measurer(std::rc::Rc::new(draw_ui::FixedWidthTextMeasurer::default()));
        app.update(viewport, 0.016);
        app.layout(viewport);
        let list = rect(&app, app.list());
        for &row in app.list_rows() {
            let item = rect(&app, row);
            assert!(
                item.left() >= list.left() - 0.5 && item.right() <= list.right() + 0.5,
                "row {item:?} escapes list {list:?}"
            );
        }
    }

    #[test]
    fn titlebar_inset_pads_the_sidebar_only() {
        let mut app = laid_out(1100.0, 720.0);
        assert_eq!(app.titlebar_inset(), 0.0);

        let list_row = app.list_rows()[0];
        let list_top_before = rect(&app, list_row).top();

        app.set_titlebar_inset(28.0);
        app.layout(app.viewport());
        assert_eq!(app.titlebar_inset(), 28.0);

        // The sidebar's own padding grew by the safe area...
        let draw_ui::Widget::Flex(flex) =
            draw_ui::widget(app.tree(), app.sidebar()).expect("sidebar")
        else {
            panic!("sidebar is a flex container");
        };
        assert!((flex.padding.top - (space::MD + 28.0)).abs() < 1e-3);

        // ...while the other panes keep their top edge.
        assert!((rect(&app, list_row).top() - list_top_before).abs() < 1e-3);

        // Setting it to zero again restores the base padding.
        app.set_titlebar_inset(0.0);
        app.layout(app.viewport());
        let draw_ui::Widget::Flex(flex) =
            draw_ui::widget(app.tree(), app.sidebar()).expect("sidebar")
        else {
            panic!("sidebar is a flex container");
        };
        assert!((flex.padding.top - space::MD).abs() < 1e-3);
    }

    #[test]
    fn detail_router_switches_between_note_and_settings() {
        let mut app = laid_out(1100.0, 720.0);
        let note = app.detail_router().view(0).unwrap();
        let settings = app.detail_router().view(1).unwrap();

        assert_eq!(app.detail_route(), 0);
        assert_eq!(app.tree().is_visible(note), Some(true));
        assert_eq!(app.tree().is_visible(settings), Some(false));
        // The inactive view reserves no layout space.
        assert_eq!(rect(&app, settings), Rect::ZERO);

        app.show_detail_route(1);
        app.update(app.viewport(), 0.016);
        app.layout(app.viewport());

        assert_eq!(app.detail_route(), 1);
        assert_eq!(app.tree().is_visible(note), Some(false));
        assert_eq!(app.tree().is_visible(settings), Some(true));

        // The active view fills the pane; the hidden one has no rect.
        let detail = rect(&app, app.detail());
        let settings_rect = rect(&app, settings);
        assert!((settings_rect.size.width - detail.size.width).abs() < 1.0);
        assert!((settings_rect.size.height - detail.size.height).abs() < 1.0);
        assert_eq!(rect(&app, note), Rect::ZERO);

        // Switching back restores the note view.
        app.show_detail_route(0);
        app.update(app.viewport(), 0.016);
        app.layout(app.viewport());
        assert_eq!(app.tree().is_visible(note), Some(true));
        assert_eq!(rect(&app, settings), Rect::ZERO);
    }

    #[test]
    fn three_columns_are_side_by_side() {
        let app = laid_out(1100.0, 720.0);
        let sidebar = rect(&app, app.sidebar());
        let list = rect(&app, app.list());
        let detail = rect(&app, app.detail());

        assert!((sidebar.size.width - SIDEBAR_WIDTH).abs() < 1e-3);
        assert!((list.size.width - LIST_WIDTH).abs() < 1e-3);
        assert!(detail.size.width > 0.0);

        assert!((sidebar.left()).abs() < 1e-3);
        assert!((list.left() - (SIDEBAR_WIDTH + RESIZE_GUTTER)).abs() < 1e-3);
        assert!((detail.left() - DETAIL_X).abs() < 1e-3);

        // Columns tile left-to-right without overlap.
        assert!(sidebar.right() <= list.left() + 1.0);
        assert!(list.right() <= detail.left() + 1.0);

        // All columns span the full height.
        assert!((sidebar.size.height - 720.0).abs() < 1e-3);
        assert!((list.size.height - 720.0).abs() < 1e-3);
        assert!((detail.size.height - 720.0).abs() < 1e-3);
    }

    #[test]
    fn columns_resize_with_the_viewport() {
        let mut app = laid_out(1100.0, 720.0);
        let detail_before = rect(&app, app.detail()).size.width;

        let wide = ViewportSize::new(Size::new(1400.0, 800.0));
        app.update(wide, 0.016);
        app.layout(wide);

        let sidebar = rect(&app, app.sidebar());
        let list = rect(&app, app.list());
        let detail = rect(&app, app.detail());
        assert!((sidebar.size.width - SIDEBAR_WIDTH).abs() < 1e-3);
        assert!((list.size.width - LIST_WIDTH).abs() < 1e-3);
        assert!(detail.size.width > detail_before);
        assert!((detail.right() - 1400.0).abs() < 1e-3);
    }

    #[test]
    fn dragging_the_sidebar_gutter_resizes_the_sidebar() {
        let mut app = laid_out(1100.0, 720.0);
        let viewport = app.viewport();
        assert!((app.sidebar_width() - SIDEBAR_WIDTH).abs() < 1e-3);

        let gutter = |app: &DemoApp| Vec2::new(app.sidebar_width() + RESIZE_GUTTER / 2.0, 360.0);

        // Drag right: the sidebar grows and the flex row re-adapts the rest.
        let start = gutter(&app);
        app.event(&InputEvent::PointerDown {
            position: start,
            button: PointerButton::Left,
        });
        app.event(&InputEvent::PointerMove {
            position: start + Vec2::new(40.0, 0.0),
        });
        // The component swaps its own cursor while the drag is active.
        assert_eq!(app.cursor(), Cursor::Grabbing);
        app.event(&InputEvent::PointerUp {
            position: start + Vec2::new(40.0, 0.0),
            button: PointerButton::Left,
        });
        app.layout(viewport);

        assert_eq!(app.cursor(), Cursor::ColResize);
        assert!((app.sidebar_width() - (SIDEBAR_WIDTH + 40.0)).abs() < 1e-3);
        let sidebar = rect(&app, app.sidebar());
        let list = rect(&app, app.list());
        assert!((sidebar.size.width - (SIDEBAR_WIDTH + 40.0)).abs() < 1e-3);
        assert!((list.left() - (SIDEBAR_WIDTH + 40.0 + RESIZE_GUTTER)).abs() < 1e-3);

        // Drag far left: the sidebar clamps at its minimum.
        let start = gutter(&app);
        app.event(&InputEvent::PointerDown {
            position: start,
            button: PointerButton::Left,
        });
        app.event(&InputEvent::PointerMove {
            position: start - Vec2::new(1000.0, 0.0),
        });
        app.event(&InputEvent::PointerUp {
            position: start - Vec2::new(1000.0, 0.0),
            button: PointerButton::Left,
        });
        assert!((app.sidebar_width() - SIDEBAR_MIN).abs() < 1e-3);
    }

    #[test]
    fn dragging_the_list_gutter_resizes_the_list() {
        let mut app = laid_out(1100.0, 720.0);
        let viewport = app.viewport();
        assert!((app.list_width() - LIST_WIDTH).abs() < 1e-3);

        let start = Vec2::new(
            app.sidebar_width() + RESIZE_GUTTER + app.list_width() + RESIZE_GUTTER / 2.0,
            360.0,
        );
        app.event(&InputEvent::PointerDown {
            position: start,
            button: PointerButton::Left,
        });
        app.event(&InputEvent::PointerMove {
            position: start + Vec2::new(50.0, 0.0),
        });
        app.event(&InputEvent::PointerUp {
            position: start + Vec2::new(50.0, 0.0),
            button: PointerButton::Left,
        });
        app.layout(viewport);

        assert!((app.list_width() - (LIST_WIDTH + 50.0)).abs() < 1e-3);
        let detail = rect(&app, app.detail());
        let expected = SIDEBAR_WIDTH + RESIZE_GUTTER + LIST_WIDTH + 50.0 + RESIZE_GUTTER;
        assert!((detail.left() - expected).abs() < 1e-3);
    }

    #[test]
    fn hovering_a_gutter_reports_a_resize_cursor() {
        let mut app = laid_out(1100.0, 720.0);
        assert_eq!(app.cursor(), Cursor::Default);

        let sidebar_gutter = Vec2::new(app.sidebar_width() + RESIZE_GUTTER / 2.0, 360.0);
        app.event(&InputEvent::PointerMove {
            position: sidebar_gutter,
        });
        assert_eq!(app.cursor(), Cursor::ColResize);

        let list_gutter = Vec2::new(
            app.sidebar_width() + RESIZE_GUTTER + app.list_width() + RESIZE_GUTTER / 2.0,
            360.0,
        );
        app.event(&InputEvent::PointerMove {
            position: list_gutter,
        });
        assert_eq!(app.cursor(), Cursor::ColResize);

        let button = app.button_center().expect("button rect");
        app.event(&InputEvent::PointerMove { position: button });
        assert_eq!(app.cursor(), Cursor::Pointer);
    }

    #[test]
    fn clicking_a_note_selects_it_and_updates_the_detail() {
        let mut app = laid_out(1100.0, 720.0);
        assert_eq!(app.selected(), 0);

        let target = app.list_rows()[3];
        let center = rect(&app, target).center();
        click(&mut app, center);
        assert_eq!(app.selected(), 3);

        // State -> text is applied in update().
        let viewport = app.viewport();
        app.update(viewport, 0.016);
        assert_eq!(app.detail_title_text(), Some(NOTES[3].title));
        assert_eq!(app.detail_body_text(), Some(NOTES[3].body));
    }

    #[test]
    fn clicking_a_nav_row_updates_sidebar_selection() {
        let mut app = laid_out(1100.0, 720.0);
        let target = app.nav_rows()[2];
        let center = rect(&app, target).center();
        click(&mut app, center);
        assert_eq!(app.selected_nav(), 2);
    }

    #[test]
    fn primary_button_fires_and_reports_center() {
        let mut app = laid_out(1100.0, 720.0);
        let center = app.button_center().expect("button rect");
        click(&mut app, center);
        assert_eq!(app.clicks(), 1);
    }

    #[test]
    fn detail_children_stay_inside_the_detail_pane() {
        let app = laid_out(1100.0, 720.0);
        let detail = rect(&app, app.detail());
        assert!(detail.contains_rect(rect(&app, app.hero())));
        for row in app.list_rows() {
            assert!(rect(&app, app.list()).contains_rect(rect(&app, *row)));
        }
        for row in app.nav_rows() {
            assert!(rect(&app, app.sidebar()).contains_rect(rect(&app, *row)));
        }
    }

    #[test]
    fn selected_note_row_has_square_left_round_right_and_a_full_height_bar() {
        let app = laid_out(1100.0, 720.0);
        let row = app.list_rows()[0];
        let row_rect = rect(&app, row);

        let mut ctx = PaintContext::new();
        draw_ui::paint(app.tree(), &mut ctx);
        let list = ctx.into_draw_list();

        // The selection surface is square on the left and rounded on the right.
        let corners = list
            .commands()
            .iter()
            .find_map(|c| match c {
                DrawCommand::FillRoundedRect { rect, corners, .. } if *rect == row_rect => {
                    Some(*corners)
                }
                _ => None,
            })
            .expect("selected row surface");
        assert_eq!(corners.top_left, 0.0);
        assert_eq!(corners.bottom_left, 0.0);
        assert!(corners.top_right > 0.0 && corners.bottom_right > 0.0);

        // The accent bar spans the full item height.
        let bar = list
            .commands()
            .iter()
            .find_map(|c| match c {
                DrawCommand::FillRoundedRect {
                    rect,
                    corners,
                    paint,
                } if paint.color == app.theme.palette().accent
                    && (rect.size.height - row_rect.size.height).abs() < 1e-3
                    && row_rect.contains_rect(*rect) =>
                {
                    Some(*corners)
                }
                _ => None,
            })
            .expect("full-height accent bar");
        assert_eq!(bar.top_left, 0.0);
        assert_eq!(bar.bottom_left, 0.0);
    }

    #[test]
    fn list_header_does_not_wrap() {
        let app = laid_out(1100.0, 720.0);
        let mut ctx = PaintContext::new();
        app.paint(&mut ctx);
        let list = ctx.into_draw_list();
        let heading = TextSize::Heading.px();
        let painted = list.commands().iter().any(|c| {
            matches!(c, DrawCommand::DrawText { text, font_size, .. }
                if text == "All Notes" && (*font_size - heading).abs() < 1e-3)
        });
        assert!(painted, "the 'All Notes' heading must stay on one line");
        let wrapped = list.commands().iter().any(|c| {
            matches!(c, DrawCommand::DrawText { text, font_size, .. }
                if (text == "All" || text == "Notes") && (*font_size - heading).abs() < 1e-3)
        });
        assert!(!wrapped, "the heading wrapped at the space");
    }

    #[test]
    fn full_pipeline_records_a_draw_list_headlessly() {
        let app = laid_out(1100.0, 720.0);
        let viewport = app.viewport();

        let mut ctx = PaintContext::new();
        app.paint(&mut ctx);
        let list = ctx.into_draw_list();

        let mut backend = RecordingBackend::new();
        backend.begin_frame(viewport).unwrap();
        backend.submit(&list).unwrap();
        backend.end_frame().unwrap();

        assert_eq!(backend.frame_count(), 1);
        let commands = backend.last_frame().expect("frame").commands();
        assert!(commands
            .iter()
            .any(|c| matches!(c, DrawCommand::FillRect { .. })));
        assert!(commands
            .iter()
            .any(|c| matches!(c, DrawCommand::FillCircle { .. })));
        assert!(commands
            .iter()
            .any(|c| matches!(c, DrawCommand::DrawText { .. })));
        assert!(commands
            .iter()
            .any(|c| matches!(c, DrawCommand::FillRoundedRect { .. })));
    }
}
