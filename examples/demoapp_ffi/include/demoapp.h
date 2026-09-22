/*
 * demoapp.h — C ABI over the real demo_app gallery (`demoapp_ffi`).
 *
 * A foreign host creates a `DemoApp`, drives its frame, and reads the resulting
 * `DrawList` back with the `quill_draw_list_*` functions from `quill.h`. The
 * geometry, layout and colors are the Rust app's; `DrawText` commands have no
 * ABI v1 record and read back as `QUILL_CMD_UNSUPPORTED`.
 *
 * Layout mirrors `examples/demoapp_ffi/src/lib.rs`. A null handle is a no-op (or
 * a null list).
 */
#ifndef DEMOAPP_H
#define DEMOAPP_H

#include <stdint.h>

#include "quill.h"

#ifdef __cplusplus
extern "C" {
#endif

/* An opaque, Rust-owned DemoApp + viewport. */
typedef struct DemoAppHandle DemoAppHandle;

/* Dark theme. */
DemoAppHandle *demoapp_new(void);
/* Dark (light == 0) or light theme. */
DemoAppHandle *demoapp_new_with_mode(uint32_t light);
void demoapp_free(DemoAppHandle *handle);

/* Logical viewport for the next update/layout. */
void demoapp_set_viewport(DemoAppHandle *handle, float width, float height);
/* Select a catalog group (clamped). */
void demoapp_show_group(DemoAppHandle *handle, uint32_t index);
/* Number of catalog groups. */
uint32_t demoapp_group_count(void);

/* Drain per-frame requests / advance overlay timers. */
void demoapp_update(DemoAppHandle *handle, float dt);
/* Resolve UI layout for the current viewport. */
void demoapp_layout(DemoAppHandle *handle);

/* Paint the current frame into a fresh DrawList; release it with
 * quill_draw_list_free. Returns null for a null handle. */
QuillDrawList *demoapp_paint(const DemoAppHandle *handle);

#ifdef __cplusplus
} /* extern "C" */
#endif

#endif /* DEMOAPP_H */
