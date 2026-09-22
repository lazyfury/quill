#include "widget.hpp"

#include <algorithm>

namespace cppffi {

Column& Column::add(WidgetPtr child) {
    children_.push_back(std::move(child));
    return *this;
}

float Column::height_for(float width) const {
    const float inner = width - 2.0f * padding_;
    float height = 2.0f * padding_;
    for (std::size_t i = 0; i < children_.size(); ++i) {
        height += children_[i]->height_for(inner);
        if (i + 1 < children_.size()) {
            height += gap_;
        }
    }
    return height;
}

void Column::layout(Rect bounds) {
    bounds_ = bounds;
    const float inner = bounds.w - 2.0f * padding_;
    float y = bounds.y + padding_;
    for (auto& child : children_) {
        const float height = child->height_for(inner);
        child->layout({bounds.x + padding_, y, inner, height});
        y += height + gap_;
    }
}

void Column::paint(Canvas& canvas) const {
    for (const auto& child : children_) {
        child->paint(canvas);
    }
}

Row& Row::add(WidgetPtr child) {
    children_.push_back(std::move(child));
    return *this;
}

float Row::height_for(float) const {
    float tallest = 0.0f;
    for (const auto& child : children_) {
        tallest = std::max(tallest, child->height_for(0.0f));
    }
    return tallest;
}

void Row::layout(Rect bounds) {
    bounds_ = bounds;
    const auto count = static_cast<float>(children_.size());
    if (count == 0.0f) {
        return;
    }
    const float each = (bounds.w - gap_ * (count - 1.0f)) / count;
    float x = bounds.x;
    for (auto& child : children_) {
        child->layout({x, bounds.y, each, child->height_for(each)});
        x += each + gap_;
    }
}

void Row::paint(Canvas& canvas) const {
    for (const auto& child : children_) {
        child->paint(canvas);
    }
}

}  // namespace cppffi
