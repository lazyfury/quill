// A C++ OpenGL 3.3 backend for the quill `DrawList`.
//
// It is the host's own `RenderBackend`: transform, opacity and clip are
// resolved on the CPU while tessellating, so the GPU pass is one flat
// colored-triangle pipeline. This mirrors how `draw_backend_wgpu` works, but
// lives entirely in C++ and consumes the FFI command stream.

#pragma once

#include <cstdint>
#include <vector>

#include "quill.h"

namespace cppffi {

class GlBackend {
public:
    GlBackend() = default;
    ~GlBackend();
    GlBackend(const GlBackend&) = delete;
    GlBackend& operator=(const GlBackend&) = delete;

    // Compiles the program and creates the vertex array/buffer. Requires a
    // current GL context.
    bool init();

    // Starts a frame for a `fb_width x fb_height` device framebuffer whose
    // logical size is `fb_size / scale`.
    void begin_frame(int fb_width, int fb_height, float scale);

    // Consumes a whole draw list.
    void submit(const QuillDrawList* list);

    // Flushes anything left in the batch.
    void end_frame();

    std::uint64_t triangles() const { return triangles_; }
    std::uint64_t commands() const { return commands_; }

private:
    struct Vertex {
        float x, y;
        float r, g, b, a;
    };

    struct State {
        QuillTransform transform;
        float opacity;
        bool has_clip;
        QuillRect clip;
    };

    void reset_state();
    void apply(const QuillCommand& command);
    void flush();
    void set_scissor(const QuillRect& rect);
    void clear_scissor();

    // Tessellation, in the current local space; `apply` bakes the transform.
    QuillVec2 transform_point(QuillVec2 p) const;
    void triangle(QuillVec2 a, QuillVec2 b, QuillVec2 c, const QuillColor& color);
    void fill_convex(const std::vector<QuillVec2>& points, const QuillColor& color);
    void ring(const std::vector<QuillVec2>& outer, const std::vector<QuillVec2>& inner,
              const QuillColor& color);
    void stroke_segment(QuillVec2 from, QuillVec2 to, float width, const QuillColor& color);

    unsigned program_ = 0;
    unsigned vao_ = 0;
    unsigned vbo_ = 0;
    int viewport_location_ = -1;
    int fb_width_ = 0;
    int fb_height_ = 0;
    float scale_ = 1.0f;
    std::vector<Vertex> vertices_;
    std::vector<State> stack_;
    State state_{};
    bool scissor_on_ = false;
    QuillRect scissor_{};
    std::uint64_t triangles_ = 0;
    std::uint64_t commands_ = 0;
};

}  // namespace cppffi
