#include "canvas.hpp"

namespace cppffi {

QuillVec2 to_quill(Vec2 v) { return QuillVec2{v.x, v.y}; }

QuillRect to_quill(Rect r) { return QuillRect{r.x, r.y, r.w, r.h}; }

QuillColor to_quill(Color c) { return QuillColor{c.r, c.g, c.b, c.a}; }

QuillPaint to_quill_paint(Color c) { return QuillPaint{to_quill(c)}; }

QuillTransform rotation(float angle, Vec2 origin) {
    const float s = std::sin(angle);
    const float c = std::cos(angle);
    // p' = R * (p - origin) + origin = R * p + (origin - R * origin).
    const float rx = c * origin.x - s * origin.y;
    const float ry = s * origin.x + c * origin.y;
    return QuillTransform{
        QuillVec2{c, s},
        QuillVec2{-s, c},
        QuillVec2{origin.x - rx, origin.y - ry},
    };
}

void Canvas::save() { quill_draw_list_save(list_); }
void Canvas::restore() { quill_draw_list_restore(list_); }

void Canvas::set_transform(QuillTransform transform) {
    quill_draw_list_set_transform(list_, transform);
}

void Canvas::set_opacity(float opacity) { quill_draw_list_set_opacity(list_, opacity); }

void Canvas::clip(Rect rect) { quill_draw_list_clip_rect(list_, to_quill(rect)); }

void Canvas::fill_rect(Rect rect, Color color) {
    quill_draw_list_fill_rect(list_, to_quill(rect), to_quill_paint(color));
}

void Canvas::stroke_rect(Rect rect, Color color, float width) {
    quill_draw_list_stroke_rect(list_, to_quill(rect), to_quill_paint(color), width);
}

void Canvas::line(Vec2 from, Vec2 to, Color color, float width) {
    quill_draw_list_line(list_, to_quill(from), to_quill(to), to_quill_paint(color), width);
}

void Canvas::fill_circle(Vec2 center, float radius, Color color) {
    quill_draw_list_fill_circle(list_, to_quill(center), radius, to_quill_paint(color));
}

void Canvas::stroke_circle(Vec2 center, float radius, Color color, float width) {
    quill_draw_list_stroke_circle(list_, to_quill(center), radius, to_quill_paint(color), width);
}

void Canvas::fill_rounded(Rect rect, float radius, Color color) {
    const QuillCornerRadii corners{radius, radius, radius, radius};
    quill_draw_list_fill_rounded_rect(list_, to_quill(rect), corners, to_quill_paint(color));
}

void Canvas::stroke_rounded(Rect rect, float radius, float width, Color color) {
    const QuillCornerRadii corners{radius, radius, radius, radius};
    quill_draw_list_stroke_rounded_rect(list_, to_quill(rect), corners, to_quill_paint(color),
                                        width);
}

}  // namespace cppffi
