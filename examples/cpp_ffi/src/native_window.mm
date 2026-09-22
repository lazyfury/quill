// The one Objective-C++ file: it hands the wgpu backend the NSView of a GLFW
// window, which `glfw3.h` alone does not expose.
//
// wgpu's Metal backend attaches a `CAMetalLayer` to this view, so the window
// must be created with `GLFW_CLIENT_API = GLFW_NO_API` (no OpenGL context).

#import <Cocoa/Cocoa.h>

#define GLFW_INCLUDE_NONE
#include <GLFW/glfw3.h>
#define GLFW_EXPOSE_NATIVE_COCOA
#include <GLFW/glfw3native.h>

extern "C" void* cpp_ffi_ns_view(GLFWwindow* window) {
    NSWindow* ns_window = (NSWindow*)glfwGetCocoaWindow(window);
    if (ns_window == nil) {
        return nullptr;
    }
    return (void*)[ns_window contentView];
}
