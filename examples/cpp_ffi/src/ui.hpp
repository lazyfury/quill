// The C++ UI: a tiny retained widget tree that lays itself out and emits a
// quill `DrawList` through `Canvas`.
//
// This is the point of the demo — the UI is organized here, in C++, and quill
// only supplies the core types and the command list. There is no Rust widget,
// layout or theme in the picture.

#pragma once

#include <vector>

#include "theme.hpp"
#include "widget.hpp"

namespace cppffi {

// Root padding and gap, shared by the layout and the self-check.
constexpr float kRootPadding = space::LG;
constexpr float kRootGap = space::MD;
constexpr float kPanelPadding = space::MD;

// The values the dashboard renders. A frame supplies a fresh state; the widget
// tree itself is rebuilt from it, so there is nothing to mutate.
struct DashboardState {
    // 0.0..=1.0 balance bar.
    float balance = 0.6f;
    // Whether the automatic refresh is paused (drives the toggle).
    bool paused = false;
    // Normalized 0..1 samples for the chart.
    std::vector<float> samples;
    // Spinner rotation in radians.
    float phase = 0.0f;
};

// A default sample series, so the demo has a chart with no input.
std::vector<float> default_samples();

// Lays the dashboard out in `bounds` and paints it into `canvas`.
void build_dashboard(Canvas& canvas, Rect bounds, const DashboardState& state);

}  // namespace cppffi
