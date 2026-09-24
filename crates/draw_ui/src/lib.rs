//! `draw_ui` — layout, paint and input for backend-neutral UI controls.
//!
//! This crate owns three stages of the pipeline:
//!
//! - **Layout** ([`layout()`]) resolves absolute rectangles from anchors/offsets;
//!   flex and grid containers size and arrange their children.
//! - **Paint** ([`paint`]) emits a backend-neutral `draw_render::DrawList`.
//! - **Input** ([`hit_test`] / [`handle_input`] / [`route_input`]) hit-tests
//!   controls and runs the `_input -> world -> GUI -> _unhandled_input` order.
//!
//! Control data ([`ControlData`], [`Widget`], [`NodeDecor`]) lives on the
//! [`SceneTree`] node's extension slot, and the text measurer / GUI interaction
//! state / layout cache live on the root node. The theme is a value passed to
//! component constructors. Building components lives in `draw_components`;
//! submitting the resulting `DrawList` to a backend is the host's job.
//!
//! ```ignore
//! use draw_ui as ui;
//!
//! ui::layout(&mut tree, viewport);
//! ui::paint(&tree, &mut ctx);
//! ```
//!
//! This crate never touches browser APIs, so all of the above is testable with
//! native `cargo test`.

/// Crate name, kept for lightweight smoke checks.
pub const CRATE: &str = "draw_ui";

mod control;
mod debug;
mod decor;
mod input;
pub mod layout;
mod paint;
mod ui;
mod widget;

pub use control::{
    ClickCallback, Control, ControlData, CursorProvider, DragCallback, DragPhase, GuiState,
    MouseFilter, PointerCallback, ScrollCallback, SecondaryCallback,
};
pub use debug::DebugDrawOptions;
pub use decor::{
    dynamic_surface_decor, foreground_decor, surface_decor, DecorRef, InteractState, NodeDecor,
};
pub use input::{
    focused, handle_input, hit_test, hovered, hovered_cursor, hovered_is_button, is_interactive,
    route_input,
};
pub use layout::{
    Align, AlignContent, ApproxTextMeasurer, ContentSize, FixedWidthTextMeasurer, FlexDirection,
    FlexStyle, GridPlacement, GridStyle, Justify, LayoutStyle, SizeBasis, TextMeasurer,
    TextOptions, Track, WordBreak,
};
pub use paint::{fill_rounded_rect, fill_rounded_rect_corners, inset, surface, SurfaceStyle};
pub use widget::{estimate_text_size, BoxLayout, ButtonData, ButtonState, Widget};

use std::rc::Rc;

use draw_core::{NodeId, Size, ViewportSize};
use draw_render::PaintContext;
use draw_scene::SceneTree;

use crate::control::{gui_state as read_gui_state, gui_state_mut as control_gui_state_mut};
use crate::ui::Ui;

// -- environment -------------------------------------------------------------

/// Replaces the text measurer (stored on the tree root).
pub fn set_text_measurer(tree: &mut SceneTree, measurer: Rc<dyn TextMeasurer>) {
    Ui.set_text_measurer(tree, measurer)
}

/// Forces the next [`layout()`] call to recompute the whole tree.
pub fn invalidate_layout(tree: &mut SceneTree) {
    Ui.invalidate_layout(tree)
}

/// Number of times the full measure/arrange pass has run.
pub fn layout_count(tree: &SceneTree) -> u64 {
    Ui.layout_count(tree)
}

/// Number of controls arranged during the last [`layout()`] pass.
pub fn last_arranged_nodes(tree: &SceneTree) -> usize {
    Ui.last_arranged_nodes(tree)
}

// -- control data ------------------------------------------------------------

/// Marks `id` (and its ancestors) as needing layout.
///
/// The construction layer (`draw_components`) calls this after mutating a control's
/// layout inputs directly through [`SceneTree::data_mut`].
pub fn mark_dirty(tree: &mut SceneTree, id: NodeId) {
    Ui.mark_dirty(tree, id)
}

/// Attaches themed chrome to `id`, painted by [`paint`] around the control's
/// own content.
pub fn add_decor(tree: &mut SceneTree, id: NodeId, decor: DecorRef) {
    Ui.add_decor(tree, id, decor)
}

/// Clips `id` (and everything below it) to its own rectangle.
///
/// Opt-in, and the only source of `DrawCommand::ClipRect` in the UI: `paint`
/// pushes the resolved clip once per clipped region and pops it again. The clip
/// is resolved from the layout rectangles, so a control whose intersection with
/// its clipping ancestors is empty is skipped entirely — painted nowhere, and
/// not hit-testable either.
///
/// **Non-breaking addition to `draw_ui`** (`ControlData` gained `clip` /
/// `clip_rect`); recorded in `docs/design-system.md`.
pub fn set_clip(tree: &mut SceneTree, id: NodeId, clip: bool) {
    Ui.set_clip(tree, id, clip)
}

/// Decorators attached to `id`, in paint order.
pub fn decor(tree: &SceneTree, id: NodeId) -> &[DecorRef] {
    Ui.decor(tree, id)
}

/// Hover/pressed/focused state of `id`, inherited from its ancestors.
pub fn state_for(tree: &SceneTree, id: NodeId) -> InteractState {
    Ui.state_for(tree, id)
}

/// Reads the viewport GUI interaction state, if initialized.
pub fn gui_state_of(tree: &SceneTree) -> Option<&GuiState> {
    read_gui_state(tree)
}

/// Mutably borrows the viewport GUI interaction state, creating it on first
/// use.
pub fn gui_state_mut(tree: &mut SceneTree) -> &mut GuiState {
    control_gui_state_mut(tree)
}

// -- queries -----------------------------------------------------------------

/// Layout data for `id`, read from the node's extension slot.
pub fn control(tree: &SceneTree, id: NodeId) -> Option<&ControlData> {
    Ui.control(tree, id)
}

/// The control's visual widget, read from the node's extension slot.
pub fn widget(tree: &SceneTree, id: NodeId) -> Option<&Widget> {
    Ui.widget(tree, id)
}

/// Number of controls in the UI.
pub fn control_count(tree: &SceneTree) -> usize {
    Ui.control_count(tree)
}

// -- traversal ---------------------------------------------------------------

/// Resolves every control's absolute rectangle against `viewport`.
pub fn layout(tree: &mut SceneTree, viewport: ViewportSize) {
    Ui.layout(tree, viewport)
}

/// The size the UI's content wants, given the space a parent can offer.
///
/// [`layout`] pins every UI root to the viewport, so a view always fills the
/// surface it was handed and nothing in the resolved rectangles says how much
/// room the content *wanted*. A host that sizes its window to its content — a
/// menu-bar panel, a popover — needs exactly that number, and it has to come
/// from the same [`TextMeasurer`] that will paint the frame.
///
/// Measurement is the first of layout's two passes and is pure: no painting,
/// no backend, no window. The result includes each root's own padding, so
/// offering `Size::new(width, f32::INFINITY)` reads as "how tall do you need to
/// be at this width". The measurement is cached per (node, available) pair, and
/// [`layout`] clears that cache, so asking does not disturb a later frame.
///
/// **Note** this is a non-breaking addition to `draw_ui` made for the host in
/// `examples/deepseek_balance`; see `docs/design-system.md`.
pub fn content_size(tree: &SceneTree, available: Size) -> ContentSize {
    Ui.content_size(tree, available)
}

/// Emits control visuals into `ctx` in draw order.
pub fn paint(tree: &SceneTree, ctx: &mut PaintContext) {
    Ui.paint(tree, ctx)
}

/// Draws debug bounds plus `name#id` labels for every visible control.
pub fn paint_debug(tree: &SceneTree, ctx: &mut PaintContext, options: &DebugDrawOptions) {
    Ui.paint_debug(tree, ctx, options)
}
