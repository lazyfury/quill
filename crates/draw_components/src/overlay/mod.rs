//! A generic overlay layer: confirm dialogs, popovers, tooltips and toasts on
//! top of the [`Ui`] stack.
//!
//! [`Overlays`] owns its own [`Ui`], so the host keeps painting its
//! main UI exactly as before and then layers this on top:
//!
//! ```ignore
//! app.ui.layout(viewport);
//! overlays.layout(&app.ui, viewport); // anchor to laid-out targets
//!
//! app.ui.paint(&mut ctx);             // main UI (decor + content)
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
#[cfg(test)]
mod tests;

use std::cell::RefCell;
use std::rc::Rc;

use draw_core::{Color, Edges, EventResult, InputEvent, Key, NodeId, Size, Viewport};
use draw_render::PaintContext;
use draw_theme::{radius, space, SurfaceLevel, TextSize, Theme};
use draw_ui::{Align, Flex, Justify, Label, MouseFilter, Ui};

use crate::Button;
use draw_ui::surface_decor;
use draw_ui::SurfaceStyle;
use draw_ui::Tone;

pub use placement::Placement;

type Callback = Rc<RefCell<dyn FnMut()>>;
type ContentFn = Rc<dyn Fn(&mut Ui, NodeId)>;

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
    Viewport,
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
    theme: Theme,
    ui: Ui,
    entries: Vec<Entry>,
    measurer: Option<Rc<dyn draw_ui::TextMeasurer>>,
    next_id: u64,
    viewport: Viewport,
    dirty: bool,
    actions: Rc<RefCell<Vec<Action>>>,
}

impl Overlays {
    pub fn new(theme: Theme) -> Self {
        let mut ui = Ui::new();
        ui.set_theme(theme);
        Self {
            theme,
            ui,
            entries: Vec::new(),
            measurer: None,
            next_id: 1,
            viewport: Viewport::default(),
            dirty: false,
            actions: Rc::new(RefCell::new(Vec::new())),
        }
    }

    pub fn theme(&self) -> &Theme {
        &self.theme
    }

    /// Swaps the theme and rebuilds the overlay tree.
    pub fn set_theme(&mut self, theme: Theme) {
        self.theme = theme;
        self.dirty = true;
    }

    /// Uses `measurer` for overlay text layout, matching the host UI.
    pub fn set_text_measurer(&mut self, measurer: Rc<dyn draw_ui::TextMeasurer>) {
        self.ui.set_text_measurer(measurer.clone());
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
            Anchor::Viewport,
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
        content: impl Fn(&mut Ui, NodeId) + 'static,
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
            Anchor::Viewport,
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
        self.ui.control(root).map(|control| control.rect)
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
    pub fn layout(&mut self, host: &Ui, viewport: Viewport) {
        // Tooltips live only while their target (or a descendant) is hovered.
        let hovered = host.hovered();
        let stale: Vec<OverlayId> = self
            .entries
            .iter()
            .filter_map(|entry| match (entry.anchor, &entry.kind) {
                (Anchor::Target(target), Kind::Tips { .. })
                    if !hovered.is_some_and(|node| is_self_or_ancestor(host, target, node)) =>
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

        self.ui.layout(viewport);
        let viewport_rect = viewport.logical_rect();
        let mut moved = false;
        for entry in &mut self.entries {
            let Some(root) = entry.root else {
                continue;
            };
            let size = self
                .ui
                .control(root)
                .map(|control| control.rect.size)
                .unwrap_or(Size::ZERO);
            let anchor = match entry.anchor {
                Anchor::Target(id) => host.control(id).map(|control| control.rect),
                Anchor::Viewport => None,
            };
            let rect = placement::place(
                anchor.unwrap_or(viewport_rect),
                size,
                viewport_rect,
                entry.placement,
                entry.offset,
                MARGIN,
            );
            if self.ui.control(root).map(|control| control.rect) != Some(rect) {
                self.ui.set_anchors(root, Edges::ZERO);
                self.ui.set_offsets(
                    root,
                    Edges::new(rect.left(), rect.top(), rect.right(), rect.bottom()),
                );
                moved = true;
            }
        }
        if moved {
            self.ui.layout(viewport);
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
        self.ui.paint(ctx);
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
            if self.ui.hit_test(*position).is_none() {
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

        let ui_result = self.ui.handle_input(event);
        self.process_actions();

        // `Ui::handle_input` reports `Handled` for every `PointerUp`, so pointer
        // events only count while they are over overlay content.
        let consumed = match pointer {
            Some(position) => {
                modal || (self.ui.hit_test(position).is_some() && ui_result.is_handled())
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
        let mut ui = Ui::new();
        ui.set_theme(self.theme);
        if let Some(measurer) = &self.measurer {
            ui.set_text_measurer(measurer.clone());
        }
        for entry in &mut self.entries {
            entry.root = Some(build_entry(entry, &mut ui, self.actions.clone()));
        }
        self.ui = ui;
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

fn is_self_or_ancestor(host: &Ui, target: NodeId, mut node: NodeId) -> bool {
    loop {
        if node == target {
            return true;
        }
        match host.tree().parent(node) {
            Some(parent) => node = parent,
            None => return false,
        }
    }
}

/// Builds one overlay's content and returns its root node.
fn build_entry(entry: &Entry, ui: &mut Ui, actions: Rc<RefCell<Vec<Action>>>) -> NodeId {
    let theme = ui.theme();
    let palette = theme.palette;
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
            let root = ui
                .add(
                    ui.root(),
                    Flex::column().gap(space::MD).padding(Edges::all(space::LG)),
                )
                .id();
            crate::detach(ui, root);
            ui.set_min_size(root, Size::new(CONFIRM_WIDTH, 0.0));
            ui.add_decor(root, surface_decor(surface));

            ui.add(
                root,
                Label::new(title.clone())
                    .font_size(TextSize::Heading.px())
                    .color(palette.foreground),
            );
            ui.add(
                root,
                Label::new(message.clone())
                    .font_size(TextSize::Body.px())
                    .color(palette.muted),
            );

            let row = ui
                .add(
                    root,
                    Flex::row()
                        .align(Align::Center)
                        .justify(Justify::End)
                        .gap(space::SM)
                        .padding(Edges::ZERO),
                )
                .id();
            let id = entry.id;
            let cancel_actions = actions.clone();
            ui.add(
                row,
                Button::ghost(cancel.clone())
                    .on_click(move || cancel_actions.borrow_mut().push(Action::Cancel(id))),
            );
            let button = if *destructive {
                Button::destructive(confirm.clone())
            } else {
                Button::primary(confirm.clone())
            };
            let confirm_actions = actions;
            ui.add(
                row,
                button.on_click(move || confirm_actions.borrow_mut().push(Action::Confirm(id))),
            );
            root
        }
        Kind::Popover { title, content } => {
            let root = ui
                .add(
                    ui.root(),
                    Flex::column().gap(space::SM).padding(Edges::all(space::MD)),
                )
                .id();
            crate::detach(ui, root);
            ui.add_decor(root, surface_decor(surface));
            if let Some(title) = title {
                ui.add(
                    root,
                    Label::new(title.clone())
                        .font_size(TextSize::Subheading.px())
                        .color(palette.foreground),
                );
            }
            content(ui, root);
            root
        }
        Kind::Tips { text } => {
            let root = ui
                .add(
                    ui.root(),
                    Flex::row()
                        .padding(Edges::symmetric(space::SM, space::XS))
                        .gap(0.0),
                )
                .id();
            crate::detach(ui, root);
            ui.add_decor(
                root,
                surface_decor(
                    SurfaceStyle::new(palette.foreground.lerp(palette.background, 0.08))
                        .radius(radius::SM),
                ),
            );
            ui.set_mouse_filter(root, MouseFilter::Ignore);
            let label = ui
                .add(
                    root,
                    Label::new(text.clone())
                        .font_size(TextSize::Small.px())
                        .color(palette.background),
                )
                .id();
            ui.set_mouse_filter(label, MouseFilter::Ignore);
            root
        }
        Kind::Message { text, tone } => {
            let root = ui
                .add(
                    ui.root(),
                    Flex::row()
                        .align(Align::Center)
                        .padding(Edges::symmetric(space::MD, space::SM))
                        .gap(0.0),
                )
                .id();
            crate::detach(ui, root);
            ui.add_decor(
                root,
                surface_decor(
                    SurfaceStyle::new(theme.surface(SurfaceLevel::Floating))
                        .border(palette.border)
                        .radius(radius::MD),
                ),
            );
            ui.set_mouse_filter(root, MouseFilter::Ignore);
            let label = ui
                .add(
                    root,
                    Label::new(text.clone())
                        .font_size(TextSize::Small.px())
                        .color(tone.color(&theme)),
                )
                .id();
            ui.set_mouse_filter(label, MouseFilter::Ignore);
            root
        }
    }
}
