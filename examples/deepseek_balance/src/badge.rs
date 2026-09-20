//! 多窗口测试用的第二个窗口：一张贴在桌面右下角的无边框小卡片。
//!
//! 与主窗口/面板完全独立 —— 自己的 window、surface 和 backend，
//! 内容是静态文案（标题一行、余额一行），不接数据层：
//! 它存在的意义是验证「一个进程、一个事件循环、两个窗口」。

use draw_core::{Rect, Size, Vec2};
use draw_render::{PaintContext, TextAlign};
use draw_theme::Palette;

/// 徽章窗口的逻辑尺寸。
pub const BADGE_WIDTH: f32 = 190.0;
pub const BADGE_HEIGHT: f32 = 84.0;
/// 距桌面右下角的外边距，逻辑像素。
pub const SCREEN_MARGIN: f32 = 16.0;

/// 卡片内边距。
const PAD: f32 = 16.0;
/// 圆角半径。
const RADIUS: f32 = 14.0;

/// 标题行。
pub const TITLE: &str = "Deepseek";
/// 余额行 —— 多窗口测试用静态文案，不走数据层。
pub const BALANCE_LINE: &str = "余额：¥00.01";

/// 把徽章画进 `ctx`。`width × height` 是 surface 的逻辑尺寸。
pub fn paint(ctx: &mut PaintContext, width: f32, height: f32, palette: &Palette) {
    let rect = Rect::from_min_size(Vec2::ZERO, Size::new(width, height));
    // 透明窗口里只画这张圆角卡片，卡片外露出桌面。
    ctx.fill_rounded_rect(rect, RADIUS, palette.surface_raised);
    ctx.stroke_rounded_rect(rect, RADIUS, 1.0, palette.border);
    ctx.draw_text(
        TITLE,
        Vec2::new(PAD, 34.0),
        16.0,
        TextAlign::Left,
        palette.foreground,
    );
    ctx.draw_text(
        BALANCE_LINE,
        Vec2::new(PAD, 62.0),
        14.0,
        TextAlign::Left,
        palette.muted,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use draw_render::DrawCommand;
    use draw_theme::Theme;

    #[test]
    fn the_badge_paints_card_and_two_lines() {
        let mut ctx = PaintContext::new();
        paint(&mut ctx, BADGE_WIDTH, BADGE_HEIGHT, &Theme::dark().palette);
        let list = ctx.into_draw_list();
        // 卡片填充 + 描边 + 两行文字。
        assert_eq!(list.len(), 4, "one card, one border, two lines");

        let commands = list.commands();
        // 卡片铺满整个 surface：圆角之外靠透明清屏色露桌面。
        match &commands[0] {
            DrawCommand::FillRoundedRect { rect, corners, .. } => {
                assert_eq!(rect.size.width, BADGE_WIDTH);
                assert_eq!(rect.size.height, BADGE_HEIGHT);
                assert_eq!(corners.top_left, RADIUS);
            }
            other => panic!("expected the card fill first, got {other:?}"),
        }
        match &commands[1] {
            DrawCommand::StrokeRoundedRect { rect, width, .. } => {
                assert_eq!(rect.size.width, BADGE_WIDTH, "the border hugs the card");
                assert_eq!(*width, 1.0);
            }
            other => panic!("expected the card border second, got {other:?}"),
        }

        // 两行文字：都左对齐、贴同一左内边距，标题在余额上一行、字号更大。
        let lines: Vec<(String, f32, f32, TextAlign, f32)> = commands[2..]
            .iter()
            .map(|command| match command {
                DrawCommand::DrawText {
                    text,
                    position,
                    font_size,
                    align,
                    ..
                } => (text.clone(), position.x, position.y, *align, *font_size),
                other => panic!("expected text, got {other:?}"),
            })
            .collect();
        assert_eq!(lines[0].0, TITLE);
        assert_eq!(lines[1].0, BALANCE_LINE);
        assert_eq!(lines[0].1, PAD, "both lines share the left padding");
        assert_eq!(lines[1].1, PAD);
        assert!(lines[0].2 < lines[1].2, "the title sits on the line above");
        assert!(
            lines[0].4 > lines[1].4,
            "the title is the larger of the two"
        );
        for line in &lines {
            assert_eq!(line.3, TextAlign::Left);
            assert!(line.2 < BADGE_HEIGHT, "baselines stay inside the card");
        }
    }

    #[test]
    fn the_card_fits_the_declared_size() {
        // 无边框窗口的尺寸承诺：卡片必须装得下两行文字 + 上下内边距。
        assert!(BADGE_HEIGHT > PAD * 2.0 + 62.0 - 34.0);
        assert!(BADGE_WIDTH > PAD * 2.0);
    }
}
