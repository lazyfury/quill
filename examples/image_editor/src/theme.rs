//! 编辑器自己的主题：设计系统的调色板 + **紧凑密度**。
//!
//! 这是一次 token 替换，不是第二条代码路径 —— 组件库的间距 / 控件高度都从
//! [`Theme`] 读，所以这里只换 [`Density`]：更小的 padding、默认 mini 控件。
//! 想再调紧 / 调松，改这一个函数即可，所有组件跟着变。

use draw_theme::{Density, Theme};

/// 编辑器主题。`light` 选浅色外观，密度固定为 [`Density::COMPACT`]。
pub fn editor_theme(light: bool) -> Theme {
    let base = if light { Theme::light() } else { Theme::dark() };
    base.with_density(Density::COMPACT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use draw_theme::{ControlSize, Space};

    #[test]
    fn the_editor_theme_is_compact_and_keeps_the_mode() {
        let dark = editor_theme(false);
        assert!(dark.is_dark());
        assert_eq!(dark.default_control(), ControlSize::Mini);
        // Compact scales spacing down from the documented 12px `MD` step.
        assert!(dark.spacing(Space::MD) < 12.0);
        assert!(editor_theme(true).mode.is_light());
    }
}
