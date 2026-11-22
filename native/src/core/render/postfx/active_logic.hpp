#pragma once
#include "core/render/postfx/effect_logic.hpp"
#include "core/render/postfx/signal_logic.hpp"
#include "core/runtime/json.hpp"

#include <cstdint>
#include <string>
#include <vector>

namespace render::postfx {

enum CondOp : uint8_t { OP_TRUE, OP_IS, OP_ABOVE, OP_BELOW, OP_BETWEEN, OP_EVENT };

struct RuleCond {
    int signal = 0;
    CondOp op = OP_TRUE;
    uint32_t mask = 0;
    int wheels = 1;
    double value = 0, lo = 0, hi = 0;
};

enum SetMode : uint8_t { SET_OFF, SET_BLEND, SET_ROUND, SET_SWITCH };

struct RuleSet {
    int instance = 0;
    int slot = 0;
    SetMode mode = SET_BLEND;
    std::vector<float> values;
};

struct RuleSpec {
    std::string name;
    int priority = 0, order = 0;
    bool event = false;
    std::vector<RuleCond> when;
    bool has_ramp = false;
    int ramp_signal = 0;
    double ramp_from = 0, ramp_to = 1;
    double min_ms = 0, hold_ms = 0, for_ms = 400, fade_in_ms = 150, fade_out_ms = 300;
    std::vector<RuleSet> sets;
};

// The config swofty/shaders/active.json says runs now.
struct ActiveSpec {
    uint64_t serial = 0;
    std::string name, source, forced_by, toggle_key;
    std::vector<InstanceSpec> instances;
    std::vector<RuleSpec> rules;
};

inline bool parse_rule(const json::Value &r, RuleSpec &out)
{
    if (r.type != json::Value::Obj)
        return false;
    out = {};
    out.name = r["name"].str();
    out.priority = int(r["priority"].num());
    out.order = int(r["order"].num());
    out.event = r["event"].type == json::Value::Bool && r["event"].b;
    auto num = [&](const json::Value &v, double def) { return v.type == json::Value::Num ? v.n : def; };
    out.min_ms = num(r["min_ms"], 0);
    out.hold_ms = num(r["hold_ms"], 0);
    out.for_ms = num(r["for_ms"], 400);
    out.fade_in_ms = num(r["fade_in_ms"], 150);
    out.fade_out_ms = num(r["fade_out_ms"], 300);
    for (auto &w : r["when"].a) {
        RuleCond c;
        c.signal = signal_index(w["signal"].str());
        if (c.signal < 0)
            return false;
        std::string op = w["op"].str();
        c.op = op == "true"      ? OP_TRUE
               : op == "is"      ? OP_IS
               : op == "above"   ? OP_ABOVE
               : op == "below"   ? OP_BELOW
               : op == "between" ? OP_BETWEEN
               : op == "event"   ? OP_EVENT
                                 : OP_TRUE;
        if (op != "true" && op != "is" && op != "above" && op != "below" && op != "between" && op != "event")
            return false;
        for (auto &v : w["values"].a)
            if (v.type == json::Value::Num && v.n >= 0 && v.n < 32)
                c.mask |= 1u << unsigned(v.n);
        c.wheels = w["wheels"].type == json::Value::Num ? int(w["wheels"].n) : 1;
        c.value = num(w["value"], 0);
        c.lo = num(w["lo"], 0);
        c.hi = num(w["hi"], 0);
        out.when.push_back(c);
    }
    const json::Value &ramp = r["ramp"];
    if (ramp.type == json::Value::Obj) {
        out.ramp_signal = signal_index(ramp["signal"].str());
        if (out.ramp_signal < 0)
            return false;
        out.has_ramp = true;
        out.ramp_from = num(ramp["from"], 0);
        out.ramp_to = num(ramp["to"], 1);
    }
    for (auto &s : r["set"].a) {
        RuleSet x;
        x.instance = int(s["instance"].num(-1));
        if (x.instance < 0)
            continue;
        if (s["enabled"].type == json::Value::Bool && !s["enabled"].b) {
            x.mode = SET_OFF;
            out.sets.push_back(std::move(x));
            continue;
        }
        std::string mode = s["mode"].str();
        x.mode = mode == "round" ? SET_ROUND : mode == "switch" ? SET_SWITCH : SET_BLEND;
        x.slot = int(s["slot"].num(-1));
        for (auto &v : s["values"].a)
            x.values.push_back(v.type == json::Value::Num ? float(v.n) : 0.0f);
        if (x.slot < 0 || x.values.empty() || size_t(x.slot) + x.values.size() > SLOTS)
            continue;
        out.sets.push_back(std::move(x));
    }
    return true;
}

inline bool parse_active(const std::string &text, ActiveSpec &out)
{
    json::Value root;
    if (!json::parse(text, root) || root.type != json::Value::Obj)
        return false;
    out = {};
    out.serial = uint64_t(root["serial"].num());
    out.name = root["name"].str();
    out.source = root["source"].str();
    out.forced_by = root["forced_by"].str();
    out.toggle_key = root["toggle_key"].str();
    for (auto &i : root["instances"].a) {
        InstanceSpec s;
        s.effect = i["effect"].str();
        s.label = i["label"].str();
        for (auto &v : i["params"].a)
            s.params.push_back(v.type == json::Value::Num ? float(v.n) : 0.0f);
        s.index = int(&i - root["instances"].a.data());
        if (!s.effect.empty())
            out.instances.push_back(std::move(s));
    }
    for (auto &r : root["rules"].a) {
        RuleSpec rule;
        if (parse_rule(r, rule))
            out.rules.push_back(std::move(rule));
    }
    return true;
}

}
