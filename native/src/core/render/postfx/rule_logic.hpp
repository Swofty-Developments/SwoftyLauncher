#pragma once
#include "core/render/postfx/active_logic.hpp"
#include "core/render/postfx/plan_logic.hpp"
#include "core/render/postfx/signal_logic.hpp"

#include <algorithm>
#include <array>
#include <cmath>
#include <cstddef>
#include <cstdint>
#include <vector>

namespace render::postfx {

// The rules of an active config, stepped once per presented frame (docs: shader events contract, section
// 4). reset() puts every rule off with weight 0 and takes the instances' base options; step() moves the
// rules by dt and leaves the blended options and the instances a rule turned off in options() and off().
class RuleEngine {
public:
    struct State {
        bool on = false;
        bool changed = false;
        double w = 0, a = 0, left = 0, true_for = 0, false_for = 0, target = 0;
    };

    void reset(const std::vector<RuleSpec> &rules, const Plan &plan)
    {
        rules_ = &rules;
        state_.assign(rules.size(), State{});
        base_.resize(plan.instances.size());
        for (size_t i = 0; i < plan.instances.size(); i++)
            std::copy(plan.instances[i].options, plan.instances[i].options + SLOTS, base_[i].begin());
        opts_ = base_;
        off_.assign(plan.instances.size(), 0);
        plan_of_.assign(rules.size(), {});
        order_.clear();
        for (size_t r = 0; r < rules.size(); r++) {
            for (auto &set : rules[r].sets) {
                int at = -1;
                for (size_t i = 0; i < plan.instances.size(); i++)
                    if (plan.instances[i].source == set.instance)
                        at = int(i);
                plan_of_[r].push_back(at);
            }
            order_.push_back(r);
        }
        std::stable_sort(order_.begin(), order_.end(), [&](size_t x, size_t y) {
            if (rules[x].priority != rules[y].priority)
                return rules[x].priority < rules[y].priority;
            return rules[x].order < rules[y].order;
        });
    }

    void step(const Signals &sig, double frame_ms)
    {
        if (!rules_)
            return;
        double dt = std::clamp(frame_ms, 0.0, 100.0);
        bool any = false;
        for (size_t r = 0; r < rules_->size(); r++) {
            const RuleSpec &rule = (*rules_)[r];
            State &st = state_[r];
            bool was = st.on;
            if (rule.event) {
                bool hit = false;
                for (auto &c : rule.when)
                    hit |= event_passes(c, sig);
                st.left = hit ? rule.for_ms : std::max(0.0, st.left - dt);
                st.on = st.left > 0;
                st.target = st.on ? 1 : 0;
            } else if (rule.has_ramp) {
                double t = 0;
                if (sig.has[rule.ramp_signal] && rule.ramp_to != rule.ramp_from)
                    t = std::clamp((sig.num[rule.ramp_signal] - rule.ramp_from) / (rule.ramp_to - rule.ramp_from), 0.0, 1.0);
                st.target = t;
                st.on = t > 0;
            } else {
                bool cond = true;
                for (auto &c : rule.when)
                    cond = cond && holds(c, sig);
                st.true_for = cond ? st.true_for + dt : 0;
                st.false_for = cond ? 0 : st.false_for + dt;
                if (!st.on && cond && st.true_for >= rule.min_ms)
                    st.on = true;
                if (st.on && !cond && st.false_for > rule.hold_ms)
                    st.on = false;
                st.target = st.on ? 1 : 0;
            }
            st.changed = st.on != was;
            if (st.target > st.w)
                st.w = rule.fade_in_ms > 0 ? std::min(st.target, st.w + dt / rule.fade_in_ms) : st.target;
            else if (st.target < st.w)
                st.w = rule.fade_out_ms > 0 ? std::max(st.target, st.w - dt / rule.fade_out_ms) : st.target;
            st.a = st.w * st.w * (3 - 2 * st.w);
            any |= st.a > 0;
        }
        opts_ = base_;
        std::fill(off_.begin(), off_.end(), 0);
        if (!any)
            return;
        for (size_t r : order_) {
            double a = state_[r].a;
            if (a <= 0)
                continue;
            const RuleSpec &rule = (*rules_)[r];
            for (size_t k = 0; k < rule.sets.size(); k++) {
                int at = plan_of_[r][k];
                if (at < 0)
                    continue;
                const RuleSet &set = rule.sets[k];
                if (set.mode == SET_OFF) {
                    if (a >= 0.5)
                        off_[size_t(at)] = 1;
                    continue;
                }
                for (size_t c = 0; c < set.values.size(); c++) {
                    float &v = opts_[size_t(at)][size_t(set.slot) + c];
                    float target = set.values[c];
                    if (set.mode == SET_SWITCH) {
                        if (a >= 0.5)
                            v = target;
                    } else {
                        v = v + (target - v) * float(a);
                        if (set.mode == SET_ROUND)
                            v = std::round(v);
                    }
                }
            }
        }
    }

    size_t rules() const { return state_.size(); }
    const State &state(size_t rule) const { return state_[rule]; }
    const float *options(size_t instance) const { return opts_[instance].data(); }
    bool off(size_t instance) const { return off_[instance] != 0; }

private:
    static bool number_holds(const RuleCond &c, double x)
    {
        switch (c.op) {
        case OP_ABOVE:
            return x > c.value;
        case OP_BELOW:
            return x < c.value;
        case OP_BETWEEN:
            return x > c.lo && x < c.hi;
        default:
            return false;
        }
    }

    static bool holds(const RuleCond &c, const Signals &sig)
    {
        if (!sig.has[c.signal])
            return false;
        switch (c.op) {
        case OP_TRUE:
            return sig.num[c.signal] != 0;
        case OP_IS: {
            if (c.signal == S_CAR_SURFACE) {
                int n = 0;
                for (int w : sig.wheel)
                    n += w >= 0 && w < 32 && (c.mask >> w & 1);
                return n >= c.wheels;
            }
            int v = int(sig.num[c.signal]);
            return v >= 0 && v < 32 && (c.mask >> v & 1);
        }
        default:
            return number_holds(c, sig.num[c.signal]);
        }
    }

    static bool event_passes(const RuleCond &c, const Signals &sig)
    {
        for (uint8_t i = 0; i < sig.n_events[c.signal]; i++)
            if (c.op == OP_EVENT || number_holds(c, sig.magnitude[c.signal][i]))
                return true;
        return false;
    }

    const std::vector<RuleSpec> *rules_ = nullptr;
    std::vector<State> state_;
    std::vector<std::array<float, SLOTS>> base_, opts_;
    std::vector<char> off_;
    std::vector<std::vector<int>> plan_of_;
    std::vector<size_t> order_;
};

}
