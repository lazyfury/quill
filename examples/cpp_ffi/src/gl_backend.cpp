#include "gl_backend.hpp"

#include <algorithm>
#include <cmath>
#include <cstdio>

#if defined(__APPLE__)
#ifndef GL_SILENCE_DEPRECATION
#define GL_SILENCE_DEPRECATION
#endif
#include <OpenGL/gl3.h>
#else
#error "cpp_ffi's OpenGL backend currently targets macOS (OpenGL/gl3.h)."
#endif

namespace cppffi {

namespace {

constexpr int kCircleSegments = 64;
constexpr int kCornerSegments = 8;
constexpr float kPi = 3.14159265358979323846f;

const char* kVertexShader = R"(#version 330 core
layout(location = 0) in vec2 aPos;
layout(location = 1) in vec4 aColor;
uniform vec2 uViewport;
out vec4 vColor;
void main() {
    vec2 ndc = vec2(aPos.x / uViewport.x * 2.0 - 1.0,
                    1.0 - aPos.y / uViewport.y * 2.0);
    gl_Position = vec4(ndc, 0.0, 1.0);
    vColor = aColor;
}
)";

const char* kFragmentShader = R"(#version 330 core
in vec4 vColor;
out vec4 fragColor;
void main() {
    fragColor = vColor;
}
)";

unsigned compile_shader(GLenum type, const char* source) {
    const unsigned shader = glCreateShader(type);
    glShaderSource(shader, 1, &source, nullptr);
    glCompileShader(shader);
    int ok = 0;
    glGetShaderiv(shader, GL_COMPILE_STATUS, &ok);
    if (!ok) {
        char log[2048];
        glGetShaderInfoLog(shader, sizeof(log), nullptr, log);
        std::fprintf(stderr, "GLSL compile failed: %s\n", log);
        glDeleteShader(shader);
        return 0;
    }
    return shader;
}

std::vector<QuillVec2> circle_points(QuillVec2 center, float radius, int segments) {
    const int n = std::max(3, segments);
    std::vector<QuillVec2> points;
    points.reserve(static_cast<std::size_t>(n));
    for (int i = 0; i < n; ++i) {
        const float t = static_cast<float>(i) / static_cast<float>(n) * 2.0f * kPi;
        points.push_back({center.x + std::cos(t) * radius, center.y + std::sin(t) * radius});
    }
    return points;
}

// A rounded rectangle as a convex polygon, clockwise from the top-right corner.
//
// Every corner emits the same number of points whether or not it has a radius,
// so an outer polygon and its inset (used for strokes) always line up index for
// index in `ring`.
std::vector<QuillVec2> rounded_points(QuillRect rect, QuillCornerRadii corners, int segments,
                                      float inset) {
    const float x0 = rect.x + inset;
    const float y0 = rect.y + inset;
    const float x1 = rect.x + rect.width - inset;
    const float y1 = rect.y + rect.height - inset;
    const float limit = std::min(x1 - x0, y1 - y0) * 0.5f;
    const float tl = std::min(std::max(0.0f, corners.top_left - inset), limit);
    const float tr = std::min(std::max(0.0f, corners.top_right - inset), limit);
    const float br = std::min(std::max(0.0f, corners.bottom_right - inset), limit);
    const float bl = std::min(std::max(0.0f, corners.bottom_left - inset), limit);

    std::vector<QuillVec2> points;
    const int n = std::max(1, segments);
    auto arc = [&](float cx, float cy, float radius, float a0, float a1) {
        for (int i = 0; i <= n; ++i) {
            if (radius <= 0.0f) {
                points.push_back({cx, cy});
                continue;
            }
            const float t = a0 + (a1 - a0) * static_cast<float>(i) / static_cast<float>(n);
            points.push_back({cx + std::cos(t) * radius, cy + std::sin(t) * radius});
        }
    };
    arc(x1 - tr, y0 + tr, tr, -kPi * 0.5f, 0.0f);
    arc(x1 - br, y1 - br, br, 0.0f, kPi * 0.5f);
    arc(x0 + bl, y1 - bl, bl, kPi * 0.5f, kPi);
    arc(x0 + tl, y0 + tl, tl, kPi, kPi * 1.5f);
    return points;
}

QuillColor scaled(QuillColor color, float opacity) {
    color.a *= opacity;
    return color;
}

}  // namespace

GlBackend::~GlBackend() {
    if (vbo_ != 0) {
        glDeleteBuffers(1, &vbo_);
    }
    if (vao_ != 0) {
        glDeleteVertexArrays(1, &vao_);
    }
    if (program_ != 0) {
        glDeleteProgram(program_);
    }
}

bool GlBackend::init() {
    const unsigned vertex = compile_shader(GL_VERTEX_SHADER, kVertexShader);
    const unsigned fragment = compile_shader(GL_FRAGMENT_SHADER, kFragmentShader);
    if (vertex == 0 || fragment == 0) {
        return false;
    }
    program_ = glCreateProgram();
    glAttachShader(program_, vertex);
    glAttachShader(program_, fragment);
    glLinkProgram(program_);
    int ok = 0;
    glGetProgramiv(program_, GL_LINK_STATUS, &ok);
    glDeleteShader(vertex);
    glDeleteShader(fragment);
    if (!ok) {
        char log[2048];
        glGetProgramInfoLog(program_, sizeof(log), nullptr, log);
        std::fprintf(stderr, "GLSL link failed: %s\n", log);
        return false;
    }
    viewport_location_ = glGetUniformLocation(program_, "uViewport");

    glGenVertexArrays(1, &vao_);
    glGenBuffers(1, &vbo_);
    glBindVertexArray(vao_);
    glBindBuffer(GL_ARRAY_BUFFER, vbo_);
    glEnableVertexAttribArray(0);
    glVertexAttribPointer(0, 2, GL_FLOAT, GL_FALSE, sizeof(Vertex), reinterpret_cast<void*>(0));
    glEnableVertexAttribArray(1);
    glVertexAttribPointer(1, 4, GL_FLOAT, GL_FALSE, sizeof(Vertex),
                          reinterpret_cast<void*>(2 * sizeof(float)));
    glBindVertexArray(0);
    return true;
}

void GlBackend::reset_state() {
    state_ = State{};
    state_.transform = QuillTransform{QuillVec2{1.0f, 0.0f}, QuillVec2{0.0f, 1.0f},
                                      QuillVec2{0.0f, 0.0f}};
    state_.opacity = 1.0f;
    state_.has_clip = false;
    stack_.clear();
}

void GlBackend::begin_frame(int fb_width, int fb_height, float scale) {
    fb_width_ = fb_width;
    fb_height_ = fb_height;
    scale_ = scale > 0.0f ? scale : 1.0f;
    glViewport(0, 0, fb_width, fb_height);
    glDisable(GL_SCISSOR_TEST);
    scissor_on_ = false;
    glEnable(GL_BLEND);
    glBlendFunc(GL_SRC_ALPHA, GL_ONE_MINUS_SRC_ALPHA);
    reset_state();
    vertices_.clear();
    triangles_ = 0;
    commands_ = 0;
}

void GlBackend::submit(const QuillDrawList* list) {
    const std::size_t count = quill_draw_list_len(list);
    for (std::size_t i = 0; i < count; ++i) {
        apply(quill_draw_list_command(list, i));
    }
}

void GlBackend::end_frame() { flush(); }

void GlBackend::apply(const QuillCommand& command) {
    ++commands_;
    switch (command.tag) {
        case QUILL_CMD_SAVE:
            stack_.push_back(state_);
            break;
        case QUILL_CMD_RESTORE:
            if (!stack_.empty()) {
                // The scissor is GL state, so the batch drawn under the old
                // clip has to go out before the pop changes it.
                flush();
                state_ = stack_.back();
                stack_.pop_back();
                if (state_.has_clip) {
                    set_scissor(state_.clip);
                } else {
                    clear_scissor();
                }
            }
            break;
        case QUILL_CMD_SET_TRANSFORM:
            state_.transform = command.transform;
            break;
        case QUILL_CMD_SET_OPACITY:
            state_.opacity = command.opacity;
            break;
        case QUILL_CMD_CLIP_RECT:
            flush();
            state_.has_clip = true;
            state_.clip = command.rect;
            set_scissor(command.rect);
            break;
        case QUILL_CMD_FILL_RECT:
            fill_convex(rounded_points(command.rect, {}, 0, 0.0f), command.paint.color);
            break;
        case QUILL_CMD_STROKE_RECT:
            ring(rounded_points(command.rect, {}, 0, 0.0f),
                 rounded_points(command.rect, {}, 0, command.width), command.paint.color);
            break;
        case QUILL_CMD_LINE:
            stroke_segment(command.from, command.to, command.width, command.paint.color);
            break;
        case QUILL_CMD_FILL_CIRCLE:
            fill_convex(circle_points(command.center, command.radius, kCircleSegments),
                        command.paint.color);
            break;
        case QUILL_CMD_STROKE_CIRCLE:
            ring(circle_points(command.center, command.radius + command.width * 0.5f,
                               kCircleSegments),
                 circle_points(command.center, command.radius - command.width * 0.5f,
                               kCircleSegments),
                 command.paint.color);
            break;
        case QUILL_CMD_FILL_ROUNDED_RECT:
            fill_convex(rounded_points(command.rect, command.corners, kCornerSegments, 0.0f),
                        command.paint.color);
            break;
        case QUILL_CMD_STROKE_ROUNDED_RECT:
            ring(rounded_points(command.rect, command.corners, kCornerSegments, 0.0f),
                 rounded_points(command.rect, command.corners, kCornerSegments, command.width),
                 command.paint.color);
            break;
        case QUILL_CMD_UNSUPPORTED:
            break;
    }
}

void GlBackend::flush() {
    if (vertices_.empty()) {
        return;
    }
    glUseProgram(program_);
    glUniform2f(viewport_location_, static_cast<float>(fb_width_) / scale_,
                static_cast<float>(fb_height_) / scale_);
    glBindVertexArray(vao_);
    glBindBuffer(GL_ARRAY_BUFFER, vbo_);
    glBufferData(GL_ARRAY_BUFFER,
                 static_cast<GLsizeiptr>(vertices_.size() * sizeof(Vertex)), vertices_.data(),
                 GL_DYNAMIC_DRAW);
    glDrawArrays(GL_TRIANGLES, 0, static_cast<GLsizei>(vertices_.size()));
    vertices_.clear();
}

void GlBackend::set_scissor(const QuillRect& rect) {
    const int x0 = static_cast<int>(std::floor(rect.x * scale_));
    const int y0 = static_cast<int>(std::floor(rect.y * scale_));
    const int x1 = static_cast<int>(std::ceil((rect.x + rect.width) * scale_));
    const int y1 = static_cast<int>(std::ceil((rect.y + rect.height) * scale_));
    const int cx0 = std::clamp(x0, 0, fb_width_);
    const int cx1 = std::clamp(x1, 0, fb_width_);
    const int cy0 = std::clamp(y0, 0, fb_height_);
    const int cy1 = std::clamp(y1, 0, fb_height_);
    // GL's scissor origin is bottom-left; the clip is top-left.
    const int gl_y = fb_height_ - cy1;
    glEnable(GL_SCISSOR_TEST);
    glScissor(cx0, gl_y, std::max(0, cx1 - cx0), std::max(0, cy1 - cy0));
    scissor_on_ = true;
}

void GlBackend::clear_scissor() {
    glDisable(GL_SCISSOR_TEST);
    scissor_on_ = false;
}

QuillVec2 GlBackend::transform_point(QuillVec2 p) const {
    const QuillTransform& t = state_.transform;
    return QuillVec2{
        t.x_axis.x * p.x + t.y_axis.x * p.y + t.origin.x,
        t.x_axis.y * p.x + t.y_axis.y * p.y + t.origin.y,
    };
}

void GlBackend::triangle(QuillVec2 a, QuillVec2 b, QuillVec2 c, const QuillColor& color) {
    const QuillVec2 pa = transform_point(a);
    const QuillVec2 pb = transform_point(b);
    const QuillVec2 pc = transform_point(c);
    const QuillColor col = scaled(color, state_.opacity);
    vertices_.push_back({pa.x, pa.y, col.r, col.g, col.b, col.a});
    vertices_.push_back({pb.x, pb.y, col.r, col.g, col.b, col.a});
    vertices_.push_back({pc.x, pc.y, col.r, col.g, col.b, col.a});
    ++triangles_;
}

void GlBackend::fill_convex(const std::vector<QuillVec2>& points, const QuillColor& color) {
    if (points.size() < 3) {
        return;
    }
    for (std::size_t i = 1; i + 1 < points.size(); ++i) {
        triangle(points[0], points[i], points[i + 1], color);
    }
}

void GlBackend::ring(const std::vector<QuillVec2>& outer, const std::vector<QuillVec2>& inner,
                     const QuillColor& color) {
    const std::size_t count = std::min(outer.size(), inner.size());
    if (count < 2) {
        return;
    }
    for (std::size_t i = 0; i < count; ++i) {
        const std::size_t j = (i + 1) % count;
        triangle(outer[i], outer[j], inner[j], color);
        triangle(outer[i], inner[j], inner[i], color);
    }
}

void GlBackend::stroke_segment(QuillVec2 from, QuillVec2 to, float width, const QuillColor& color) {
    const float dx = to.x - from.x;
    const float dy = to.y - from.y;
    const float length = std::sqrt(dx * dx + dy * dy);
    if (length < 1e-6f) {
        return;
    }
    const float nx = -dy / length * width * 0.5f;
    const float ny = dx / length * width * 0.5f;
    fill_convex(
        {
            {from.x + nx, from.y + ny},
            {to.x + nx, to.y + ny},
            {to.x - nx, to.y - ny},
            {from.x - nx, from.y - ny},
        },
        color);
}

}  // namespace cppffi
