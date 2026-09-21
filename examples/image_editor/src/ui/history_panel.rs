//! 历史面板：用一条虚拟化 `List` 展示撤销 / 重做栈，点某一步就跳到那个状态。
//!
//! 行序是一条时间线：**撤销栈（最旧→最新）→ 「● 当前」→ 重做栈（下一个最前）**。
//! 点某一行：撤销栈的行就撤回到那一步，重做栈的行就向前重做；视图 [`update`]
//! 会检测到 document `revision` 变化并重合成。
//!
//! [`update`]: crate::ui::EditorView::update

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use draw_components::{Component, List, ListColumn};
use draw_theme::Theme;

use crate::app::state::AppState;

/// 历史行高（逻辑像素）。
pub const HISTORY_ROW_HEIGHT: f32 = 24.0;

/// 历史 `List`。`count` 是总行数（撤销 + 1 + 重做），由视图在历史变化时刷新。
pub fn history_list(theme: Theme, state: Rc<RefCell<AppState>>, count: Rc<Cell<usize>>) -> List {
    let source = {
        let state = state.clone();
        move |index: usize| {
            let state = state.borrow();
            let undo = state.history.undo_labels();
            let redo = state.history.redo_labels();
            let undo_len = undo.len();
            if index < undo_len {
                vec![undo[index].to_string()]
            } else if index == undo_len {
                vec!["● 当前".to_string()]
            } else {
                redo.get(index - undo_len - 1)
                    .map(|label| vec![label.to_string()])
                    .unwrap_or_default()
            }
        }
    };

    let activate = {
        let state = state.clone();
        move |index: usize| {
            let mut state = state.borrow_mut();
            let undo_len = state.history.undo_len();
            if index + 1 < undo_len {
                // 撤回到第 index+1 步。
                while state.history.undo_len() > index + 1 {
                    state.undo();
                }
            } else if index > undo_len {
                for _ in 0..(index - undo_len) {
                    state.redo();
                }
            }
        }
    };

    List::new(theme, HISTORY_ROW_HEIGHT, source)
        .columns(vec![ListColumn::flexible()])
        .count(count)
        .on_activate(activate)
        .grow(1.0)
}
