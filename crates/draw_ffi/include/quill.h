/*
 * quill.h — C ABI over the quill core (`draw_ffi`).
 *
 * A foreign-language host builds its own scene/UI and uses this header to fill
 * a backend-neutral `DrawList`, then reads it back command by command and
 * rasterizes it with its own backend (the C++ `examples/cpp_ffi` OpenGL
 * renderer). Nothing from `draw_scene` / `draw_ui` crosses this boundary.
 *
 * The layout here is a hand-maintained mirror of `crates/draw_ffi/src/lib.rs`.
 * Every type is `#[repr(C)]` on the Rust side and made of `float` / a C enum /
 * an opaque pointer, so the two agree. If you change one, change the other and
 * bump `QUILL_ABI_VERSION`; a host should refuse to run when
 * `quill_abi_version()` disagrees with this constant.
 */
#ifndef QUILL_H
#define QUILL_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define QUILL_ABI_VERSION 1u

/* An opaque, Rust-owned draw list. Never inspect it; use the functions below. */
typedef struct QuillDrawList QuillDrawList;

/* Command discriminator. Discriminants are part of the ABI. */
typedef enum QuillCommandTag {
    QUILL_CMD_SAVE = 0,
    QUILL_CMD_RESTORE = 1,
    QUILL_CMD_SET_TRANSFORM = 2,
    QUILL_CMD_SET_OPACITY = 3,
    QUILL_CMD_CLIP_RECT = 4,
    QUILL_CMD_FILL_RECT = 5,
    QUILL_CMD_STROKE_RECT = 6,
    QUILL_CMD_LINE = 7,
    QUILL_CMD_FILL_CIRCLE = 8,
    QUILL_CMD_STROKE_CIRCLE = 9,
    QUILL_CMD_FILL_ROUNDED_RECT = 10,
    QUILL_CMD_STROKE_ROUNDED_RECT = 11,
    /* DrawImage / DrawText are outside ABI v1. A record with this tag is
     * otherwise zeroed and should be skipped. */
    QUILL_CMD_UNSUPPORTED = 12
} QuillCommandTag;

typedef struct QuillVec2 {
    float x;
    float y;
} QuillVec2;

typedef struct QuillRect {
    float x;
    float y;
    float width;
    float height;
} QuillRect;

typedef struct QuillColor {
    float r;
    float g;
    float b;
    float a;
} QuillColor;

/* Two basis axes plus an origin. A point maps to
 * x_axis * p.x + y_axis * p.y + origin. */
typedef struct QuillTransform {
    QuillVec2 x_axis;
    QuillVec2 y_axis;
    QuillVec2 origin;
} QuillTransform;

/* Clockwise from the top-left, in logical pixels. */
typedef struct QuillCornerRadii {
    float top_left;
    float top_right;
    float bottom_right;
    float bottom_left;
} QuillCornerRadii;

typedef struct QuillPaint {
    QuillColor color;
} QuillPaint;

/* One command, flattened to a fixed layout. Only the fields named by `tag`
 * are meaningful; every other field is zeroed. The mapping is:
 *
 *   SAVE / RESTORE            no fields
 *   SET_TRANSFORM             transform
 *   SET_OPACITY               opacity
 *   CLIP_RECT                 rect
 *   FILL_RECT                 rect, paint
 *   STROKE_RECT               rect, paint, width
 *   LINE                      from, to, paint, width
 *   FILL_CIRCLE               center, radius, paint
 *   STROKE_CIRCLE             center, radius, paint, width
 *   FILL_ROUNDED_RECT         rect, corners, paint
 *   STROKE_ROUNDED_RECT       rect, corners, paint, width
 */
typedef struct QuillCommand {
    QuillCommandTag tag;
    QuillTransform transform;
    QuillRect rect;
    QuillCornerRadii corners;
    QuillVec2 from;
    QuillVec2 to;
    QuillVec2 center;
    QuillPaint paint;
    float radius;
    float width;
    float opacity;
} QuillCommand;

/* Returns QUILL_ABI_VERSION. */
uint32_t quill_abi_version(void);

/* Allocate an empty list. Release it with quill_draw_list_free. */
QuillDrawList *quill_draw_list_new(void);

/* Free a list. A null pointer is a no-op. */
void quill_draw_list_free(QuillDrawList *list);

/* Remove every command, keeping the allocation. */
void quill_draw_list_clear(QuillDrawList *list);

/* Number of commands; 0 for a null list. */
size_t quill_draw_list_len(const QuillDrawList *list);

/* Read one command. Out of bounds (or a null list) yields
 * QUILL_CMD_UNSUPPORTED with every field zeroed. */
QuillCommand quill_draw_list_command(const QuillDrawList *list, size_t index);

/* State commands. */
void quill_draw_list_save(QuillDrawList *list);
void quill_draw_list_restore(QuillDrawList *list);
void quill_draw_list_set_transform(QuillDrawList *list, QuillTransform transform);
void quill_draw_list_set_opacity(QuillDrawList *list, float opacity);
void quill_draw_list_clip_rect(QuillDrawList *list, QuillRect rect);

/* Geometry commands. */
void quill_draw_list_fill_rect(QuillDrawList *list, QuillRect rect, QuillPaint paint);
void quill_draw_list_stroke_rect(QuillDrawList *list, QuillRect rect, QuillPaint paint, float width);
void quill_draw_list_line(QuillDrawList *list, QuillVec2 from, QuillVec2 to, QuillPaint paint, float width);
void quill_draw_list_fill_circle(QuillDrawList *list, QuillVec2 center, float radius, QuillPaint paint);
void quill_draw_list_stroke_circle(QuillDrawList *list, QuillVec2 center, float radius, QuillPaint paint, float width);
void quill_draw_list_fill_rounded_rect(QuillDrawList *list, QuillRect rect, QuillCornerRadii corners, QuillPaint paint);
void quill_draw_list_stroke_rounded_rect(QuillDrawList *list, QuillRect rect, QuillCornerRadii corners, QuillPaint paint, float width);

#ifdef __cplusplus
} /* extern "C" */
#endif

#endif /* QUILL_H */
