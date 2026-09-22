#include "gallery.hpp"

namespace cppffi {

namespace {

Color lerp(Color a, Color b, float t) {
    return Color{
        a.r + (b.r - a.r) * t,
        a.g + (b.g - a.g) * t,
        a.b + (b.b - a.b) * t,
        a.a + (b.a - a.a) * t,
    };
}

Color with_alpha(Color color, float alpha) {
    color.a = alpha;
    return color;
}

constexpr Color kTransparent{0.0f, 0.0f, 0.0f, 0.0f};
constexpr Color kBlack{0.0f, 0.0f, 0.0f, 1.0f};

}  // namespace

void paint_button(Canvas& canvas, Rect rect, ButtonVariant variant, ButtonState state) {
    const Palette& p = palette();
    Color fill = kTransparent;
    bool bordered = false;
    switch (variant) {
        case ButtonVariant::Primary:
            fill = state.pressed ? lerp(p.accent, kBlack, 0.12f)
                   : state.hovered ? lerp(p.accent, p.foreground, 0.10f)
                                   : p.accent;
            break;
        case ButtonVariant::Secondary:
            fill = (state.hovered || state.pressed) ? p.surface_hover : p.surface_raised;
            bordered = true;
            break;
        case ButtonVariant::Ghost:
            fill = (state.hovered || state.pressed) ? p.surface_hover : kTransparent;
            break;
        case ButtonVariant::Destructive:
            fill = state.pressed ? lerp(p.error, kBlack, 0.12f)
                   : state.hovered ? lerp(p.error, p.foreground, 0.10f)
                                   : p.error;
            break;
    }
    const bool on_fill =
        variant == ButtonVariant::Primary || variant == ButtonVariant::Destructive;
    const Color foreground = on_fill ? p.on_accent : p.foreground;

    canvas.fill_rounded(rect, radius::MD, fill);
    if (bordered) {
        canvas.stroke_rounded(rect, radius::MD, border::HAIRLINE, p.border);
    }
    // A label stand-in: two bars, since ABI v1 has no text.
    const Vec2 c = rect.center();
    canvas.fill_rounded({c.x - 10.0f, c.y - 5.0f, 20.0f, 3.0f}, 1.5f, foreground);
    canvas.fill_rounded({c.x - 10.0f, c.y + 2.0f, 13.0f, 3.0f}, 1.5f, foreground);
}

void paint_checkbox(Canvas& canvas, Rect box, bool checked) {
    const Palette& p = palette();
    const Color fill = checked ? p.accent : p.background;
    const Color edge = checked ? p.accent : p.border;
    canvas.fill_rounded(box, radius::SM, fill);
    canvas.stroke_rounded(box, radius::SM, border::HAIRLINE, edge);
    if (!checked) {
        return;
    }
    const Rect inner{box.x + 2.0f, box.y + 2.0f, box.w - 4.0f, box.h - 4.0f};
    const Vec2 a{inner.x + inner.w * 0.12f, inner.y + inner.h * 0.52f};
    const Vec2 b{inner.x + inner.w * 0.38f, inner.y + inner.h * 0.80f};
    const Vec2 c{inner.x + inner.w * 0.88f, inner.y + inner.h * 0.18f};
    canvas.line(a, b, p.on_accent, 1.8f);
    canvas.line(b, c, p.on_accent, 1.8f);
}

void paint_switch(Canvas& canvas, Rect track, bool on) {
    const Palette& p = palette();
    const float corner = track.h * 0.5f;
    canvas.fill_rounded(track, corner, on ? p.accent : p.surface_raised);
    canvas.stroke_rounded(track, corner, border::HAIRLINE, on ? p.accent : p.border);
    const float knob_radius = 6.5f;
    const float inset = 1.5f;
    const float cx =
        on ? track.right() - knob_radius - inset : track.x + knob_radius + inset;
    canvas.fill_circle({cx, track.center().y}, knob_radius, on ? p.on_accent : p.muted);
}

void paint_card(Canvas& canvas, Rect rect, bool bordered) {
    canvas.fill_rounded(rect, radius::LG, palette().surface_raised);
    if (bordered) {
        canvas.stroke_rounded(rect, radius::LG, border::HAIRLINE, palette().border);
    }
}

void paint_divider(Canvas& canvas, Rect rect) {
    canvas.line({rect.x, rect.center().y}, {rect.right(), rect.center().y},
                palette().border_subtle, border::HAIRLINE);
}

void paint_badge(Canvas& canvas, Rect rect, Color tone, bool solid) {
    if (solid) {
        canvas.fill_rounded(rect, radius::SM, tone);
    } else {
        canvas.fill_rounded(rect, radius::SM, with_alpha(tone, 0.12f));
        canvas.stroke_rounded(rect, radius::SM, border::HAIRLINE, with_alpha(tone, 0.30f));
    }
    const Color foreground = solid ? palette().on_accent : tone;
    canvas.fill_rounded({rect.x + space::SM, rect.center().y - 1.5f, rect.w - 2.0f * space::SM,
                         3.0f},
                        1.5f, foreground);
}

void build_gallery(Canvas& canvas, Rect bounds) {
    const Palette& p = palette();
    const float x = bounds.x + space::LG;
    const float w = bounds.w - 2.0f * space::LG;
    const float pad = space::LG;
    float y = bounds.y + space::LG;

    // Buttons: the four variants, then the hover / pressed states.
    {
        const Rect card{x, y, w, pad * 2.0f + control::HEIGHT + space::MD + control::HEIGHT};
        paint_card(canvas, card);
        const float bx = card.x + pad;
        const float bw = 96.0f;
        const float gap = space::SM;
        float by = card.y + pad;
        paint_button(canvas, {bx, by, bw, control::HEIGHT}, ButtonVariant::Primary);
        paint_button(canvas, {bx + (bw + gap), by, bw, control::HEIGHT},
                     ButtonVariant::Secondary);
        paint_button(canvas, {bx + 2.0f * (bw + gap), by, bw, control::HEIGHT},
                     ButtonVariant::Ghost);
        paint_button(canvas, {bx + 3.0f * (bw + gap), by, bw, control::HEIGHT},
                     ButtonVariant::Destructive);
        by += control::HEIGHT + space::MD;
        paint_button(canvas, {bx, by, bw, control::HEIGHT}, ButtonVariant::Secondary,
                     {true, false});
        paint_button(canvas, {bx + (bw + gap), by, bw, control::HEIGHT}, ButtonVariant::Primary,
                     {false, true});
        paint_button(canvas, {bx + 2.0f * (bw + gap), by, bw, control::HEIGHT},
                     ButtonVariant::Destructive, {true, false});
        y = card.bottom() + space::MD;
    }

    // Selection: checkbox off/on, switch off/on.
    {
        const Rect card{x, y, w, pad * 2.0f + control::ROW};
        paint_card(canvas, card);
        const float cy = card.center().y;
        float cx = card.x + pad;
        const float box = 16.0f;
        paint_checkbox(canvas, {cx, cy - box * 0.5f, box, box}, false);
        cx += box + space::XXL;
        paint_checkbox(canvas, {cx, cy - box * 0.5f, box, box}, true);
        cx += box + space::XXL;
        const float tw = 34.0f;
        const float th = 18.0f;
        paint_switch(canvas, {cx, cy - th * 0.5f, tw, th}, false);
        cx += tw + space::XXL;
        paint_switch(canvas, {cx, cy - th * 0.5f, tw, th}, true);
        y = card.bottom() + space::MD;
    }

    // Badges and a divider.
    {
        const float bh = 24.0f;
        const Rect card{x, y, w, pad * 2.0f + bh + space::MD + border::HAIRLINE};
        paint_card(canvas, card);
        const float by = card.y + pad;
        float bx = card.x + pad;
        paint_badge(canvas, {bx, by, 72.0f, bh}, p.accent, false);
        bx += 72.0f + space::SM;
        paint_badge(canvas, {bx, by, 72.0f, bh}, p.success, false);
        bx += 72.0f + space::SM;
        paint_badge(canvas, {bx, by, 72.0f, bh}, p.error, true);
        paint_divider(canvas, {card.x + pad, by + bh + space::MD, w - 2.0f * pad,
                               border::HAIRLINE});
    }
}

}  // namespace cppffi
