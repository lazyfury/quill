cmake_minimum_required(VERSION 3.20)
project(cpp_ffi CXX)
enable_language(OBJCXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

if(NOT APPLE)
  message(FATAL_ERROR
    "cpp_ffi targets macOS for now: the OpenGL backend uses <OpenGL/gl3.h>. "
    "Porting it means adding a GL loader (GLAD) and the matching GLFW hints.")
endif()

# GLFW: prefer an installed one, fall back to building the pinned release.
find_package(glfw3 QUIET)
if(NOT glfw3_FOUND)
  include(FetchContent)
  set(GLFW_BUILD_DOCS OFF CACHE BOOL "" FORCE)
  set(GLFW_BUILD_TESTS OFF CACHE BOOL "" FORCE)
  set(GLFW_BUILD_EXAMPLES OFF CACHE BOOL "" FORCE)
  set(GLFW_INSTALL OFF CACHE BOOL "" FORCE)
  FetchContent_Declare(glfw
    GIT_REPOSITORY https://github.com/glfw/glfw.git
    GIT_TAG 3.4
    GIT_SHALLOW TRUE)
  FetchContent_MakeAvailable(glfw)
endif()

# The Rust core + the demo_app/wgpu FFI, built as static libraries by cargo.
set(QUILL_ROOT "${CMAKE_CURRENT_SOURCE_DIR}/../..")
set(QUILL_FFI_LIB "${QUILL_ROOT}/target/release/libdraw_ffi.a")
set(DEMOAPP_FFI_LIB "${QUILL_ROOT}/target/release/libdemoapp_ffi.a")
set(WGPU_FFI_LIB "${QUILL_ROOT}/target/release/libwgpu_ffi.a")

add_executable(cpp_ffi
  src/main.cpp
  src/canvas.cpp
  src/theme.cpp
  src/widget.cpp
  src/ui.cpp
  src/gallery.cpp
  src/gl_backend.cpp
  src/native_window.mm)

target_include_directories(cpp_ffi PRIVATE
  "${CMAKE_CURRENT_SOURCE_DIR}/src"
  "${QUILL_ROOT}/crates/draw_ffi/include"
  "${QUILL_ROOT}/examples/demoapp_ffi/include"
  "${QUILL_ROOT}/examples/wgpu_ffi/include")

add_custom_target(draw_ffi_build
  COMMAND cargo build --release -p draw_ffi
  WORKING_DIRECTORY "${QUILL_ROOT}"
  COMMENT "cargo build -p draw_ffi --release")
add_custom_target(demoapp_ffi_build
  COMMAND cargo build --release -p demoapp_ffi
  WORKING_DIRECTORY "${QUILL_ROOT}"
  COMMENT "cargo build -p demoapp_ffi --release")
add_custom_target(wgpu_ffi_build
  COMMAND cargo build --release -p wgpu_ffi
  WORKING_DIRECTORY "${QUILL_ROOT}"
  COMMENT "cargo build -p wgpu_ffi --release")
add_dependencies(cpp_ffi draw_ffi_build demoapp_ffi_build wgpu_ffi_build)

# Each staticlib bundles its Rust deps, so `quill_*` resolves from any of them;
# they are all listed for clarity.
target_link_libraries(cpp_ffi PRIVATE glfw "${QUILL_FFI_LIB}" "${DEMOAPP_FFI_LIB}" "${WGPU_FFI_LIB}")
target_link_libraries(cpp_ffi PRIVATE
  "-framework OpenGL"
  "-framework Cocoa"
  "-framework IOKit"
  "-framework CoreVideo"
  "-framework CoreFoundation"
  "-framework Metal"
  "-framework QuartzCore"
  "-framework CoreGraphics"
  "-framework IOSurface")

target_compile_definitions(cpp_ffi PRIVATE GL_SILENCE_DEPRECATION)
