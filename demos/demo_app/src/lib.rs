//! Shared, backend-neutral demo application: a three-column, macOS-style notes
//! app built from `draw_components` components on the frozen `draw_ui` core.
//!
//! Layout is the classic macOS split view:
//!
//! ```text
//! ┌──────────┬────────────────┬──────────────────────────────┐
//! │ sidebar  │  content list  │  detail                      │
//! │ 220px    │  324px         │  fills the rest              │
//! │ traffic  │  header        │  toolbar / hero / body       │
//! │ nav      │  note rows     │  actions                     │
//! └──────────┴────────────────┴──────────────────────────────┘
//! ```
//!
//! Icons and images are monochrome rounded squares (placeholders). The detail
//! hero renders a static image placeholder (monochrome rounded square).
//!
//! The app is intentionally **static**: nothing animates, so a host can profile
//! the idle cost (layout caching, paint and GPU submit) without animation noise.
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

use draw_components::{Badge, Button, Checkbox, Divider, Overlays, Switch, Text};
use draw_core::{Color, Edges, EventResult, InputEvent, NodeId, Rect, Size, Vec2, Viewport};
use draw_render::{CornerRadii, PaintContext};
use draw_theme::{radius, space, TextSize, Theme};
use draw_ui::{
    fill_rounded_rect, fill_rounded_rect_corners, inset, Align, Column, Flex, Justify, Label,
    Panel, Row, SurfaceStyle, TextMeasurer, TextOptions, Tone, Ui, View, ViewExt,
};

/// Sidebar width in logical pixels.
pub const SIDEBAR_WIDTH: f32 = 220.0;
/// Content-list width in logical pixels.
pub const LIST_WIDTH: f32 = 324.0;
/// X offset where the detail pane starts.
pub const DETAIL_X: f32 = SIDEBAR_WIDTH + LIST_WIDTH;

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

/// Application state shared by every demo host.
pub struct DemoApp {
    ui: Ui,
    theme: Theme,
    sidebar: NodeId,
    list: NodeId,
    detail: NodeId,
    hero: NodeId,
    list_rows: Vec<NodeId>,
    nav_rows: Vec<NodeId>,
    detail_title: NodeId,
    detail_body: NodeId,
    detail_tag: NodeId,
    detail_meta: NodeId,
    primary_button: NodeId,
    selected: Rc<Cell<usize>>,
    selected_nav: Rc<Cell<usize>>,
    clicks: Rc<Cell<u32>>,
    overlays: Overlays,
    delete_requested: Rc<Cell<bool>>,
    deleted: Rc<Cell<bool>>,
    viewport: Viewport,
}

impl Default for DemoApp {
    fn default() -> Self {
        Self::new()
    }
}

impl DemoApp {
    /// Builds the app with the dark theme.
    pub fn new() -> Self {
        Self::with_theme(Theme::dark())
    }

    /// Builds the app with an explicit theme.
    pub fn with_theme(theme: Theme) -> Self {
        let mut ui = Ui::new();
        ui.set_theme(theme);
        let root = ui.root();

        let selected = Rc::new(Cell::new(0));
        let selected_nav = Rc::new(Cell::new(0));
        let delete_requested = Rc::new(Cell::new(false));
        let clicks = Rc::new(Cell::new(0));
        let ids = Rc::new(RefCell::new(Ids::default()));

        // Three columns plus their hairline separators, all declarative.
        ui.mount(root, sidebar_view(theme, &selected_nav, &ids));
        ui.mount(root, list_view(theme, &selected, &ids));
        ui.mount(
            root,
            detail_view(theme, &selected, &clicks, &delete_requested, &ids),
        );
        for x in [SIDEBAR_WIDTH, DETAIL_X] {
            ui.mount(root, separator_view(theme, x));
        }

        let ids = ids.borrow();
        Self {
            ui,
            theme,
            sidebar: ids.sidebar.expect("sidebar node"),
            list: ids.list.expect("list node"),
            detail: ids.detail.expect("detail node"),
            hero: ids.hero.expect("hero node"),
            list_rows: ids.list_rows.clone(),
            nav_rows: ids.nav_rows.clone(),
            detail_title: ids.detail_title.expect("detail title node"),
            detail_body: ids.detail_body.expect("detail body node"),
            detail_tag: ids.detail_tag.expect("detail tag node"),
            detail_meta: ids.detail_meta.expect("detail meta node"),
            primary_button: ids.primary_button.expect("primary button node"),
            selected,
            selected_nav,
            clicks,
            overlays: Overlays::new(theme),
            delete_requested,
            deleted: Rc::new(Cell::new(false)),
            viewport: Viewport::new(Size::new(1100.0, 720.0)),
        }
    }

    // -- accessors ---------------------------------------------------------

    pub fn ui(&self) -> &Ui {
        &self.ui
    }

    /// Mutable UI access, e.g. to inject a text measurer.
    pub fn ui_mut(&mut self) -> &mut Ui {
        &mut self.ui
    }

    /// Installs `measurer` for both the main UI and the overlay layer.
    pub fn set_text_measurer(&mut self, measurer: Rc<dyn TextMeasurer>) {
        self.ui.set_text_measurer(measurer.clone());
        self.overlays.set_text_measurer(measurer);
    }

    pub fn overlays(&self) -> &Overlays {
        &self.overlays
    }

    pub fn theme(&self) -> &Theme {
        &self.theme
    }

    pub fn viewport(&self) -> Viewport {
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
        self.selected.get()
    }

    pub fn selected_nav(&self) -> usize {
        self.selected_nav.get()
    }

    /// Number of times the primary ("New Note") button has been clicked.
    pub fn clicks(&self) -> u32 {
        self.clicks.get()
    }

    /// Center of the primary button in logical viewport coordinates.
    pub fn button_center(&self) -> Option<Vec2> {
        self.ui
            .control(self.primary_button)
            .map(|control| control.rect.center())
    }

    /// Text of the detail title label (used by tests).
    pub fn detail_title_text(&self) -> Option<&str> {
        self.ui
            .widget(self.detail_title)
            .and_then(|widget| widget.text())
    }

    /// Text of the list-item count badge label.
    pub fn detail_body_text(&self) -> Option<&str> {
        self.ui
            .widget(self.detail_body)
            .and_then(|widget| widget.text())
    }

    // -- pipeline ----------------------------------------------------------

    /// Updates state-driven text and overlay timers.
    pub fn update(&mut self, viewport: Viewport, dt: f32) {
        self.viewport = viewport;
        self.overlays.update(dt);

        if self.delete_requested.replace(false) {
            let deleted = self.deleted.clone();
            let id = self
                .overlays
                .confirm("Delete note?", "This cannot be undone.");
            self.overlays
                .confirm_label(id, "Delete")
                .destructive(id, true)
                .on_confirm(id, move || deleted.set(true));
        }
        if self.deleted.replace(false) {
            self.overlays.message_tone("Note deleted", Tone::Success);
        }

        let index = self.selected.get().min(NOTES.len() - 1);
        let note = &NOTES[index];
        self.ui.set_text(self.detail_title, note.title);
        self.ui.set_text(self.detail_body, note.body);
        self.ui.set_text(self.detail_tag, note.tag);
        self.ui.set_text(
            self.detail_meta,
            format!("Edited {} · {}", note.modified, note.tag),
        );
    }

    /// Resolves UI layout for `viewport`, then positions the overlays.
    pub fn layout(&mut self, viewport: Viewport) {
        self.viewport = viewport;
        self.ui.layout(viewport);
        self.overlays.layout(&self.ui, viewport);
    }

    /// Emits this frame's `DrawList` into `ctx`.
    ///
    /// Order: window background, kit surfaces, UI content, kit foregrounds
    /// (indicators, icons, the hero image placeholder), then overlays.
    pub fn paint(&self, ctx: &mut PaintContext) {
        let size = self.viewport.logical_size();
        ctx.fill_rect(
            Rect::from_min_size(Vec2::ZERO, size),
            self.theme.palette.background,
        );

        self.ui.paint(ctx);
        self.overlays.paint(ctx);
    }

    /// Routes an event to the overlays first, then kit interactions, then UI.
    pub fn event(&mut self, event: &InputEvent) -> EventResult {
        if self.overlays.handle_input(event).is_handled() {
            return EventResult::Handled;
        }
        self.ui.handle_input(event)
    }

    /// Controls in the demo UI.
    pub fn control_count(&self) -> usize {
        self.ui.control_count()
    }

    /// Whether the pointer is over anything clickable (a themed component or
    /// core button). Hosts use this for cursor feedback.
    pub fn pointer_over_clickable(&self) -> bool {
        self.ui
            .hovered()
            .is_some_and(|id| self.ui.is_interactive(id))
    }
}

/// Adds a compact rounded-square icon/thumbnail placeholder.
/// Node ids captured while the view tree is built.
#[derive(Default)]
struct Ids {
    sidebar: Option<NodeId>,
    list: Option<NodeId>,
    detail: Option<NodeId>,
    hero: Option<NodeId>,
    detail_title: Option<NodeId>,
    detail_body: Option<NodeId>,
    detail_tag: Option<NodeId>,
    detail_meta: Option<NodeId>,
    primary_button: Option<NodeId>,
    list_rows: Vec<NodeId>,
    nav_rows: Vec<NodeId>,
}

/// A compact square placeholder: hover surface + a small inner mark.
fn icon_box(size: f32) -> impl View {
    Panel::new()
        .color(Color::TRANSPARENT)
        .flat()
        .min_size(size, size)
        .dynamic_background(|theme, state| {
            let fill = if state.hovered || state.pressed {
                theme.palette.surface_hover
            } else {
                Color::TRANSPARENT
            };
            SurfaceStyle::new(fill).radius(radius::SM)
        })
        .foreground(|ctx, rect, theme, _| {
            let inner = inset(rect, rect.size.width * 0.32);
            fill_rounded_rect(ctx, inner, 1.5, theme.palette.subtle);
        })
}

/// The app icon: accent square with a light inner mark.
fn app_icon(size: f32, theme: Theme) -> impl View {
    Panel::new()
        .color(Color::TRANSPARENT)
        .flat()
        .min_size(size, size)
        .background(SurfaceStyle::new(theme.palette.accent).radius(radius::SM))
        .foreground(|ctx, rect, theme, _| {
            fill_rounded_rect(ctx, inset(rect, 6.0), 1.0, theme.palette.on_accent);
        })
}

/// A note thumbnail: bordered surface with a shaded inner rectangle.
fn thumb(size: f32, theme: Theme, shade: f32) -> impl View {
    Panel::new()
        .color(Color::TRANSPARENT)
        .flat()
        .min_size(size, size)
        .background(
            SurfaceStyle::new(theme.palette.surface_raised)
                .border(theme.palette.border)
                .radius(radius::MD),
        )
        .foreground(move |ctx, rect, theme, _| {
            fill_rounded_rect(
                ctx,
                inset(rect, 12.0),
                2.0,
                theme.palette.subtle.with_alpha(shade),
            );
        })
}

fn separator_view(theme: Theme, x: f32) -> impl View {
    Panel::new()
        .color(theme.palette.border_subtle)
        .flat()
        .anchors(Edges::new(0.0, 0.0, 0.0, 1.0))
        .offsets(Edges::new(x, 0.0, x + 1.0, 0.0))
}

fn nav_row_view(
    label: &str,
    index: usize,
    selected: &Rc<Cell<usize>>,
    ids: &Rc<RefCell<Ids>>,
) -> impl View {
    let held = selected.clone();
    let current = index;
    let click = selected.clone();
    let row_ids = ids.clone();
    Row::new()
        .align(Align::Center)
        .gap(space::SM)
        .padding(Edges::new(space::SM, space::XS, space::SM, space::XS))
        .child(icon_box(14.0))
        .child(Text::small(label))
        .min_size(0.0, 28.0)
        .dynamic_background(move |theme, interact| {
            let fill = if held.get() == current {
                theme.palette.selection
            } else if interact.hovered {
                theme.palette.surface_hover
            } else {
                Color::TRANSPARENT
            };
            SurfaceStyle::new(fill).radius(radius::SM)
        })
        .on_click(move || click.set(index))
        .capture(move |node| row_ids.borrow_mut().nav_rows.push(node))
}

fn sidebar_view(theme: Theme, selected_nav: &Rc<Cell<usize>>, ids: &Rc<RefCell<Ids>>) -> impl View {
    let traffic =
        Row::new()
            .gap(space::XS)
            .min_size(0.0, 12.0)
            .foreground(|ctx, rect, theme, _| {
                let radius = 5.0;
                let step = radius * 2.0 + 6.0;
                let y = rect.center().y;
                for (index, color) in [
                    theme.palette.error,
                    theme.palette.warning,
                    theme.palette.success,
                ]
                .into_iter()
                .enumerate()
                {
                    ctx.fill_circle(
                        Vec2::new(rect.left() + radius + index as f32 * step, y),
                        radius,
                        color,
                    );
                }
            });

    let title_row = Row::new()
        .align(Align::Center)
        .gap(space::SM)
        .child(app_icon(20.0, theme))
        .child(Text::subheading("Quill"));

    let search_label = Label::new("Search")
        .font_size(TextSize::Small.px())
        .color(theme.palette.muted)
        .text_options(TextOptions::no_wrap())
        .anchors(Edges::new(0.0, 0.5, 1.0, 0.5))
        .offsets(Edges::new(space::SM, -8.0, -space::SM, 8.0));
    let search = Panel::new()
        .color(Color::TRANSPARENT)
        .flat()
        .child(search_label)
        .min_size(0.0, 30.0)
        .background(
            SurfaceStyle::new(theme.palette.surface_raised)
                .border(theme.palette.border)
                .radius(radius::MD),
        );

    let library_rows: Vec<_> = NAV_ITEMS
        .iter()
        .enumerate()
        .map(|(index, label)| nav_row_view(label, index, selected_nav, ids))
        .collect();
    let tag_rows: Vec<_> = TAG_ITEMS
        .iter()
        .enumerate()
        .map(|(index, label)| nav_row_view(label, NAV_ITEMS.len() + index, selected_nav, ids))
        .collect();

    let footer = Row::new()
        .align(Align::Center)
        .gap(space::SM)
        .child(Badge::new("v0.1.0").tone(Tone::Muted))
        .child(Text::caption("local").tone(Tone::Subtle));

    let sidebar_ids = ids.clone();
    Column::new()
        .gap(space::MD)
        .padding(Edges::new(space::LG, space::MD, space::MD, space::MD))
        .child(traffic)
        .child(title_row)
        .child(search)
        .child(Text::caption("Library").tone(Tone::Subtle))
        .children(library_rows)
        .child(Text::caption("Tags").tone(Tone::Subtle))
        .children(tag_rows)
        .child(Flex::new().padding(Edges::ZERO).grow(1.0))
        .child(footer)
        .anchors(Edges::new(0.0, 0.0, 0.0, 1.0))
        .offsets(Edges::new(0.0, 0.0, SIDEBAR_WIDTH, 0.0))
        .background(SurfaceStyle::new(theme.palette.surface))
        .capture(move |node| sidebar_ids.borrow_mut().sidebar = Some(node))
}

fn note_row_view(
    theme: Theme,
    note: &Note,
    index: usize,
    selected: &Rc<Cell<usize>>,
    ids: &Rc<RefCell<Ids>>,
) -> impl View {
    let held = selected.clone();
    let current = index;
    let click = selected.clone();
    let bar = selected.clone();
    let row_ids = ids.clone();
    let shade = 0.18 + index as f32 * 0.05;
    let column = Column::new()
        .gap(space::XXS)
        .child(Text::small(note.title))
        .child(
            Text::caption(note.snippet)
                .tone(Tone::Muted)
                .max_lines(1)
                .ellipsis(true),
        )
        .grow(1.0);

    Row::new()
        .align(Align::Start)
        .gap(space::MD)
        .padding(Edges::all(space::SM))
        .child(thumb(44.0, theme, shade))
        .child(column)
        .min_size(0.0, 60.0)
        .dynamic_background(move |theme, interact| {
            let fill = if held.get() == current {
                theme.palette.selection
            } else if interact.hovered {
                theme.palette.surface_hover
            } else {
                Color::TRANSPARENT
            };
            SurfaceStyle::new(fill).corners(CornerRadii::new(0.0, radius::MD, radius::MD, 0.0))
        })
        .foreground(move |ctx, rect, theme, _| {
            if bar.get() == current {
                let bar = Rect::from_min_max(
                    Vec2::new(rect.left(), rect.top()),
                    Vec2::new(rect.left() + 3.0, rect.bottom()),
                );
                fill_rounded_rect_corners(
                    ctx,
                    bar,
                    CornerRadii::new(0.0, 1.5, 1.5, 0.0),
                    theme.palette.accent,
                );
            }
        })
        .on_click(move || click.set(index))
        .capture(move |node| row_ids.borrow_mut().list_rows.push(node))
}

fn list_view(theme: Theme, selected: &Rc<Cell<usize>>, ids: &Rc<RefCell<Ids>>) -> impl View {
    let header = Row::new()
        .align(Align::Center)
        .justify(Justify::SpaceBetween)
        .gap(space::SM)
        .padding(Edges::new(space::SM, space::XS, space::SM, space::XS))
        .child(Text::heading("All Notes"))
        .child(Text::small(format!("{} notes", NOTES.len())).tone(Tone::Muted))
        .min_size(0.0, 32.0);

    let rows: Vec<_> = NOTES
        .iter()
        .enumerate()
        .map(|(index, note)| note_row_view(theme, note, index, selected, ids))
        .collect();

    let list_ids = ids.clone();
    Column::new()
        .gap(space::XS)
        .padding(Edges::new(space::MD, space::MD, space::MD, space::MD))
        .child(header)
        .child(Divider::horizontal())
        .children(rows)
        .anchors(Edges::new(0.0, 0.0, 0.0, 1.0))
        .offsets(Edges::new(SIDEBAR_WIDTH, 0.0, DETAIL_X, 0.0))
        .background(SurfaceStyle::new(theme.palette.background))
        .capture(move |node| list_ids.borrow_mut().list = Some(node))
}

fn detail_view(
    theme: Theme,
    selected: &Rc<Cell<usize>>,
    clicks: &Rc<Cell<u32>>,
    delete_requested: &Rc<Cell<bool>>,
    ids: &Rc<RefCell<Ids>>,
) -> impl View {
    let back = selected.clone();
    let forward = selected.clone();
    let nav_group = Row::new()
        .align(Align::Center)
        .gap(space::XS)
        .child(icon_box(28.0).on_click(move || {
            let value = back.get();
            back.set(value.saturating_sub(1));
        }))
        .child(icon_box(28.0).on_click(move || {
            let value = forward.get();
            forward.set((value + 1).min(NOTES.len() - 1));
        }));

    let counter = clicks.clone();
    let primary_ids = ids.clone();
    let primary = Button::primary("New Note")
        .on_click(move || counter.set(counter.get() + 1))
        .capture(move |node| primary_ids.borrow_mut().primary_button = Some(node));
    let actions = Row::new()
        .align(Align::Center)
        .gap(space::SM)
        .child(Button::ghost("Share"))
        .child(primary);

    let toolbar = Row::new()
        .align(Align::Center)
        .gap(space::SM)
        .padding(Edges::new(space::LG, space::SM, space::LG, space::SM))
        .child(nav_group)
        .child(Flex::new().padding(Edges::ZERO).grow(1.0))
        .child(actions)
        .min_size(0.0, 48.0);

    let note = &NOTES[0];
    let hero_ids = ids.clone();
    let hero = Flex::new()
        .padding(Edges::ZERO)
        .min_size(0.0, 220.0)
        .background(
            SurfaceStyle::new(theme.palette.surface_raised)
                .border(theme.palette.border)
                .radius(radius::LG),
        )
        .foreground(|ctx, rect, theme, _| {
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
                    theme.palette.subtle.with_alpha(0.18),
                );
            }
        })
        .capture(move |node| hero_ids.borrow_mut().hero = Some(node));

    let title_ids = ids.clone();
    let detail_title = Text::heading(note.title)
        .grow(1.0)
        .capture(move |node| title_ids.borrow_mut().detail_title = Some(node));
    let tag_ids = ids.clone();
    let detail_tag = Text::caption(note.tag)
        .tone(Tone::Accent)
        .capture(move |node| tag_ids.borrow_mut().detail_tag = Some(node));
    let title_row = Row::new()
        .align(Align::Center)
        .gap(space::SM)
        .child(detail_title)
        .child(detail_tag);

    let meta_ids = ids.clone();
    let detail_meta = Text::small(format!("Edited {} · {}", note.modified, note.tag))
        .tone(Tone::Muted)
        .capture(move |node| meta_ids.borrow_mut().detail_meta = Some(node));
    let body_ids = ids.clone();
    let detail_body = Text::new(note.body)
        .tone(Tone::Muted)
        .capture(move |node| body_ids.borrow_mut().detail_body = Some(node));

    let preferences = Row::new()
        .align(Align::Center)
        .gap(space::XL)
        .child(Checkbox::new("Pin note"))
        .child(Switch::new().label("Shared"));

    let delete_flag = delete_requested.clone();
    let content_actions = Row::new()
        .align(Align::Center)
        .gap(space::SM)
        .child(Button::secondary("Open"))
        .child(Button::secondary("Duplicate"))
        .child(Button::ghost("Delete").on_click(move || delete_flag.set(true)));

    let content = Column::new()
        .gap(space::LG)
        .padding(Edges::new(space::XXL, space::LG, space::XXL, space::XXL))
        .child(hero)
        .child(title_row)
        .child(detail_meta)
        .child(detail_body)
        .child(preferences)
        .child(Divider::horizontal())
        .child(content_actions);

    let detail_ids = ids.clone();
    Column::new()
        .gap(0.0)
        .padding(Edges::ZERO)
        .child(toolbar)
        .child(Divider::horizontal())
        .child(content)
        .anchors(Edges::new(0.0, 0.0, 1.0, 1.0))
        .offsets(Edges::new(DETAIL_X, 0.0, 0.0, 0.0))
        .background(SurfaceStyle::new(theme.palette.background))
        .capture(move |node| detail_ids.borrow_mut().detail = Some(node))
}

#[cfg(test)]
mod tests {
    use super::*;
    use draw_backend_recording::RecordingBackend;
    use draw_core::PointerButton;
    use draw_render::{DrawCommand, RenderBackend};

    fn laid_out(width: f32, height: f32) -> DemoApp {
        let viewport = Viewport::new(Size::new(width, height));
        let mut app = DemoApp::new();
        app.update(viewport, 0.016);
        app.layout(viewport);
        app
    }

    fn rect(app: &DemoApp, id: NodeId) -> Rect {
        app.ui().control(id).expect("control").rect
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
        let viewport = Viewport::new(Size::new(1100.0, 720.0));
        let mut app = DemoApp::new();
        app.ui_mut()
            .set_text_measurer(std::rc::Rc::new(draw_ui::FixedWidthTextMeasurer::default()));
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
    fn three_columns_are_side_by_side() {
        let app = laid_out(1100.0, 720.0);
        let sidebar = rect(&app, app.sidebar());
        let list = rect(&app, app.list());
        let detail = rect(&app, app.detail());

        assert!((sidebar.size.width - SIDEBAR_WIDTH).abs() < 1e-3);
        assert!((list.size.width - LIST_WIDTH).abs() < 1e-3);
        assert!(detail.size.width > 0.0);

        assert!((sidebar.left()).abs() < 1e-3);
        assert!((list.left() - SIDEBAR_WIDTH).abs() < 1e-3);
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

        let wide = Viewport::new(Size::new(1400.0, 800.0));
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
        app.ui.paint(&mut ctx);
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
                } if paint.color == app.theme.palette.accent
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
