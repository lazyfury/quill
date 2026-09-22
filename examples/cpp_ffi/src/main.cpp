// The C++ host for the quill FFI demo.
//
// It owns the window and the OpenGL context, builds the UI (see `ui.cpp`) into
// a quill `DrawList` through the FFI, and rasterizes that list with its own
// OpenGL backend (see `gl_backend.cpp`).
//
// Verification is screenshot-free: `--dump` prints the command stream the UI
// produced, and `--selfcheck` renders into an offscreen framebuffer and reads
// the pixels back with `glReadPixels`.

#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <functional>
#include <map>
#include <string>
#include <vector>

#define GLFW_INCLUDE_NONE
#include <GLFW/glfw3.h>

#if defined(__APPLE__)
#ifndef GL_SILENCE_DEPRECATION
#define GL_SILENCE_DEPRECATION
#endif
#include <OpenGL/gl3.h>
#else
#error "cpp_ffi's OpenGL host currently targets macOS (OpenGL/gl3.h)."
#endif

#include "canvas.hpp"
#include "demoapp.h"
#include "gallery.hpp"
#include "gl_backend.hpp"
#include "ui.hpp"
#include "wgpu_ffi.h"

using namespace cppffi;

// Provided by `native_window.mm`: the NSView of a GLFW window, for the wgpu
// surface. Returns null if the window has no view.
extern "C" void* cpp_ffi_ns_view(GLFWwindow* window);

namespace {

struct Options {
    bool dump = false;
    bool selfcheck = false;
    bool gallery = false;
    bool demoapp = false;
    bool wgpu = false;
    bool help = false;
    int width = 520;
    int height = 480;
    int frames = 0;
};

void print_help() {
    std::printf(
        "cpp_ffi — a C++ UI + OpenGL backend on top of the quill core FFI\n"
        "\n"
        "usage: cpp_ffi [--selfcheck | --dump] [--width N] [--height N] [--frames N]\n"
        "\n"
        "  --selfcheck   render into an offscreen framebuffer and assert the pixels\n"
        "  --dump        build the UI and print the DrawList command stream (no GL)\n"
        "  --gallery     show the demo_app-styled component gallery instead of the dashboard\n"
        "  --demoapp     load the real demo_app gallery through demoapp_ffi (chrome only;\n"
        "                DrawText has no ABI v1 record). Arrow keys switch group.\n"
        "  --wgpu        render with the Rust wgpu backend (wgpu_ffi) instead of the\n"
        "                C++ OpenGL backend; --selfcheck --wgpu reads it back offscreen.\n"
        "  --width N     window width in logical pixels (default 520)\n"
        "  --height N    window height in logical pixels (default 480)\n"
        "  --frames N    exit after N frames (smoke test for the real window)\n"
        "  --help        this text\n");
}

bool parse(int argc, char** argv, Options& options) {
    for (int i = 1; i < argc; ++i) {
        const std::string arg = argv[i];
        auto value = [&](int& out) {
            if (i + 1 >= argc) {
                return false;
            }
            out = std::atoi(argv[++i]);
            return out > 0;
        };
        if (arg == "--selfcheck") {
            options.selfcheck = true;
        } else if (arg == "--dump") {
            options.dump = true;
        } else if (arg == "--gallery") {
            options.gallery = true;
        } else if (arg == "--demoapp") {
            options.demoapp = true;
        } else if (arg == "--wgpu") {
            options.wgpu = true;
        } else if (arg == "--help" || arg == "-h") {
            options.help = true;
        } else if (arg == "--width") {
            if (!value(options.width)) {
                return false;
            }
        } else if (arg == "--height") {
            if (!value(options.height)) {
                return false;
            }
        } else if (arg == "--frames") {
            if (!value(options.frames)) {
                return false;
            }
        } else {
            std::fprintf(stderr, "unknown argument: %s\n", arg.c_str());
            return false;
        }
    }
    return true;
}

const char* tag_name(QuillCommandTag tag) {
    switch (tag) {
        case QUILL_CMD_SAVE: return "save";
        case QUILL_CMD_RESTORE: return "restore";
        case QUILL_CMD_SET_TRANSFORM: return "set_transform";
        case QUILL_CMD_SET_OPACITY: return "set_opacity";
        case QUILL_CMD_CLIP_RECT: return "clip_rect";
        case QUILL_CMD_FILL_RECT: return "fill_rect";
        case QUILL_CMD_STROKE_RECT: return "stroke_rect";
        case QUILL_CMD_LINE: return "line";
        case QUILL_CMD_FILL_CIRCLE: return "fill_circle";
        case QUILL_CMD_STROKE_CIRCLE: return "stroke_circle";
        case QUILL_CMD_FILL_ROUNDED_RECT: return "fill_rounded_rect";
        case QUILL_CMD_STROKE_ROUNDED_RECT: return "stroke_rounded_rect";
        case QUILL_CMD_UNSUPPORTED: return "unsupported";
    }
    return "?";
}

DashboardState demo_state(float time) {
    DashboardState state;
    // Gently animate the bar and the spinner so a live window is not static.
    state.balance = 0.55f + 0.35f * std::sin(time * 0.7f);
    state.paused = false;
    state.samples = default_samples();
    state.phase = time * 1.5f;
    return state;
}

// One entry point for the two scenes, so dump/window/self-check agree on what
// is on screen.
void build_scene(Canvas& canvas, Rect bounds, const Options& options, float time) {
    if (options.gallery) {
        build_gallery(canvas, bounds);
    } else {
        build_dashboard(canvas, bounds, demo_state(time));
    }
}

void glfw_error(int code, const char* description) {
    std::fprintf(stderr, "GLFW error %d: %s\n", code, description);
}

void print_histogram(QuillDrawList* list) {
    const std::size_t count = quill_draw_list_len(list);
    std::map<int, int> counts;
    for (std::size_t i = 0; i < count; ++i) {
        counts[static_cast<int>(quill_draw_list_command(list, i).tag)]++;
    }
    std::printf("DrawList: %zu commands\n", count);
    for (const auto& [tag, n] : counts) {
        std::printf("  %-22s %d\n", tag_name(static_cast<QuillCommandTag>(tag)), n);
    }
}

// --dump: no window, no GPU — just prove the FFI + UI path.
int run_dump(const Options& options) {
    if (options.demoapp) {
        DemoAppHandle* app = demoapp_new();
        demoapp_set_viewport(app, static_cast<float>(options.width),
                             static_cast<float>(options.height));
        demoapp_update(app, 0.0f);
        demoapp_layout(app);
        QuillDrawList* list = demoapp_paint(app);
        print_histogram(list);
        quill_draw_list_free(list);
        demoapp_free(app);
        return 0;
    }

    QuillDrawList* list = quill_draw_list_new();
    Canvas canvas(list);
    build_scene(canvas,
                {0.0f, 0.0f, static_cast<float>(options.width),
                 static_cast<float>(options.height)},
                options, 0.0f);
    print_histogram(list);
    quill_draw_list_free(list);
    return 0;
}

GLFWwindow* create_window(const Options& options, bool visible, bool no_api) {
    if (no_api) {
        // No OpenGL context: the wgpu backend creates a Metal layer on the view.
        glfwWindowHint(GLFW_CLIENT_API, GLFW_NO_API);
    } else {
        glfwWindowHint(GLFW_CONTEXT_VERSION_MAJOR, 3);
        glfwWindowHint(GLFW_CONTEXT_VERSION_MINOR, 3);
        glfwWindowHint(GLFW_OPENGL_PROFILE, GLFW_OPENGL_CORE_PROFILE);
        glfwWindowHint(GLFW_OPENGL_FORWARD_COMPAT, GLFW_TRUE);
    }
    glfwWindowHint(GLFW_VISIBLE, visible ? GLFW_TRUE : GLFW_FALSE);
    return glfwCreateWindow(options.width, options.height, "quill C++ FFI", nullptr, nullptr);
}

void clear_to(const Color& color) {
    glClearColor(color.r, color.g, color.b, color.a);
    glClear(GL_COLOR_BUFFER_BIT);
}

// Reads one logical pixel back from a `GL_RGBA` buffer.
struct Pixel {
    int r = 0, g = 0, b = 0, a = 0;
};

Pixel sample(const std::vector<unsigned char>& rgba, int fb_width, int fb_height, float scale,
             float logical_x, float logical_y) {
    const int x = static_cast<int>(logical_x * scale);
    const int y = static_cast<int>(logical_y * scale);
    const int gl_y = fb_height - 1 - y;
    if (x < 0 || x >= fb_width || gl_y < 0 || gl_y >= fb_height) {
        return {};
    }
    const std::size_t index = (static_cast<std::size_t>(gl_y) * fb_width + x) * 4;
    return Pixel{rgba[index], rgba[index + 1], rgba[index + 2], rgba[index + 3]};
}

// The wgpu readback is already top-down (`copy_texture_to_buffer` copies rows
// in order), unlike `glReadPixels`, so it needs no y flip.
Pixel sample_top_down(const std::vector<unsigned char>& rgba, int width, int height, float scale,
                      float logical_x, float logical_y) {
    const int x = static_cast<int>(logical_x * scale);
    const int y = static_cast<int>(logical_y * scale);
    if (x < 0 || x >= width || y < 0 || y >= height) {
        return {};
    }
    const std::size_t index = (static_cast<std::size_t>(y) * width + x) * 4;
    return Pixel{rgba[index], rgba[index + 1], rgba[index + 2], rgba[index + 3]};
}

int channel(float value) { return static_cast<int>(value * 255.0f + 0.5f); }

bool near(int actual, float expected) { return std::abs(actual - channel(expected)) <= 4; }

// The expected pixels for whichever scene is on screen, shared by the OpenGL
// and wgpu self-checks so both backends are held to the same result. `at(x, y)`
// samples the rendered frame at a logical coordinate.
int check_scene_pixels(const Options& options, const std::function<Pixel(float, float)>& at,
                       std::uint64_t commands) {
    int failures = 0;
    auto check = [&](const char* label, const Pixel& got, const Color& want) {
        const bool ok = near(got.r, want.r) && near(got.g, want.g) && near(got.b, want.b);
        if (!ok) {
            ++failures;
        }
        std::printf("  %-16s (%3d,%3d,%3d) vs (%3d,%3d,%3d)  %s\n", label, got.r, got.g, got.b,
                    channel(want.r), channel(want.g), channel(want.b), ok ? "ok" : "FAIL");
    };

    if (options.demoapp) {
        // The real app fills the viewport: (4,4) lands on the sidebar surface,
        // not the clear color. Text has no ABI v1 record, so this checks that
        // the frame arrived and that the backend tessellated it; the chrome is
        // what can be compared.
        check("sidebar", at(4.0f, 4.0f), palette().surface);
        if (commands < 100) {
            ++failures;
            std::printf("  expected the full gallery frame, got %llu commands\n",
                        static_cast<unsigned long long>(commands));
        }
    } else if (options.gallery) {
        // Coordinates follow `build_gallery`: cards at x=16, card 1 at y=16,
        // pad 16, so the first button row starts at (32, 32), 96px wide with an
        // 8px gap; the hover row is one control height + 12 below.
        check("background", at(4.0f, 4.0f), palette().background);
        check("card", at(20.0f, 20.0f), palette().surface_raised);
        check("primary", at(40.0f, 40.0f), palette().accent);
        check("destructive", at(352.0f, 40.0f), palette().error);
        check("secondary hover", at(40.0f, 88.0f), palette().surface_hover);
    } else {
        // Coordinates follow the layout in `ui.cpp`: root padding 16, panel
        // padding 12, progress bar at (28, 28) sized (W-56, 14).
        const float w = static_cast<float>(options.width);
        check("background", at(4.0f, 4.0f), palette().background);
        check("panel", at(20.0f, 20.0f), palette().surface_raised);
        check("bar fill", at(32.0f, 33.0f), palette().accent);
        check("bar track", at(w - 40.0f, 33.0f), palette().surface_hover);
    }
    return failures;
}

// --selfcheck: render offscreen and verify the pixels the UI promised.
int run_selfcheck(const Options& options) {
    GLFWwindow* window = create_window(options, false, false);
    if (window == nullptr) {
        std::fprintf(stderr, "failed to create the offscreen window\n");
        return 1;
    }
    glfwMakeContextCurrent(window);

    GlBackend backend;
    if (!backend.init()) {
        std::fprintf(stderr, "failed to init the OpenGL backend\n");
        glfwDestroyWindow(window);
        return 1;
    }

    int fb_width = 0;
    int fb_height = 0;
    glfwGetFramebufferSize(window, &fb_width, &fb_height);
    const float scale =
        static_cast<float>(fb_width) / static_cast<float>(options.width);

    // An offscreen color target, so the check does not depend on a window
    // drawable existing for a hidden window.
    GLuint framebuffer = 0;
    GLuint color = 0;
    glGenFramebuffers(1, &framebuffer);
    glBindFramebuffer(GL_FRAMEBUFFER, framebuffer);
    glGenRenderbuffers(1, &color);
    glBindRenderbuffer(GL_RENDERBUFFER, color);
    glRenderbufferStorage(GL_RENDERBUFFER, GL_RGBA8, fb_width, fb_height);
    glFramebufferRenderbuffer(GL_FRAMEBUFFER, GL_COLOR_ATTACHMENT0, GL_RENDERBUFFER, color);
    if (glCheckFramebufferStatus(GL_FRAMEBUFFER) != GL_FRAMEBUFFER_COMPLETE) {
        std::fprintf(stderr, "offscreen framebuffer is incomplete\n");
        glfwDestroyWindow(window);
        return 1;
    }

    backend.begin_frame(fb_width, fb_height, scale);
    clear_to(palette().background);
    QuillDrawList* list = nullptr;
    DemoAppHandle* demoapp = nullptr;
    if (options.demoapp) {
        demoapp = demoapp_new();
        demoapp_set_viewport(demoapp, static_cast<float>(options.width),
                             static_cast<float>(options.height));
        demoapp_update(demoapp, 0.0f);
        demoapp_layout(demoapp);
        list = demoapp_paint(demoapp);
    } else {
        list = quill_draw_list_new();
        Canvas canvas(list);
        if (options.gallery) {
            build_gallery(canvas, {0.0f, 0.0f, static_cast<float>(options.width),
                                   static_cast<float>(options.height)});
        } else {
            DashboardState state = demo_state(0.0f);
            state.balance = 0.6f;  // fixed, so the pixel expectations below hold
            build_dashboard(canvas,
                            {0.0f, 0.0f, static_cast<float>(options.width),
                             static_cast<float>(options.height)},
                            state);
        }
    }
    backend.submit(list);
    backend.end_frame();
    quill_draw_list_free(list);
    if (demoapp != nullptr) {
        demoapp_free(demoapp);
    }
    glFinish();

    std::vector<unsigned char> rgba(static_cast<std::size_t>(fb_width) * fb_height * 4);
    glReadPixels(0, 0, fb_width, fb_height, GL_RGBA, GL_UNSIGNED_BYTE, rgba.data());

    auto at = [&](float x, float y) { return sample(rgba, fb_width, fb_height, scale, x, y); };

    std::printf("self-check: %s, offscreen %dx%d device, scale %.2f\n",
                options.demoapp ? "demo_app" : (options.gallery ? "gallery" : "dashboard"),
                fb_width, fb_height, scale);
    std::printf("  drawlist         %llu commands, %llu triangles\n",
                static_cast<unsigned long long>(backend.commands()),
                static_cast<unsigned long long>(backend.triangles()));

    const int failures = check_scene_pixels(options, at, backend.commands());

    glBindFramebuffer(GL_FRAMEBUFFER, 0);
    glDeleteRenderbuffers(1, &color);
    glDeleteFramebuffers(1, &framebuffer);
    glfwDestroyWindow(window);
    glfwTerminate();

    if (failures == 0) {
        std::printf("self-check: ok\n");
        return 0;
    }
    std::printf("self-check: %d failure(s)\n", failures);
    return 1;
}

int run_window(const Options& options) {
    const bool use_wgpu = options.wgpu;
    GLFWwindow* window = create_window(options, true, use_wgpu);
    if (window == nullptr) {
        std::fprintf(stderr, "failed to create the window\n");
        glfwTerminate();
        return 1;
    }

    GlBackend backend;
    WgpuFfi* wgpu = nullptr;
    if (use_wgpu) {
        int fb_width = 0;
        int fb_height = 0;
        int window_width = 0;
        int window_height = 0;
        glfwGetFramebufferSize(window, &fb_width, &fb_height);
        glfwGetWindowSize(window, &window_width, &window_height);
        const float scale = window_width > 0
                                ? static_cast<float>(fb_width) / static_cast<float>(window_width)
                                : 1.0f;
        wgpu = wgpu_ffi_new(cpp_ffi_ns_view(window), fb_width, fb_height, scale);
        if (wgpu == nullptr) {
            std::fprintf(stderr, "failed to init the wgpu backend\n");
            glfwDestroyWindow(window);
            glfwTerminate();
            return 1;
        }
    } else {
        glfwMakeContextCurrent(window);
        glfwSwapInterval(1);
        if (!backend.init()) {
            std::fprintf(stderr, "failed to init the OpenGL backend\n");
            glfwDestroyWindow(window);
            glfwTerminate();
            return 1;
        }
    }

    // The real demo_app gallery, when asked for. Its frame comes from Rust
    // (`demoapp_*`); the OpenGL backend draws the resulting DrawList. Arrow
    // keys switch the catalog group so the comparison can move around.
    DemoAppHandle* demoapp = options.demoapp ? demoapp_new() : nullptr;
    if (demoapp != nullptr) {
        glfwSetWindowUserPointer(window, demoapp);
        glfwSetKeyCallback(window, [](GLFWwindow* handle, int key, int, int action, int) {
            if (action != GLFW_PRESS) {
                return;
            }
            auto* app = static_cast<DemoAppHandle*>(glfwGetWindowUserPointer(handle));
            const uint32_t count = demoapp_group_count();
            if (app == nullptr || count == 0) {
                return;
            }
            static uint32_t group = 0;
            if (key == GLFW_KEY_RIGHT || key == GLFW_KEY_DOWN) {
                group = (group + 1) % count;
            } else if (key == GLFW_KEY_LEFT || key == GLFW_KEY_UP) {
                group = (group + count - 1) % count;
            } else {
                return;
            }
            demoapp_show_group(app, group);
        });
    }

    const double start = glfwGetTime();
    double last = start;
    int last_fb_width = 0;
    int last_fb_height = 0;
    int frames = 0;
    while (!glfwWindowShouldClose(window)) {
        glfwPollEvents();
        int fb_width = 0;
        int fb_height = 0;
        int window_width = 0;
        int window_height = 0;
        glfwGetFramebufferSize(window, &fb_width, &fb_height);
        glfwGetWindowSize(window, &window_width, &window_height);
        if (fb_width == 0 || fb_height == 0) {
            glfwWaitEvents();
            continue;
        }
        const float scale = window_width > 0
                                ? static_cast<float>(fb_width) / static_cast<float>(window_width)
                                : 1.0f;

        const double now = glfwGetTime();
        QuillDrawList* list = nullptr;
        if (demoapp != nullptr) {
            demoapp_set_viewport(demoapp, static_cast<float>(window_width),
                                 static_cast<float>(window_height));
            demoapp_update(demoapp, static_cast<float>(now - last));
            demoapp_layout(demoapp);
            list = demoapp_paint(demoapp);
        } else {
            list = quill_draw_list_new();
            Canvas canvas(list);
            build_scene(canvas,
                        {0.0f, 0.0f, static_cast<float>(window_width),
                         static_cast<float>(window_height)},
                        options, static_cast<float>(now - start));
        }

        if (use_wgpu) {
            if (fb_width != last_fb_width || fb_height != last_fb_height) {
                wgpu_ffi_resize(wgpu, fb_width, fb_height, scale);
                last_fb_width = fb_width;
                last_fb_height = fb_height;
            }
            const Color clear = palette().background;
            wgpu_ffi_render(wgpu, list, clear.r, clear.g, clear.b, clear.a);
        } else {
            backend.begin_frame(fb_width, fb_height, scale);
            clear_to(palette().background);
            backend.submit(list);
            backend.end_frame();
        }
        quill_draw_list_free(list);
        last = now;

        if (!use_wgpu) {
            glfwSwapBuffers(window);
        }
        ++frames;
        if (options.frames > 0 && frames >= options.frames) {
            break;
        }
    }

    if (demoapp != nullptr) {
        demoapp_free(demoapp);
    }
    if (wgpu != nullptr) {
        wgpu_ffi_free(wgpu);
    }
    glfwDestroyWindow(window);
    glfwTerminate();
    return 0;
}

// --selfcheck --wgpu: render offscreen with the Rust wgpu backend and assert
// the same pixels the OpenGL self-check does. No window needed.
int run_wgpu_selfcheck(const Options& options) {
    WgpuFfi* wgpu = wgpu_ffi_new(nullptr, 0, 0, 1.0f);
    if (wgpu == nullptr) {
        std::fprintf(stderr, "failed to init the wgpu backend\n");
        return 1;
    }

    QuillDrawList* list = nullptr;
    DemoAppHandle* demoapp = nullptr;
    if (options.demoapp) {
        demoapp = demoapp_new();
        demoapp_set_viewport(demoapp, static_cast<float>(options.width),
                             static_cast<float>(options.height));
        demoapp_update(demoapp, 0.0f);
        demoapp_layout(demoapp);
        list = demoapp_paint(demoapp);
    } else {
        list = quill_draw_list_new();
        Canvas canvas(list);
        if (options.gallery) {
            build_gallery(canvas, {0.0f, 0.0f, static_cast<float>(options.width),
                                   static_cast<float>(options.height)});
        } else {
            DashboardState state = demo_state(0.0f);
            state.balance = 0.6f;
            build_dashboard(canvas,
                            {0.0f, 0.0f, static_cast<float>(options.width),
                             static_cast<float>(options.height)},
                            state);
        }
    }

    const Color clear = palette().background;
    const int rc = wgpu_ffi_render_offscreen(wgpu, list, static_cast<uint32_t>(options.width),
                                             static_cast<uint32_t>(options.height), 1.0f,
                                             clear.r, clear.g, clear.b, clear.a);
    const std::size_t count = quill_draw_list_len(list);
    quill_draw_list_free(list);
    if (demoapp != nullptr) {
        demoapp_free(demoapp);
    }
    if (rc != 0) {
        std::fprintf(stderr, "wgpu offscreen render failed: %d\n", rc);
        wgpu_ffi_free(wgpu);
        return 1;
    }

    const int width = options.width;
    const int height = options.height;
    std::vector<unsigned char> rgba(static_cast<std::size_t>(width) * height * 4);
    const std::size_t written = wgpu_ffi_read_pixels(wgpu, rgba.data(), rgba.size());
    if (written != rgba.size()) {
        std::fprintf(stderr, "wgpu readback returned %zu of %zu bytes\n", written, rgba.size());
        wgpu_ffi_free(wgpu);
        return 1;
    }

    auto at = [&](float x, float y) { return sample_top_down(rgba, width, height, 1.0f, x, y); };
    std::printf("self-check: %s, wgpu offscreen %dx%d, scale 1.00\n",
                options.demoapp ? "demo_app" : (options.gallery ? "gallery" : "dashboard"),
                width, height);
    std::printf("  drawlist         %zu commands\n", count);
    const int failures = check_scene_pixels(options, at, count);

    wgpu_ffi_free(wgpu);
    if (failures == 0) {
        std::printf("self-check: ok\n");
        return 0;
    }
    std::printf("self-check: %d failure(s)\n", failures);
    return 1;
}

}  // namespace

int main(int argc, char** argv) {
    Options options;
    if (!parse(argc, argv, options)) {
        return 2;
    }
    if (options.help) {
        print_help();
        return 0;
    }
    // The header and the library are a contract; refuse a mismatch instead of
    // misreading the command records.
    if (quill_abi_version() != QUILL_ABI_VERSION) {
        std::fprintf(stderr, "ABI mismatch: header %u, library %u\n", QUILL_ABI_VERSION,
                     quill_abi_version());
        return 3;
    }
    if (options.dump) {
        return run_dump(options);
    }
    if (options.selfcheck && options.wgpu) {
        // Headless: no GLFW window or context needed.
        return run_wgpu_selfcheck(options);
    }
    if (!glfwInit()) {
        std::fprintf(stderr, "failed to init GLFW\n");
        return 1;
    }
    glfwSetErrorCallback(glfw_error);
    if (options.selfcheck) {
        return run_selfcheck(options);
    }
    return run_window(options);
}
