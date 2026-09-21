//! 应用状态：当前工具、画布视图参数，以及 Phase 2 引入的真实 [`Document`]。
//!
//! 这里只放**纯数据**，不依赖任何 UI 类型，保证 `app -> document` 的方向成立。
//!
//! 多文档（`documents: Vec<Document>` + `active_document`，见 §23）留到需要
//! 打开多个文件的阶段；现在一个 `document` 就够表达"创建文档"。

use crate::canvas::CanvasCamera;
use crate::document::{
    Color, Command, Document, History, PixelRegion, DEFAULT_HEIGHT, DEFAULT_WIDTH,
};

/// 当前激活的工具，与 `AGENTS.md` 的 MVP 列表一致。
///
/// 工具的实现（Brush engine、框选……）在后续 Phase；Phase 1/2 只做选择。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ActiveTool {
    Move,
    #[default]
    Brush,
    Eraser,
    RectangleSelect,
    Eyedropper,
}

impl ActiveTool {
    /// 工具栏从上到下的顺序。
    pub const ALL: [ActiveTool; 5] = [
        ActiveTool::Move,
        ActiveTool::Brush,
        ActiveTool::Eraser,
        ActiveTool::RectangleSelect,
        ActiveTool::Eyedropper,
    ];

    /// 工具栏上的短标签。
    pub const fn short_label(self) -> &'static str {
        match self {
            Self::Move => "移动",
            Self::Brush => "画笔",
            Self::Eraser => "橡皮",
            Self::RectangleSelect => "框选",
            Self::Eyedropper => "吸管",
        }
    }

    /// Lucide 图标名（`crate::icons`），对应工具栏上的图标。
    pub const fn icon(self) -> &'static str {
        match self {
            Self::Move => "mouse-pointer-2",
            Self::Brush => "brush",
            Self::Eraser => "eraser",
            Self::RectangleSelect => "square-dashed",
            Self::Eyedropper => "pipette",
        }
    }

    /// 状态栏上的完整名字。
    pub const fn label(self) -> &'static str {
        match self {
            Self::Move => "移动工具",
            Self::Brush => "画笔工具",
            Self::Eraser => "橡皮擦",
            Self::RectangleSelect => "矩形选择",
            Self::Eyedropper => "吸管",
        }
    }
}

/// 撤销 / 重做的两个方向（工具栏按钮与快捷键共用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoryAction {
    Undo,
    Redo,
}

impl HistoryAction {
    pub const ALL: [HistoryAction; 2] = [HistoryAction::Undo, HistoryAction::Redo];

    /// 状态栏 / 提示里的方向名。
    pub const fn label(self) -> &'static str {
        match self {
            Self::Undo => "撤销",
            Self::Redo => "重做",
        }
    }

    /// Lucide 图标名（`crate::icons`）。
    pub const fn icon(self) -> &'static str {
        match self {
            Self::Undo => "undo-2",
            Self::Redo => "redo-2",
        }
    }
}

/// 编辑器状态。
#[derive(Debug, Clone, PartialEq)]
pub struct AppState {
    pub active_tool: ActiveTool,
    /// 画布相机（缩放 / 平移）。Phase 3 起真正参与坐标转换与渲染。
    pub canvas: CanvasCamera,
    /// 当前文档。`AppState::default()` 会创建一个 128×128 文档（像素图测试用）。
    pub document: Document,
    /// 撤销 / 重做栈（Phase 6）。属于编辑会话，不属于文档数据。
    pub history: History,
    /// 前景色（画笔颜色；吸管取色写这里）。
    pub foreground: Color,
    /// 背景色（吸管/填充以后用）。
    pub background: Color,
    /// 当前矩形选区（Phase 8）。`None` = 无选区；画笔会裁剪到选区里。
    pub selection: Option<PixelRegion>,
}

impl AppState {
    /// 用「新建窗口」选出来的尺寸 / 背景色开一个全新的编辑会话（空历史）。
    pub fn with_document(width: u32, height: u32, background: Color) -> Self {
        Self {
            active_tool: ActiveTool::default(),
            canvas: CanvasCamera::default(),
            document: Document::with_background("未命名", width, height, background),
            history: History::new(),
            foreground: Color::BLACK,
            background: Color::WHITE,
            selection: None,
        }
    }

    /// 撤销一步，返回命令名字；没有可撤销的返回 `None`。
    pub fn undo(&mut self) -> Option<&'static str> {
        let Self {
            history, document, ..
        } = self;
        history.undo(document)
    }

    /// 重做一步，返回命令名字；没有可重做的返回 `None`。
    pub fn redo(&mut self) -> Option<&'static str> {
        let Self {
            history, document, ..
        } = self;
        history.redo(document)
    }

    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    /// 执行一条命令并压入撤销栈（图层的增删 / 可见性 / 移动等走它）。
    pub fn execute(&mut self, command: Box<dyn Command>) {
        let AppState {
            document, history, ..
        } = self;
        history.execute(command, document);
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            active_tool: ActiveTool::default(),
            canvas: CanvasCamera::default(),
            document: Document::new("未命名", DEFAULT_WIDTH, DEFAULT_HEIGHT),
            history: History::new(),
            foreground: Color::BLACK,
            background: Color::WHITE,
            selection: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_tool_is_brush_and_tools_have_labels() {
        let state = AppState::default();
        assert_eq!(state.active_tool, ActiveTool::Brush);
        assert_eq!(state.active_tool.label(), "画笔工具");
        assert_eq!(state.active_tool.short_label(), "画笔");
        assert_eq!(ActiveTool::ALL.len(), 5);
    }

    #[test]
    fn every_tool_has_a_distinct_short_label() {
        let mut labels: Vec<&str> = ActiveTool::ALL.iter().map(|t| t.short_label()).collect();
        labels.sort_unstable();
        labels.dedup();
        assert_eq!(labels.len(), ActiveTool::ALL.len());
    }

    #[test]
    fn the_default_state_creates_a_128_by_128_document_and_identity_camera() {
        let state = AppState::default();
        assert_eq!((state.document.width, state.document.height), (128, 128));
        assert_eq!(state.document.name, "未命名");
        assert_eq!(state.canvas.zoom, 1.0);
        assert_eq!(state.foreground, Color::BLACK);
        assert_eq!(state.background, Color::WHITE);
    }

    #[test]
    fn the_default_state_has_an_empty_history() {
        let mut state = AppState::default();
        assert!(!state.can_undo());
        assert!(!state.can_redo());
        assert_eq!(state.undo(), None);
        assert_eq!(state.redo(), None);
    }

    #[test]
    fn history_actions_have_labels() {
        assert_eq!(HistoryAction::Undo.label(), "撤销");
        assert_eq!(HistoryAction::Redo.label(), "重做");
        assert_eq!(HistoryAction::ALL.len(), 2);
    }
}
