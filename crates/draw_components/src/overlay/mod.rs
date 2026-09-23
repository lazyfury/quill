//! A generic overlay layer: confirm dialogs, popovers, tooltips and toasts on
//! top of the app's [`SceneTree`].
//!
//! [`Overlays`] owns its own private `SceneTree`, so the host lays out and
//! paints its main UI as usual and then layers this on top:
//!
//! ```ignore
//! draw_ui::layout(&mut tree, viewport);
//! overlays.layout(&tree, viewport);   // anchor to laid-out targets
//!
//! draw_ui::paint(&tree, &mut ctx);    // main UI (decor + content)
//! overlays.paint(&mut ctx);           // scrim + overlay content, on top
//! ```
//!
//! Input goes to the overlays first; a modal overlay consumes everything so the
//! UI underneath cannot react:
//!
//! ```ignore
//! if overlays.handle_input(event).is_handled() {
//!     return EventResult::Handled;
//! }
//! app.event(event);
//! ```
//!
//! Entries are declarative and rebuilt only when the set changes, so per-frame
//! layout stays incremental. Positioning (with edge flipping) is in
//! [`placement`].

mod placement;
use std::cell::RefCell;
use std::rc::Rc;

use crate::base::{Component, Flex, Label};
use draw_core::{Color, Edges, EventResult, InputEvent, Key, NodeId, Size, ViewportSize};
use draw_render::PaintContext;
use draw_scene::SceneTree;
use draw_theme::{radius, Space, SurfaceLevel, TextSize, Theme, Tone};
use draw_ui::{Align, Justify, MouseFilter};

use crate::Button;
use draw_ui::SurfaceStyle;

/// Builds the overlay layer's private tree with a viewport-filling root.
fn overlay_tree(measurer: Option<Rc<dyn draw_ui::TextMeasurer>>) -> (SceneTree, NodeId) {
    let mut tree = SceneTree::new();
    if let Some(measurer) = measurer {
        draw_ui::set_text_measurer(&mut tree, measurer);
    }
    let tree_root = tree.root();
    let root = tree.add_child(tree_root, Flex::column().mouse_filter(MouseFilter::Ignore));
    (tree, root)
}

pub use placement::Placement;

type Callback = Rc<RefCell<dyn FnMut()>>;
type ContentFn = Rc<dyn Fn(&mut SceneTree, NodeId)>;

const MARGIN: f32 = 8.0;
const OFFSET: f32 = 8.0;
const CONFIRM_WIDTH: f32 = 320.0;
const MESSAGE_DURATION: f32 = 2.5;

/// Identifies an open overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OverlayId(u64);

/// What an overlay is anchored to.
#[derive(Debug, Clone, Copy)]
enum Anchor {
    /// A control in the host UI (resolved from its laid-out rect).
    Target(NodeId),
    /// The viewport itself (modal dialogs / toasts).
    ViewportSize,
}

/// The content of an overlay.
enum Kind {
    Confirm {
        title: String,
        message: String,
        confirm: String,
        cancel: String,
        destructive: bool,
    },
    Popover {
        title: Option<String>,
        content: ContentFn,
    },
    /// A drop-down menu: like a popover but the content owns all chrome
    /// (a [`Menu`](crate::Menu) draws its own surface), so no default padding or
    /// border is added around it.
    Menu {
        content: ContentFn,
    },
    Tips {
        text: String,
    },
    Message {
        text: String,
        tone: Tone,
    },
}

enum Action {
    Confirm(OverlayId),
    Cancel(OverlayId),
}

struct Entry {
    id: OverlayId,
    kind: Kind,
    anchor: Anchor,
    placement: Placement,
    offset: f32,
    modal: bool,
    dismiss_on_outside: bool,
    dismiss_on_escape: bool,
    duration: Option<f32>,
    elapsed: f32,
    scrim: Option<Color>,
    root: Option<NodeId>,
    on_confirm: Option<Callback>,
    on_cancel: Option<Callback>,
    on_close: Option<Callback>,
}

impl Entry {
    fn new(id: OverlayId, kind: Kind, anchor: Anchor, placement: Placement) -> Self {
        Self {
            id,
            kind,
            anchor,
            placement,
            offset: OFFSET,
            modal: false,
            dismiss_on_outside: false,
            dismiss_on_escape: false,
            duration: None,
            elapsed: 0.0,
            scrim: None,
            root: None,
            on_confirm: None,
            on_cancel: None,
            on_close: None,
        }
    }
}

/// The overlay layer.
pub struct Overlays {
    theme: &'static dyn Theme,
    tree: SceneTree,
    root: NodeId,
    entries: Vec<Entry>,
    measurer: Option<Rc<dyn draw_ui::TextMeasurer>>,
    next_id: u64,
    viewport: ViewportSize,
    dirty: bool,
    actions: Rc<RefCell<Vec<Action>>>,
}

impl Overlays {
    pub fn new(theme: &'static dyn Theme) -> Self {
        let (tree, root) = overlay_tree(None);
        Self {
            theme,
            tree,
            root,
            entries: Vec::new(),
            measurer: None,
            next_id: 1,
            viewport: ViewportSize::default(),
            dirty: false,
            actions: Rc::new(RefCell::new(Vec::new())),
        }
    }

    pub fn theme(&self) -> &'static dyn Theme {
        self.theme
    }

    /// Swaps the theme and rebuilds the overlay tree.
    pub fn set_theme(&mut self, theme: &'static dyn Theme) {
        self.theme = theme;
        self.dirty = true;
    }

    /// Uses `measurer` for overlay text layout, matching the host UI.
    pub fn set_text_measurer(&mut self, measurer: Rc<dyn draw_ui::TextMeasurer>) {
        draw_ui::set_text_measurer(&mut self.tree, measurer.clone());
        self.measurer = Some(measurer);
        self.dirty = true;
    }

    /// Opens a modal confirmation dialog.
    ///
    /// Close it with [`Overlays::on_confirm`], [`Overlays::on_cancel`], a click
    /// outside, or Escape.
    pub fn confirm(&mut self, title: impl Into<String>, message: impl Into<String>) -> OverlayId {
        let mut entry = Entry::new(
            OverlayId(0),
            Kind::Confirm {
                title: title.into(),
                message: message.into(),
                confirm: "OK".into(),
                cancel: "Cancel".into(),
                destructive: false,
            },
            Anchor::ViewportSize,
            Placement::Center,
        );
        entry.modal = true;
        entry.dismiss_on_outside = true;
        entry.dismiss_on_escape = true;
        entry.scrim = Some(Color::new(0.0, 0.0, 0.0, 0.35));
        self.push(entry)
    }

    /// Opens a popover anchored to `target`, built by `content`.
    pub fn popover(
        &mut self,
        target: NodeId,
        placement: Placement,
        content: impl Fn(&mut SceneTree, NodeId) + 'static,
    ) -> OverlayId {
        let mut entry = Entry::new(
            OverlayId(0),
            Kind::Popover {
                title: None,
                content: Rc::new(content),
            },
            Anchor::Target(target),
            placement,
        );
        entry.dismiss_on_outside = true;
        entry.dismiss_on_escape = true;
        self.push(entry)
    }

    /// Opens a drop-down menu anchored below `target`, built by `content`.
    ///
    /// `content` owns its chrome, so it typically adds a
    /// [`Menu`](crate::Menu) and its [`MenuItem`](crate::MenuItem)s. The menu
    /// closes on Escape or a click outside.
    pub fn menu(
        &mut self,
        target: NodeId,
        content: impl Fn(&mut SceneTree, NodeId) + 'static,
    ) -> OverlayId {
        let mut entry = Entry::new(
            OverlayId(0),
            Kind::Menu {
                content: Rc::new(content),
            },
            Anchor::Target(target),
            Placement::BelowStart,
        );
        entry.dismiss_on_outside = true;
        entry.dismiss_on_escape = true;
        self.push(entry)
    }

    /// Opens a tooltip anchored to `target`. It shows while `target` (or a
    /// descendant) is hovered and closes automatically when the pointer leaves.
    pub fn tips(&mut self, target: NodeId, text: impl Into<String>) -> OverlayId {
        let entry = Entry::new(
            OverlayId(0),
            Kind::Tips { text: text.into() },
            Anchor::Target(target),
            Placement::Above,
        );
        self.push(entry)
    }

    /// Opens a transient message (toast), auto-dismissed after a short delay.
    pub fn message(&mut self, text: impl Into<String>) -> OverlayId {
        self.message_tone(text, Tone::Default)
    }

    /// A [`Overlays::message`] with an explicit [`Tone`] for the text color.
    pub fn message_tone(&mut self, text: impl Into<String>, tone: Tone) -> OverlayId {
        let mut entry = Entry::new(
            OverlayId(0),
            Kind::Message {
                text: text.into(),
                tone,
            },
            Anchor::ViewportSize,
            Placement::BottomCenter,
        );
        entry.duration = Some(MESSAGE_DURATION);
        self.push(entry)
    }

    /// Overrides the confirm button label (default `"OK"`).
    pub fn confirm_label(&mut self, id: OverlayId, label: impl Into<String>) -> &mut Self {
        if let Some(entry) = self.entry_mut(id) {
            if let Kind::Confirm { confirm, .. } = &mut entry.kind {
                *confirm = label.into();
            }
        }
        self
    }

    /// Overrides the cancel button label (default `"Cancel"`).
    pub fn cancel_label(&mut self, id: OverlayId, label: impl Into<String>) -> &mut Self {
        if let Some(entry) = self.entry_mut(id) {
            if let Kind::Confirm { cancel, .. } = &mut entry.kind {
                *cancel = label.into();
            }
        }
        self
    }

    /// Marks a confirmation as destructive (confirm button uses the error tone).
    pub fn destructive(&mut self, id: OverlayId, destructive: bool) -> &mut Self {
        if let Some(entry) = self.entry_mut(id) {
            if let Kind::Confirm { destructive: d, .. } = &mut entry.kind {
                *d = destructive;
            }
        }
        self
    }

    /// Sets the popover title.
    pub fn title(&mut self, id: OverlayId, title: impl Into<String>) -> &mut Self {
        if let Some(entry) = self.entry_mut(id) {
            if let Kind::Popover { title: t, .. } = &mut entry.kind {
                *t = Some(title.into());
            }
        }
        self
    }

    pub fn on_confirm(&mut self, id: OverlayId, callback: impl FnMut() + 'static) -> &mut Self {
        if let Some(entry) = self.entry_mut(id) {
            entry.on_confirm = Some(Rc::new(RefCell::new(callback)));
        }
        self
    }

    pub fn on_cancel(&mut self, id: OverlayId, callback: impl FnMut() + 'static) -> &mut Self {
        if let Some(entry) = self.entry_mut(id) {
            entry.on_cancel = Some(Rc::new(RefCell::new(callback)));
        }
        self
    }

    /// Called whenever an overlay closes (confirm, cancel, dismiss or
    /// [`Overlays::close`]).
    pub fn on_close(&mut self, id: OverlayId, callback: impl FnMut() + 'static) -> &mut Self {
        if let Some(entry) = self.entry_mut(id) {
            entry.on_close = Some(Rc::new(RefCell::new(callback)));
        }
        self
    }

    pub fn is_open(&self, id: OverlayId) -> bool {
        self.entries.iter().any(|entry| entry.id == id)
    }

    /// The overlay's resolved rectangle after [`Overlays::layout`].
    pub fn rect(&self, id: OverlayId) -> Option<draw_core::Rect> {
        let entry = self.entries.iter().find(|entry| entry.id == id)?;
        let root = entry.root?;
        draw_ui::control(&self.tree, root).map(|control| control.rect)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Closes one overlay, invoking its `on_close` callback if any.
    pub fn close(&mut self, id: OverlayId) {
        if let Some(index) = self.entries.iter().position(|entry| entry.id == id) {
            if let Some(callback) = self.entries[index].on_close.take() {
                (callback.borrow_mut())();
            }
            self.entries.remove(index);
            self.dirty = true;
        }
    }

    /// Closes every overlay.
    pub fn close_all(&mut self) {
        let ids: Vec<OverlayId> = self.entries.iter().map(|entry| entry.id).collect();
        for id in ids {
            self.close(id);
        }
    }

    /// Advances auto-dismiss timers.
    pub fn update(&mut self, dt: f32) {
        for entry in &mut self.entries {
            if entry.duration.is_some() {
                entry.elapsed += dt;
            }
        }
        let expired: Vec<OverlayId> = self
            .entries
            .iter()
            .filter(|entry| {
                entry
                    .duration
                    .is_some_and(|duration| entry.elapsed >= duration)
            })
            .map(|entry| entry.id)
            .collect();
        for id in expired {
            self.close(id);
        }
    }

    /// Resolves positions against the host UI and lays out the overlay tree.
    ///
    /// Call after the host's own `Ui::layout`.
    pub fn layout(&mut self, host_tree: &SceneTree, viewport: ViewportSize) {
        // Tooltips live only while their target (or a descendant) is hovered.
        let hovered = draw_ui::hovered(host_tree);
        let stale: Vec<OverlayId> = self
            .entries
            .iter()
            .filter_map(|entry| match (entry.anchor, &entry.kind) {
                (Anchor::Target(target), Kind::Tips { .. })
                    if !hovered
                        .is_some_and(|node| is_self_or_ancestor(host_tree, target, node)) =>
                {
                    Some(entry.id)
                }
                _ => None,
            })
            .collect();
        for id in stale {
            self.close(id);
        }

        self.viewport = viewport;
        if self.dirty {
            self.rebuild();
        }

        draw_ui::layout(&mut self.tree, viewport);
        let viewport_rect = viewport.logical_rect();
        let mut moved = false;
        for entry in &mut self.entries {
            let Some(root) = entry.root else {
                continue;
            };
            let size = draw_ui::control(&self.tree, root)
                .map(|control| control.rect.size)
                .unwrap_or(Size::ZERO);
            let anchor = match entry.anchor {
                Anchor::Target(id) => draw_ui::control(host_tree, id).map(|control| control.rect),
                Anchor::ViewportSize => None,
            };
            let rect = placement::place(
                anchor.unwrap_or(viewport_rect),
                size,
                viewport_rect,
                entry.placement,
                entry.offset,
                MARGIN,
            );
            if draw_ui::control(&self.tree, root).map(|control| control.rect) != Some(rect) {
                crate::base::update_control(&mut self.tree, root, |d| d.anchors = Edges::ZERO);
                crate::base::update_control(&mut self.tree, root, |d| {
                    d.offsets = Edges::new(rect.left(), rect.top(), rect.right(), rect.bottom())
                });
                moved = true;
            }
        }
        if moved {
            draw_ui::layout(&mut self.tree, viewport);
        }
    }

    /// Paints scrims and overlay content. Call last, over the host UI.
    pub fn paint(&self, ctx: &mut PaintContext) {
        if self.entries.is_empty() {
            return;
        }
        let viewport = self.viewport.logical_rect();
        for entry in &self.entries {
            if let Some(scrim) = entry.scrim {
                ctx.fill_rect(viewport, scrim);
            }
        }
        draw_ui::paint(&self.tree, ctx);
    }

    /// Routes an event to the overlays.
    ///
    /// Returns [`EventResult::Handled`] when an overlay consumed it or a modal
    /// is open (so the host UI must not process it).
    pub fn handle_input(&mut self, event: &InputEvent) -> EventResult {
        if self.entries.is_empty() {
            return EventResult::Ignored;
        }
        let modal = self.entries.iter().any(|entry| entry.modal);

        if let InputEvent::KeyDown { key: Key::Escape } = event {
            if let Some(entry) = self
                .entries
                .iter()
                .rev()
                .find(|entry| entry.dismiss_on_escape)
            {
                self.close(entry.id);
                return EventResult::Handled;
            }
        }

        let pointer = match event {
            InputEvent::PointerMove { position }
            | InputEvent::PointerDown { position, .. }
            | InputEvent::PointerUp { position, .. } => Some(*position),
            _ => None,
        };

        if let InputEvent::PointerDown { position, .. } = event {
            if draw_ui::hit_test(&self.tree, *position).is_none() {
                if let Some(entry) = self
                    .entries
                    .iter()
                    .rev()
                    .find(|entry| entry.dismiss_on_outside)
                {
                    self.close(entry.id);
                    return EventResult::Handled;
                }
            }
        }

        let ui_result = draw_ui::handle_input(&mut self.tree, event);
        self.process_actions();

        // `Ui::handle_input` reports `Handled` for every `PointerUp`, so pointer
        // events only count while they are over overlay content.
        let consumed = match pointer {
            Some(position) => {
                modal
                    || (draw_ui::hit_test(&self.tree, position).is_some() && ui_result.is_handled())
            }
            None => modal || ui_result.is_handled(),
        };
        if consumed {
            EventResult::Handled
        } else {
            EventResult::Ignored
        }
    }

    fn push(&mut self, mut entry: Entry) -> OverlayId {
        let id = OverlayId(self.next_id);
        self.next_id += 1;
        entry.id = id;
        self.entries.push(entry);
        self.dirty = true;
        id
    }

    fn entry_mut(&mut self, id: OverlayId) -> Option<&mut Entry> {
        let entry = self.entries.iter_mut().find(|entry| entry.id == id)?;
        self.dirty = true;
        Some(entry)
    }

    fn rebuild(&mut self) {
        let (tree, root) = overlay_tree(self.measurer.clone());
        self.tree = tree;
        self.root = root;
        let theme = self.theme;
        for entry in &mut self.entries {
            entry.root = Some(build_entry(
                entry,
                &mut self.tree,
                self.root,
                theme,
                self.actions.clone(),
            ));
        }
        self.dirty = false;
    }

    fn process_actions(&mut self) {
        loop {
            let actions: Vec<Action> = std::mem::take(&mut *self.actions.borrow_mut());
            if actions.is_empty() {
                break;
            }
            for action in actions {
                match action {
                    Action::Confirm(id) => {
                        if let Some(callback) = self.callback_mut(id, Slot::Confirm) {
                            (callback.borrow_mut())();
                        }
                        self.close(id);
                    }
                    Action::Cancel(id) => {
                        if let Some(callback) = self.callback_mut(id, Slot::Cancel) {
                            (callback.borrow_mut())();
                        }
                        self.close(id);
                    }
                }
            }
        }
    }

    fn callback_mut(&mut self, id: OverlayId, slot: Slot) -> Option<Callback> {
        let entry = self.entries.iter_mut().find(|entry| entry.id == id)?;
        match slot {
            Slot::Confirm => entry.on_confirm.take(),
            Slot::Cancel => entry.on_cancel.take(),
        }
    }
}

enum Slot {
    Confirm,
    Cancel,
}

fn is_self_or_ancestor(host_tree: &SceneTree, target: NodeId, mut node: NodeId) -> bool {
    loop {
        if node == target {
            return true;
        }
        match host_tree.parent(node) {
            Some(parent) => node = parent,
            None => return false,
        }
    }
}

/// Builds one overlay's content and returns its root node.
fn build_entry(
    entry: &Entry,
    tree: &mut SceneTree,
    root: NodeId,
    theme: &'static dyn Theme,
    actions: Rc<RefCell<Vec<Action>>>,
) -> NodeId {
    let palette = theme.palette();
    let surface = SurfaceStyle::new(theme.surface(SurfaceLevel::Floating))
        .border(palette.border)
        .radius(radius::LG);

    match &entry.kind {
        Kind::Confirm {
            title,
            message,
            confirm,
            cancel,
            destructive,
        } => {
            let id = entry.id;
            let cancel_actions = actions.clone();
            let confirm_actions = actions;
            let cancel = Button::ghost(cancel.clone(), theme).on_click(move || {
                cancel_actions.borrow_mut().push(Action::Cancel(id));
            });
            let confirm = if *destructive {
                Button::destructive(confirm.clone(), theme)
            } else {
                Button::primary(confirm.clone(), theme)
            }
            .on_click(move || {
                confirm_actions.borrow_mut().push(Action::Confirm(id));
            });

            tree.add_child(
                root,
                Flex::column()
                    .gap(theme.spacing(Space::MD))
                    .padding(Edges::all(theme.spacing(Space::LG)))
                    .anchors(Edges::ZERO)
                    .offsets(Edges::ZERO)
                    .min_size(CONFIRM_WIDTH, 0.0)
                    .surface(surface)
                    .child(
                        Label::new(title.clone())
                            .font_size(theme.font_size(TextSize::Heading))
                            .color(palette.foreground),
                    )
                    .child(
                        Label::new(message.clone())
                            .font_size(theme.font_size(TextSize::Body))
                            .color(palette.muted),
                    )
                    .child(
                        Flex::row()
                            .align(Align::Center)
                            .justify(Justify::End)
                            .gap(theme.spacing(Space::SM))
                            .padding(Edges::ZERO)
                            .child(cancel)
                            .child(confirm),
                    ),
            )
        }
        Kind::Popover { title, content } => {
            let node = tree.add_child(
                root,
                Flex::column()
                    .gap(theme.spacing(Space::SM))
                    .padding(Edges::all(theme.spacing(Space::MD)))
                    .anchors(Edges::ZERO)
                    .offsets(Edges::ZERO)
                    .surface(surface),
            );
            if let Some(title) = title {
                tree.add_child(
                    node,
                    Label::new(title.clone())
                        .font_size(theme.font_size(TextSize::Subheading))
                        .color(palette.foreground),
                );
            }
            content(tree, node);
            node
        }
        Kind::Menu { content } => {
            // No surface/padding here: the added `Menu` owns all chrome, so this
            // node is only a positioning wrapper.
            let node = tree.add_child(
                root,
                Flex::column()
                    .gap(0.0)
                    .padding(Edges::ZERO)
                    .anchors(Edges::ZERO)
                    .offsets(Edges::ZERO),
            );
            content(tree, node);
            node
        }
        Kind::Tips { text } => {
            let node = tree.add_child(
                root,
                Flex::row()
                    .padding(Edges::symmetric(
                        theme.spacing(Space::SM),
                        theme.spacing(Space::XS),
                    ))
                    .gap(0.0)
                    .anchors(Edges::ZERO)
                    .offsets(Edges::ZERO)
                    .mouse_filter(MouseFilter::Ignore)
                    .surface(
                        SurfaceStyle::new(palette.foreground.lerp(palette.background, 0.08))
                            .radius(radius::SM),
                    ),
            );
            tree.add_child(
                node,
                Label::new(text.clone())
                    .font_size(theme.font_size(TextSize::Small))
                    .color(palette.background)
                    .mouse_filter(MouseFilter::Ignore),
            );
            node
        }
        Kind::Message { text, tone } => {
            let node = tree.add_child(
                root,
                Flex::row()
                    .align(Align::Center)
                    .padding(Edges::symmetric(
                        theme.spacing(Space::MD),
                        theme.spacing(Space::SM),
                    ))
                    .gap(0.0)
                    .anchors(Edges::ZERO)
                    .offsets(Edges::ZERO)
                    .mouse_filter(MouseFilter::Ignore)
                    .surface(
                        SurfaceStyle::new(theme.surface(SurfaceLevel::Floating))
                            .border(palette.border)
                            .radius(radius::MD),
                    ),
            );
            tree.add_child(
                node,
                Label::new(text.clone())
                    .font_size(theme.font_size(TextSize::Small))
                    .color(tone.color(theme))
                    .mouse_filter(MouseFilter::Ignore),
            );
            node
        }
    }
}
