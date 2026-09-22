// The design tokens, mirrored from `draw_theme` so the C++ components match
// `demo_app` exactly.
//
// The values are copied from `crates/draw_theme/src/palette.rs` and
// `scale.rs` (the dark palette and the comfortable density). If you change a
// token there, change it here too — the point of the gallery is a like-for-like
// comparison with the Rust component library.

#pragma once

#include "canvas.hpp"

namespace cppffi {

/// `draw_theme::Palette` for the dark mode.
struct Palette {
    Color background;
    Color foreground;
    Color surface;
    Color surface_raised;
    Color surface_hover;
    Color muted;
    Color subtle;
    Color border;
    Color border_subtle;
    Color code_surface;
    Color accent;
    Color success;
    Color warning;
    Color error;
    Color info;
    Color on_accent;
    Color focus_ring;
    Color selection;
};

const Palette& palette();

/// `draw_theme::space` (base scale; the comfortable density is 1.0).
namespace space {
constexpr float XXXS = 2.0f;
constexpr float XXS = 4.0f;
constexpr float XS = 6.0f;
constexpr float SM = 8.0f;
constexpr float MD = 12.0f;
constexpr float LG = 16.0f;
constexpr float XL = 20.0f;
constexpr float XXL = 24.0f;
constexpr float XXXL = 32.0f;
}  // namespace space

/// `draw_theme::radius`.
namespace radius {
constexpr float NONE = 0.0f;
constexpr float SM = 4.0f;
constexpr float MD = 6.0f;
constexpr float LG = 8.0f;
constexpr float PANEL = 10.0f;
constexpr float FULL = 9999.0f;
}  // namespace radius

/// `draw_theme::border`.
namespace border {
constexpr float HAIRLINE = 1.0f;
constexpr float FOCUS = 1.5f;
}  // namespace border

/// `draw_theme::control` (comfortable density).
namespace control {
constexpr float HEIGHT = 36.0f;
constexpr float HEIGHT_SM = 32.0f;
constexpr float HEIGHT_LG = 40.0f;
constexpr float ICON_SIZE = 32.0f;
constexpr float PADDING_X = 12.0f;
constexpr float PADDING_Y = 8.0f;
constexpr float ICON = 16.0f;
constexpr float ROW = 36.0f;
constexpr float ROW_SM = 32.0f;
constexpr float TAB = 34.0f;
}  // namespace control

}  // namespace cppffi
