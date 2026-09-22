#include "ui.hpp"

#include <algorithm>
#include <cmath>
#include <memory>

namespace cppffi {

namespace {

constexpr float kPi = 3.14159265358979323846f;

// -- dashboard widgets -----------------------------------------------------

class Panel : public Widget {
public:
    Panel(WidgetPtr child, float padding) : child_(std::move(child)), padding_(padding) {}

    float height_for(float width) const override {
        return 2.0f * padding_ + child_->height_for(width - 2.0f * padding_);
    }

    void layout(Rect bounds) override {
        bounds_ = bounds;
        const float inner = bounds.w - 2.0f * padding_;
        child_->layout({bounds.x + padding_, bounds.y + padding_, inner,
                        child_->height_for(inner)});
    }

    void paint(Canvas& canvas) const override {
        canvas.fill_rounded(bounds_, radius::PANEL, palette().surface_raised);
        canvas.stroke_rounded(bounds_, radius::PANEL, border::HAIRLINE, palette().border);
        child_->paint(canvas);
    }

private:
    WidgetPtr child_;
    float padding_;
};

class ProgressBar : public Widget {
public:
    explicit ProgressBar(float value) : value_(std::clamp(value, 0.0f, 1.0f)) {}

    float height_for(float) const override { return 14.0f; }

    void paint(Canvas& canvas) const override {
        canvas.fill_rounded(bounds_, bounds_.h * 0.5f, palette().surface_hover);
        if (value_ <= 0.0f) {
            return;
        }
        // A pill, so a tiny value still reads as a rounded bar rather than a
        // sliver with square ends.
        Rect fill = bounds_;
        fill.w = std::min(std::max(bounds_.w * value_, bounds_.h), bounds_.w);
        canvas.save();
        canvas.clip(bounds_);
        canvas.fill_rounded(fill, bounds_.h * 0.5f, palette().accent);
        canvas.restore();
    }

private:
    float value_;
};

class StatRow : public Widget {
public:
    explicit StatRow(std::vector<float> weights) : weights_(std::move(weights)) {}

    float height_for(float) const override { return 10.0f; }

    void paint(Canvas& canvas) const override {
        float x = bounds_.x;
        for (std::size_t i = 0; i < weights_.size(); ++i) {
            const float w = bounds_.w * weights_[i];
            const Color color = i == 0 ? palette().accent : palette().muted;
            canvas.fill_rounded({x, bounds_.y, w, bounds_.h}, bounds_.h * 0.5f, color);
            x += w + space::XS;
        }
    }

private:
    std::vector<float> weights_;
};

class Chart : public Widget {
public:
    explicit Chart(const std::vector<float>& samples) : samples_(samples) {}

    float height_for(float) const override { return 90.0f; }

    void paint(Canvas& canvas) const override {
        for (int i = 0; i <= 3; ++i) {
            const float y = bounds_.y + bounds_.h * static_cast<float>(i) / 3.0f;
            canvas.line({bounds_.x, y}, {bounds_.x + bounds_.w, y}, palette().border_subtle,
                        1.0f);
        }
        if (samples_.size() < 2) {
            return;
        }
        // Clip the series to the panel: the first sample may sit on the top
        // edge, and a half-width stroke would otherwise bleed out.
        canvas.save();
        canvas.clip(bounds_);
        const float span = static_cast<float>(samples_.size() - 1);
        auto point = [&](std::size_t i) {
            const float t = static_cast<float>(i) / span;
            return Vec2{bounds_.x + bounds_.w * t,
                        bounds_.y + bounds_.h * (1.0f - samples_[i])};
        };
        for (std::size_t i = 1; i < samples_.size(); ++i) {
            canvas.line(point(i - 1), point(i), palette().accent, 2.0f);
        }
        for (std::size_t i = 0; i < samples_.size(); ++i) {
            canvas.fill_circle(point(i), 3.0f, palette().foreground);
        }
        canvas.restore();
    }

private:
    std::vector<float> samples_;
};

class IconButton : public Widget {
public:
    explicit IconButton(bool primary) : primary_(primary) {}

    float height_for(float) const override { return control::HEIGHT; }

    void paint(Canvas& canvas) const override {
        const Color background = primary_ ? palette().accent : palette().border;
        canvas.fill_rounded(bounds_, radius::MD, background);
        // A stand-in "icon": two stacked bars, since ABI v1 has no text.
        const Vec2 c = bounds_.center();
        canvas.fill_rounded({c.x - 8.0f, c.y - 5.0f, 16.0f, 3.0f}, 1.5f, palette().foreground);
        canvas.fill_rounded({c.x - 8.0f, c.y + 2.0f, 10.0f, 3.0f}, 1.5f, palette().foreground);
    }

private:
    bool primary_;
};

class Toggle : public Widget {
public:
    explicit Toggle(bool on) : on_(on) {}

    float height_for(float) const override { return 28.0f; }

    void paint(Canvas& canvas) const override {
        const float h = bounds_.h;
        canvas.fill_rounded(bounds_, h * 0.5f,
                            on_ ? palette().accent : palette().surface_hover);
        const float cx = on_ ? bounds_.x + bounds_.w - h * 0.5f : bounds_.x + h * 0.5f;
        canvas.fill_circle({cx, bounds_.y + h * 0.5f}, h * 0.5f - 3.0f, palette().foreground);
    }

private:
    bool on_;
};

class Spinner : public Widget {
public:
    explicit Spinner(float phase) : phase_(phase) {}

    float height_for(float) const override { return 36.0f; }

    void paint(Canvas& canvas) const override {
        const float radius = std::min(bounds_.w, bounds_.h) * 0.5f - 4.0f;
        const Vec2 center = bounds_.center();
        canvas.save();
        // Exercises opacity and a transform on the same group: the arc is
        // drawn in local space and rotated about the widget's centre.
        canvas.set_opacity(0.85f);
        canvas.set_transform(rotation(phase_, center));
        constexpr int kSegments = 12;
        for (int i = 0; i < kSegments; ++i) {
            const float a0 = static_cast<float>(i) / kSegments * 2.0f * kPi;
            const float a1 = static_cast<float>(i + 1) / kSegments * 2.0f * kPi;
            canvas.line({center.x + std::cos(a0) * radius, center.y + std::sin(a0) * radius},
                        {center.x + std::cos(a1) * radius, center.y + std::sin(a1) * radius},
                        palette().accent, 3.0f);
        }
        canvas.restore();
    }

private:
    float phase_;
};

}  // namespace

std::vector<float> default_samples() {
    return {0.35f, 0.52f, 0.44f, 0.68f, 0.61f, 0.82f, 0.74f, 0.91f, 0.86f, 0.97f};
}

void build_dashboard(Canvas& canvas, Rect bounds, const DashboardState& state) {
    auto balance_column = std::make_unique<Column>(0.0f, 10.0f);
    balance_column->add(std::make_unique<ProgressBar>(state.balance));
    balance_column->add(std::make_unique<StatRow>(std::vector<float>{0.42f, 0.24f, 0.16f}));
    auto balance_panel = std::make_unique<Panel>(std::move(balance_column), kPanelPadding);

    auto chart_panel =
        std::make_unique<Panel>(std::make_unique<Chart>(state.samples), kPanelPadding);

    auto row = std::make_unique<Row>(kRootGap);
    row->add(std::make_unique<IconButton>(true));
    row->add(std::make_unique<Toggle>(!state.paused));
    row->add(std::make_unique<Spinner>(state.phase));

    auto root = std::make_unique<Column>(kRootPadding, kRootGap);
    root->add(std::move(balance_panel));
    root->add(std::move(chart_panel));
    root->add(std::move(row));

    root->layout(bounds);
    root->paint(canvas);
}

}  // namespace cppffi
