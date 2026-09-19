//! Shared, backend-neutral demo application: a three-column, macOS-style notes
//! app built from `draw_kit` components on the frozen `draw_ui` core.
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

use std::cell::Cell;
use std::rc::Rc;

use draw_core::{Color, Edges, EventResult, InputEvent, NodeId, Rect, Size, Vec2, Viewport};
use draw_kit::{
    fill_rounded_rect, fill_rounded_rect_corners, inset, Badge, Button, Checkbox, CornerRadii,
    Divider, Kit, SurfaceStyle, Switch, Text, Tone,
};
use draw_render::PaintContext;
use draw_theme::{radius, space, TextSize, Theme};
use draw_ui::{Align, Flex, Justify, Label, Panel, TextOptions, Ui};

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
        body: "draw_kit layers themed chrome over draw_ui without extending the core \
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
    kit: Kit,
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
        let mut ui_storage = Ui::new();
        let ui = &mut ui_storage;
        let mut kit = Kit::new(theme);
        let root = ui.root();

        let selected = Rc::new(Cell::new(0));
        let selected_nav = Rc::new(Cell::new(0));

        // ---- column shells + separators ---------------------------------
        let sidebar = ui.add(
            root,
            Flex::column().gap(space::MD).padding(Edges::new(
                space::LG,
                space::MD,
                space::MD,
                space::MD,
            )),
        );
        ui.set_anchors(sidebar.id(), Edges::new(0.0, 0.0, 0.0, 1.0));
        ui.set_offsets(sidebar.id(), Edges::new(0.0, 0.0, SIDEBAR_WIDTH, 0.0));
        kit.surface(sidebar.id(), SurfaceStyle::new(theme.palette.surface));

        let list = ui.add(
            root,
            Flex::column().gap(space::XS).padding(Edges::new(
                space::MD,
                space::MD,
                space::MD,
                space::MD,
            )),
        );
        ui.set_anchors(list.id(), Edges::new(0.0, 0.0, 0.0, 1.0));
        ui.set_offsets(list.id(), Edges::new(SIDEBAR_WIDTH, 0.0, DETAIL_X, 0.0));
        kit.surface(list.id(), SurfaceStyle::new(theme.palette.background));

        let detail = ui.add(root, Flex::column().gap(0.0).padding(Edges::ZERO));
        ui.set_anchors(detail.id(), Edges::new(0.0, 0.0, 1.0, 1.0));
        ui.set_offsets(detail.id(), Edges::new(DETAIL_X, 0.0, 0.0, 0.0));
        kit.surface(detail.id(), SurfaceStyle::new(theme.palette.background));

        for x in [SIDEBAR_WIDTH, DETAIL_X] {
            let separator = ui.add(root, Panel::new().color(theme.palette.border_subtle).flat());
            ui.set_anchors(separator.id(), Edges::new(0.0, 0.0, 0.0, 1.0));
            ui.set_offsets(separator.id(), Edges::new(x, 0.0, x + 1.0, 0.0));
        }

        // ---- sidebar ----------------------------------------------------
        let traffic = ui.add(
            sidebar.id(),
            Flex::row().gap(space::XS).padding(Edges::ZERO),
        );
        ui.set_min_size(traffic.id(), Size::new(0.0, 12.0));
        kit.foreground(traffic.id(), |ctx, rect, theme, _| {
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

        let title_row = ui.add(
            sidebar.id(),
            Flex::row()
                .align(Align::Center)
                .gap(space::SM)
                .padding(Edges::ZERO),
        );
        let app_icon = add_placeholder(&mut kit, ui, title_row.id(), 20.0);
        kit.surface(
            app_icon.id(),
            SurfaceStyle::new(theme.palette.accent).radius(radius::SM),
        );
        kit.foreground(app_icon.id(), |ctx, rect, theme, _| {
            fill_rounded_rect(ctx, inset(rect, 6.0), 1.0, theme.palette.on_accent);
        });
        kit.add(ui, title_row.id(), Text::subheading("Quill"));

        let search = ui.add(sidebar.id(), Panel::new().color(Color::TRANSPARENT).flat());
        ui.set_min_size(search.id(), Size::new(0.0, 30.0));
        kit.surface(
            search.id(),
            SurfaceStyle::new(theme.palette.surface_raised)
                .border(theme.palette.border)
                .radius(radius::MD),
        );
        let search_label = ui.add(
            search.id(),
            Label::new("Search")
                .font_size(TextSize::Small.px())
                .color(theme.palette.muted)
                .text_options(TextOptions::no_wrap()),
        );
        ui.set_anchors(search_label.id(), Edges::new(0.0, 0.5, 1.0, 0.5));
        ui.set_offsets(
            search_label.id(),
            Edges::new(space::SM, -8.0, -space::SM, 8.0),
        );

        kit.add(
            ui,
            sidebar.id(),
            Text::caption("Library").tone(Tone::Subtle),
        );
        let mut nav_rows = Vec::new();
        for (index, label) in NAV_ITEMS.iter().enumerate() {
            nav_rows.push(add_nav_row(
                &mut kit,
                ui,
                sidebar.id(),
                label,
                index,
                &selected_nav,
            ));
        }

        kit.add(ui, sidebar.id(), Text::caption("Tags").tone(Tone::Subtle));
        for (index, label) in TAG_ITEMS.iter().enumerate() {
            let row_index = NAV_ITEMS.len() + index;
            nav_rows.push(add_nav_row(
                &mut kit,
                ui,
                sidebar.id(),
                label,
                row_index,
                &selected_nav,
            ));
        }

        let spacer = ui.add(sidebar.id(), Flex::new().padding(Edges::ZERO));
        ui.set_flex_grow(spacer.id(), 1.0);
        let footer = ui.add(
            sidebar.id(),
            Flex::row()
                .align(Align::Center)
                .gap(space::SM)
                .padding(Edges::ZERO),
        );
        kit.add(ui, footer.id(), Badge::new("v0.1.0").tone(Tone::Muted));
        kit.add(ui, footer.id(), Text::caption("local").tone(Tone::Subtle));

        // ---- content list ----------------------------------------------
        let header = ui.add(
            list.id(),
            Flex::row()
                .align(Align::Center)
                .justify(Justify::SpaceBetween)
                .gap(space::SM)
                .padding(Edges::new(space::SM, space::XS, space::SM, space::XS)),
        );
        ui.set_min_size(header.id(), Size::new(0.0, 32.0));
        kit.add(ui, header.id(), Text::heading("All Notes"));
        kit.add(
            ui,
            header.id(),
            Text::small(format!("{} notes", NOTES.len())).tone(Tone::Muted),
        );
        kit.add(ui, list.id(), Divider::horizontal());

        let mut list_rows = Vec::new();
        for (index, note) in NOTES.iter().enumerate() {
            list_rows.push(add_note_row(
                &mut kit,
                ui,
                list.id(),
                &theme,
                note,
                index,
                &selected,
            ));
        }

        // ---- detail pane ------------------------------------------------
        let toolbar = ui.add(
            detail.id(),
            Flex::row()
                .align(Align::Center)
                .gap(space::SM)
                .padding(Edges::new(space::LG, space::SM, space::LG, space::SM)),
        );
        ui.set_min_size(toolbar.id(), Size::new(0.0, 48.0));

        let nav_group = ui.add(
            toolbar.id(),
            Flex::row()
                .align(Align::Center)
                .gap(space::XS)
                .padding(Edges::ZERO),
        );
        let back = add_placeholder(&mut kit, ui, nav_group.id(), 28.0);
        let selected_prev = selected.clone();
        kit.on_click(back.id(), move || {
            let value = selected_prev.get();
            selected_prev.set(value.saturating_sub(1));
        });
        let forward = add_placeholder(&mut kit, ui, nav_group.id(), 28.0);
        let selected_next = selected.clone();
        kit.on_click(forward.id(), move || {
            let value = selected_next.get();
            selected_next.set((value + 1).min(NOTES.len() - 1));
        });

        let toolbar_spacer = ui.add(toolbar.id(), Flex::new().padding(Edges::ZERO));
        ui.set_flex_grow(toolbar_spacer.id(), 1.0);

        let actions = ui.add(
            toolbar.id(),
            Flex::row()
                .align(Align::Center)
                .gap(space::SM)
                .padding(Edges::ZERO),
        );
        kit.add(ui, actions.id(), Button::ghost("Share"));
        let clicks = Rc::new(Cell::new(0));
        let counter = clicks.clone();
        let primary_button = kit.add(
            ui,
            actions.id(),
            Button::primary("New Note").on_click(move || counter.set(counter.get() + 1)),
        );
        kit.add(ui, detail.id(), Divider::horizontal());

        let content = ui.add(
            detail.id(),
            Flex::column().gap(space::LG).padding(Edges::new(
                space::XXL,
                space::LG,
                space::XXL,
                space::XXL,
            )),
        );

        let hero = ui.add(content.id(), Flex::new().padding(Edges::ZERO));
        ui.set_min_size(hero.id(), Size::new(0.0, 220.0));
        kit.surface(
            hero.id(),
            SurfaceStyle::new(theme.palette.surface_raised)
                .border(theme.palette.border)
                .radius(radius::LG),
        );

        let note = &NOTES[0];
        let title_row = ui.add(
            content.id(),
            Flex::row()
                .align(Align::Center)
                .gap(space::SM)
                .padding(Edges::ZERO),
        );
        let detail_title = kit.add(ui, title_row.id(), Text::heading(note.title));
        ui.set_flex_grow(detail_title.id(), 1.0);
        let detail_tag = kit.add(
            ui,
            title_row.id(),
            Text::caption(note.tag).tone(Tone::Accent),
        );
        let detail_meta = kit.add(
            ui,
            content.id(),
            Text::small(format!("Edited {} · {}", note.modified, note.tag)).tone(Tone::Muted),
        );
        let detail_body = kit.add(ui, content.id(), Text::new(note.body).tone(Tone::Muted));

        let preferences = ui.add(
            content.id(),
            Flex::row()
                .align(Align::Center)
                .gap(space::XL)
                .padding(Edges::ZERO),
        );
        kit.add(ui, preferences.id(), Checkbox::new("Pin note"));
        kit.add(ui, preferences.id(), Switch::new().label("Shared"));

        kit.add(ui, content.id(), Divider::horizontal());
        let actions = ui.add(
            content.id(),
            Flex::row()
                .align(Align::Center)
                .gap(space::SM)
                .padding(Edges::ZERO),
        );
        kit.add(ui, actions.id(), Button::secondary("Open"));
        kit.add(ui, actions.id(), Button::secondary("Duplicate"));
        kit.add(ui, actions.id(), Button::ghost("Delete"));

        // ---- hero: a static image placeholder --------------------------
        // No animation: the app is static so idle performance can be profiled.
        kit.foreground(hero.id(), |ctx, rect, theme, _| {
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
        });

        Self {
            ui: ui_storage,
            kit,
            theme,
            sidebar: sidebar.id(),
            list: list.id(),
            detail: detail.id(),
            hero: hero.id(),
            list_rows,
            nav_rows,
            detail_title: detail_title.id(),
            detail_body: detail_body.id(),
            detail_tag: detail_tag.id(),
            detail_meta: detail_meta.id(),
            primary_button: primary_button.id(),
            selected,
            selected_nav,
            clicks,
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

    /// Updates state-driven text. The app is static: `dt` is accepted for a
    /// uniform host pipeline but nothing animates.
    pub fn update(&mut self, viewport: Viewport, _dt: f32) {
        self.viewport = viewport;

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

    /// Resolves UI layout for `viewport`.
    pub fn layout(&mut self, viewport: Viewport) {
        self.viewport = viewport;
        self.ui.layout(viewport);
    }

    /// Emits this frame's `DrawList` into `ctx`.
    ///
    /// Order: window background, kit surfaces, UI content, kit foregrounds
    /// (indicators, icons, the hero image placeholder).
    pub fn paint(&self, ctx: &mut PaintContext) {
        let size = self.viewport.logical_size();
        ctx.fill_rect(
            Rect::from_min_size(Vec2::ZERO, size),
            self.theme.palette.background,
        );

        self.kit.paint_surfaces(&self.ui, ctx);
        self.ui.paint(ctx);
        self.kit.paint_foreground(&self.ui, ctx);
    }

    /// Routes a backend-neutral input event to kit interactions then the UI.
    pub fn event(&mut self, event: &InputEvent) -> EventResult {
        let kit_handled = self.kit.handle_input(&self.ui, event);
        let ui_result = self.ui.handle_input(event);
        if kit_handled || ui_result.is_handled() {
            EventResult::Handled
        } else {
            ui_result
        }
    }

    /// Controls in the demo UI.
    pub fn control_count(&self) -> usize {
        self.ui.control_count()
    }

    /// Whether the pointer is over anything clickable (kit component or a
    /// core button). Hosts use this for cursor feedback.
    pub fn pointer_over_clickable(&self) -> bool {
        self.kit.hovered().is_some() || self.ui.hovered_is_button()
    }
}

/// Adds a compact rounded-square icon/thumbnail placeholder.
fn add_placeholder(kit: &mut Kit, ui: &mut Ui, parent: NodeId, size: f32) -> draw_kit::ControlRef {
    let node = ui.add(parent, Panel::new().color(Color::TRANSPARENT).flat());
    ui.set_min_size(node.id(), Size::new(size, size));
    kit.dynamic_surface(node.id(), move |theme, state| {
        let fill = if state.hovered || state.pressed {
            theme.palette.surface_hover
        } else {
            Color::TRANSPARENT
        };
        SurfaceStyle::new(fill).radius(radius::SM)
    });
    kit.foreground(node.id(), |ctx, rect, theme, _| {
        let inner = inset(rect, rect.size.width * 0.32);
        fill_rounded_rect(ctx, inner, 1.5, theme.palette.subtle);
    });
    node
}

/// Adds one sidebar navigation row (icon placeholder + label + selection).
fn add_nav_row(
    kit: &mut Kit,
    ui: &mut Ui,
    parent: NodeId,
    label: &str,
    index: usize,
    selected: &Rc<Cell<usize>>,
) -> NodeId {
    let row = ui.add(
        parent,
        Flex::row()
            .align(Align::Center)
            .gap(space::SM)
            .padding(Edges::new(space::SM, space::XS, space::SM, space::XS)),
    );
    ui.set_min_size(row.id(), Size::new(0.0, 28.0));

    let icon = add_placeholder(kit, ui, row.id(), 14.0);
    let _ = icon;

    kit.add(ui, row.id(), Text::small(label));

    let state = selected.clone();
    let current = index;
    kit.dynamic_surface(row.id(), move |theme, interact| {
        let fill = if state.get() == current {
            theme.palette.selection
        } else if interact.hovered {
            theme.palette.surface_hover
        } else {
            Color::TRANSPARENT
        };
        SurfaceStyle::new(fill).radius(radius::SM)
    });

    let click = selected.clone();
    let target = index;
    kit.on_click(row.id(), move || click.set(target));
    row.id()
}

/// Adds one note row to the content list.
fn add_note_row(
    kit: &mut Kit,
    ui: &mut Ui,
    parent: NodeId,
    theme: &Theme,
    note: &Note,
    index: usize,
    selected: &Rc<Cell<usize>>,
) -> NodeId {
    let row = ui.add(
        parent,
        Flex::row()
            .align(Align::Start)
            .gap(space::MD)
            .padding(Edges::all(space::SM)),
    );
    ui.set_min_size(row.id(), Size::new(0.0, 60.0));

    let state = selected.clone();
    let current = index;
    // Square left corners, rounded right corners (a macOS-style list item).
    kit.dynamic_surface(row.id(), move |theme, interact| {
        let fill = if state.get() == current {
            theme.palette.selection
        } else if interact.hovered {
            theme.palette.surface_hover
        } else {
            Color::TRANSPARENT
        };
        SurfaceStyle::new(fill).corners(CornerRadii::new(0.0, radius::MD, radius::MD, 0.0))
    });

    let state = selected.clone();
    let current = index;
    // A full-height accent bar marks the selected item.
    kit.foreground(row.id(), move |ctx, rect, theme, _| {
        if state.get() == current {
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
    });

    let thumb = add_placeholder(kit, ui, row.id(), 44.0);
    kit.surface(
        thumb.id(),
        SurfaceStyle::new(theme.palette.surface_raised)
            .border(theme.palette.border)
            .radius(radius::MD),
    );
    let shade = 0.18 + index as f32 * 0.05;
    kit.foreground(thumb.id(), move |ctx, rect, theme, _| {
        fill_rounded_rect(
            ctx,
            inset(rect, 12.0),
            2.0,
            theme.palette.subtle.with_alpha(shade),
        );
    });

    let column = ui.add(
        row.id(),
        Flex::column().gap(space::XXS).padding(Edges::ZERO),
    );
    ui.set_flex_grow(column.id(), 1.0);
    kit.add(ui, column.id(), Text::small(note.title));
    kit.add(
        ui,
        column.id(),
        Text::caption(note.snippet)
            .tone(Tone::Muted)
            .max_lines(1)
            .ellipsis(true),
    );
    kit.add(
        ui,
        column.id(),
        Text::caption(note.modified).tone(Tone::Subtle),
    );

    let click = selected.clone();
    let target = index;
    kit.on_click(row.id(), move || click.set(target));
    row.id()
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
        app.kit.paint_surfaces(&app.ui, &mut ctx);
        app.kit.paint_foreground(&app.ui, &mut ctx);
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
