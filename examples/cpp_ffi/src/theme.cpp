#include "theme.hpp"

namespace cppffi {

namespace {

constexpr Color c8(unsigned r, unsigned g, unsigned b, unsigned a = 255) {
    return Color{static_cast<float>(r) / 255.0f, static_cast<float>(g) / 255.0f,
                 static_cast<float>(b) / 255.0f, static_cast<float>(a) / 255.0f};
}

// draw_theme::Palette::dark() — keep in sync with palette.rs.
constexpr Color kAccent = c8(0x3B, 0x82, 0xF6);

const Palette kDark = {
    /*background   */ c8(0x0A, 0x0A, 0x0A),
    /*foreground   */ c8(0xF5, 0xF5, 0xF5),
    /*surface      */ c8(0x11, 0x11, 0x11),
    /*surface_raised*/ c8(0x17, 0x17, 0x17),
    /*surface_hover*/ c8(0x1C, 0x1C, 0x1C),
    /*muted        */ c8(0xA3, 0xA3, 0xA3),
    /*subtle       */ c8(0x73, 0x73, 0x73),
    /*border       */ c8(0x26, 0x26, 0x26),
    /*border_subtle*/ c8(0x1F, 0x1F, 0x1F),
    /*code_surface */ c8(0x11, 0x11, 0x11),
    /*accent       */ kAccent,
    /*success      */ c8(0x22, 0xC5, 0x5E),
    /*warning      */ c8(0xF5, 0x9E, 0x0B),
    /*error        */ c8(0xEF, 0x44, 0x44),
    /*info         */ c8(0x3B, 0x82, 0xF6),
    /*on_accent    */ c8(0x0A, 0x0A, 0x0A),
    /*focus_ring   */ Color{kAccent.r, kAccent.g, kAccent.b, 0.55f},
    /*selection    */ Color{kAccent.r, kAccent.g, kAccent.b, 0.18f},
};

}  // namespace

const Palette& palette() { return kDark; }

}  // namespace cppffi
