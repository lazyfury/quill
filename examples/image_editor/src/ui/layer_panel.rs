//! 图层面板：`Document` 图层栈的增删改排序 + 可见性 / 不透明度。
//!
//! 列表用 `draw_components::List`（虚拟化）—— 但图层数通常很少，用它的理由
//! 是**行内容会整体重绑**：文档改动后 `ListState::invalidate` + `sync` 就能
//! 刷新眼睛、名字和不透明度，不必手搓动态子树。
//!
//! 顶部的眼睛 / 不透明度是列表的列；**操作按钮作用于“当前图层”**（先在列表里
//! 选中一行）。这是刻意的：`List` 的行是文本单元，没有逐格点击，把动作放到
//! 选中对象上既符合编辑器习惯，也不用给列表加第二套交互。
//!
//! 所有按钮回调只写共享的 [`AppState`]（`Document` 的变更会让 `revision`
//! 前进），由 [`EditorView::update`](crate::ui::EditorView) 统一重合成与刷新。

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use draw_components::{Button, Column, Component, Divider, Flex, List, ListColumn, Text};
use draw_core::Edges;
use draw_theme::{space, TextSize, Theme, Tone};
use draw_ui::MouseFilter;

use crate::app::state::AppState;
use crate::document::{
    AddLayerCommand, Document, Layer, LayerMetaCommand, PixelBuffer, RemoveLayerCommand,
};
use crate::ui::card::Card;

/// 图层行高（逻辑像素）。列表的池大小按它算。
pub const LAYER_ROW_HEIGHT: f32 = 28.0;

/// 图层列表：第 0 行是**最上面的图层**（跟 Photoshop 一致），所以数据下标
/// 要翻转。眼睛 / 名字 / 不透明度三列。
pub fn layer_list(
    theme: Theme,
    state: Rc<RefCell<AppState>>,
    count: Rc<Cell<usize>>,
    selected: Rc<Cell<Option<usize>>>,
) -> List {
    let source = {
        let state = state.clone();
        move |index: usize| {
            let state = state.borrow();
            let layers = &state.document.layers;
            let Some(layer) = layers.get(layers.len().wrapping_sub(1 + index)) else {
                return Vec::new();
            };
            let eye = if layer.visible { "◉" } else { "○" };
            vec![
                eye.to_string(),
                layer.name.clone(),
                format!("{:.0}%", layer.opacity * 100.0),
            ]
        }
    };

    let activate = {
        let state = state.clone();
        move |index: usize| {
            let mut state = state.borrow_mut();
            let len = state.document.layers.len();
            if let Some(id) = state
                .document
                .layers
                .get(len.wrapping_sub(1 + index))
                .map(|layer| layer.id)
            {
                state.document.select_layer(id);
            }
        }
    };

    List::new(theme, LAYER_ROW_HEIGHT, source)
        .columns(vec![
            ListColumn::fixed(22.0),
            ListColumn::flexible(),
            ListColumn::fixed(44.0).tone(Tone::Muted),
        ])
        .count(count)
        .selected(selected)
        .on_activate(activate)
        .grow(1.0)
}

/// 完整图层面板：标题 + 列表 + 操作按钮。`list` 由 [`EditorView`] 构建，
/// 因为它需要 `ListState` 句柄。
///
/// [`EditorView`]: crate::ui::EditorView
pub fn layer_panel(
    theme: Theme,
    state: Rc<RefCell<AppState>>,
    list: List,
    rename_request: Rc<Cell<bool>>,
) -> impl Component {
    Card::new(theme)
        .gap(space::SM)
        .grow(1.0)
        .child(Text::subheading("图层", theme))
        .child(Divider::horizontal(theme))
        .child(list)
        .child(Divider::horizontal(theme))
        .child(action_buttons(theme, state, rename_request))
}

/// 两排按钮，全部作用于“当前图层”。所有动作都走 [`AppState::execute`]，所以可撤销。
fn action_buttons(
    theme: Theme,
    state: Rc<RefCell<AppState>>,
    rename_request: Rc<Cell<bool>>,
) -> impl Component {
    let add = {
        let state = state.clone();
        button(theme, "+ 图层", move || {
            let mut state = state.borrow_mut();
            let index = state.document.layers.len();
            let before_active = state.document.active_layer;
            let (width, height) = (state.document.width, state.document.height);
            let layer = Layer::new(
                format!("图层 {}", index + 1),
                PixelBuffer::new(width, height),
            );
            state.execute(Box::new(AddLayerCommand::new(layer, index, before_active)));
        })
    };
    let delete = {
        let state = state.clone();
        button(theme, "− 删除", move || {
            let mut state = state.borrow_mut();
            if let Some(id) = state.document.active_layer {
                state.execute(Box::new(RemoveLayerCommand::new(id, Some(id))));
            }
        })
    };
    let rename = {
        let rename_request = rename_request.clone();
        button(theme, "重命名", move || rename_request.set(true))
    };
    let toggle = {
        let state = state.clone();
        button(theme, "显示/隐藏", move || {
            let mut state = state.borrow_mut();
            let Some(id) = state.document.active_layer else {
                return;
            };
            let visible = state
                .document
                .layer(id)
                .map(|layer| layer.visible)
                .unwrap_or(true);
            meta_edit(&mut state, "显示/隐藏", |document| {
                document.set_layer_visible(id, !visible);
            });
        })
    };
    let less = {
        let state = state.clone();
        button(theme, "−", move || {
            let mut state = state.borrow_mut();
            let Some(id) = state.document.active_layer else {
                return;
            };
            let opacity = state
                .document
                .layer(id)
                .map(|layer| layer.opacity)
                .unwrap_or(1.0);
            meta_edit(&mut state, "不透明度", |document| {
                document.set_layer_opacity(id, opacity - 0.1);
            });
        })
    };
    let more = {
        let state = state.clone();
        button(theme, "+", move || {
            let mut state = state.borrow_mut();
            let Some(id) = state.document.active_layer else {
                return;
            };
            let opacity = state
                .document
                .layer(id)
                .map(|layer| layer.opacity)
                .unwrap_or(1.0);
            meta_edit(&mut state, "不透明度", |document| {
                document.set_layer_opacity(id, opacity + 0.1);
            });
        })
    };
    let up = {
        let state = state.clone();
        button(theme, "↑", move || {
            let mut state = state.borrow_mut();
            let Some(id) = state.document.active_layer else {
                return;
            };
            if let Some(index) = state.document.layer_index(id) {
                meta_edit(&mut state, "上移一层", |document| {
                    document.move_layer(id, index + 1);
                });
            }
        })
    };
    let down = {
        let state = state.clone();
        button(theme, "↓", move || {
            let mut state = state.borrow_mut();
            let Some(id) = state.document.active_layer else {
                return;
            };
            if let Some(index) = state.document.layer_index(id) {
                meta_edit(&mut state, "下移一层", |document| {
                    document.move_layer(id, index.saturating_sub(1));
                });
            }
        })
    };

    Column::new()
        .gap(space::XXS)
        .child(
            Flex::row()
                .gap(space::XXS)
                .padding(Edges::ZERO)
                .mouse_filter(MouseFilter::Ignore)
                .child(add)
                .child(delete)
                .child(rename),
        )
        .child(
            Flex::row()
                .gap(space::XXS)
                .padding(Edges::ZERO)
                .mouse_filter(MouseFilter::Ignore)
                .child(toggle)
                .child(less)
                .child(more)
                .child(up)
                .child(down),
        )
}

/// 做一次“只改图层元数据”的编辑，并压入一步可撤销的命令。
fn meta_edit(state: &mut AppState, label: &'static str, mutate: impl FnOnce(&mut Document)) {
    let before = LayerMetaCommand::capture(&state.document);
    mutate(&mut state.document);
    let after = LayerMetaCommand::capture(&state.document);
    state.execute(Box::new(LayerMetaCommand::new(before, after, label)));
}

fn button(theme: Theme, label: &'static str, action: impl FnMut() + 'static) -> Button {
    Button::secondary(label, theme)
        .font_size(TextSize::Small.px())
        .on_click(action)
}
