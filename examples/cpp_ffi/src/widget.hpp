// The widget primitives shared by the dashboard (`ui.cpp`) and the component
// gallery (`gallery.cpp`): a `Widget` base plus the `Column` / `Row` layout
// containers. Kept apart from both so neither has to own the other's widgets.

#pragma once

#include <memory>
#include <vector>

#include "canvas.hpp"

namespace cppffi {

class Widget {
public:
    virtual ~Widget() = default;

    // Height at the given available width. Containers ask their children.
    virtual float height_for(float width) const = 0;

    // Assigns the final rectangle; containers recurse into their children.
    virtual void layout(Rect bounds) { bounds_ = bounds; }

    virtual void paint(Canvas& canvas) const = 0;

    Rect bounds() const { return bounds_; }

protected:
    Rect bounds_{};
};

using WidgetPtr = std::unique_ptr<Widget>;

/// Stacks children vertically with `padding` around them and `gap` between.
class Column : public Widget {
public:
    Column(float padding, float gap) : padding_(padding), gap_(gap) {}

    Column& add(WidgetPtr child);

    float height_for(float width) const override;
    void layout(Rect bounds) override;
    void paint(Canvas& canvas) const override;

private:
    float padding_;
    float gap_;
    std::vector<WidgetPtr> children_;
};

/// Places children in equal-width columns with `gap` between them.
class Row : public Widget {
public:
    explicit Row(float gap) : gap_(gap) {}

    Row& add(WidgetPtr child);

    float height_for(float width) const override;
    void layout(Rect bounds) override;
    void paint(Canvas& canvas) const override;

private:
    float gap_;
    std::vector<WidgetPtr> children_;
};

}  // namespace cppffi
