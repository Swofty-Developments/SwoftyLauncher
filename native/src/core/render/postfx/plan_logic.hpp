#pragma once
#include "core/render/postfx/active_logic.hpp"
#include "core/render/postfx/effect_logic.hpp"

#include <cstddef>
#include <cstdint>
#include <string>
#include <vector>

namespace render::postfx {

// An instance's params as the 32 option slots: missing ones are 0, extra ones are dropped.
inline void pack_slots(const std::vector<float> &params, float out[SLOTS])
{
    for (size_t i = 0; i < SLOTS; i++)
        out[i] = i < params.size() ? params[i] : 0.0f;
}

// One texture of one instance. Its name is "<instance>/<texture>".
struct PlanTex {
    int instance = 0;
    std::string effect, name;
    TexDecl decl;
};

// One pass of one instance. Inputs and output index the plan's textures, or are IN_ORIGINAL /
// IN_PREVIOUS / IN_NONE (an output of IN_NONE writes the picture).
struct PlanPass {
    int instance = 0;
    const PassDecl *decl = nullptr;
    int inputs[MAX_INPUTS] = {IN_NONE, IN_NONE, IN_NONE, IN_NONE, IN_NONE, IN_NONE, IN_NONE, IN_NONE};
    size_t n_inputs = 0;
    int output = IN_NONE;
};

struct PlanInstance {
    int source = 0;
    std::string effect, label;
    const EffectDecl *decl = nullptr;
    float options[SLOTS] = {};
};

struct Plan {
    std::vector<PlanInstance> instances;
    std::vector<PlanTex> textures;
    std::vector<PlanPass> passes;
    std::vector<std::string> dropped;

    bool uses_previous() const
    {
        for (auto &p : passes)
            for (size_t i = 0; i < p.n_inputs; i++)
                if (p.inputs[i] == IN_PREVIOUS)
                    return true;
        return false;
    }
};

inline std::string instance_name(const std::string &effect, const std::string &label)
{
    return label.empty() ? effect : effect + "." + label;
}

// The instance plan for an active config. An instance whose effect is unknown, or has a pass that names
// an input that is not there, reads its own output or writes a texture from a file, is dropped with the
// reason in dropped; the rest keep their order. Instance numbers are positions among the kept instances.
inline Plan make_plan(const ActiveSpec &active, const std::vector<EffectDecl> &effects)
{
    Plan plan;
    for (size_t n = 0; n < active.instances.size(); n++) {
        const InstanceSpec &spec = active.instances[n];
        std::string who = instance_name(spec.effect, spec.label);
        const EffectDecl *fx = nullptr;
        for (auto &e : effects)
            if (e.id == spec.effect) {
                fx = &e;
                break;
            }
        if (!fx) {
            plan.dropped.push_back(who + ": no such effect");
            continue;
        }
        size_t tex_base = plan.textures.size(), pass_base = plan.passes.size();
        const char *bad = nullptr;
        auto texture_named = [&](const std::string &name) {
            if (name == "original")
                return IN_ORIGINAL;
            if (name == "previous")
                return IN_PREVIOUS;
            for (size_t i = tex_base; i < plan.textures.size(); i++)
                if (plan.textures[i].decl.name == name)
                    return int(i);
            return IN_NONE;
        };
        for (auto &t : fx->textures) {
            PlanTex x;
            x.effect = fx->id;
            x.decl = t;
            plan.textures.push_back(std::move(x));
        }
        for (auto &pd : fx->passes) {
            PlanPass p;
            p.decl = &pd;
            for (auto &in : pd.inputs) {
                int t = texture_named(in);
                if (t == IN_NONE || p.n_inputs >= MAX_INPUTS)
                    bad = "a pass names an input that is not there";
                else
                    p.inputs[p.n_inputs++] = t;
            }
            if (!pd.output.empty()) {
                p.output = texture_named(pd.output);
                if (p.output < 0 || !plan.textures[size_t(p.output)].decl.file.empty())
                    bad = "a pass writes a texture that is not there or comes from a file";
                for (size_t i = 0; i < p.n_inputs; i++)
                    if (p.output >= 0 && p.inputs[i] == p.output)
                        bad = "a pass reads the texture it writes";
            }
            plan.passes.push_back(p);
        }
        if (fx->slots > SLOTS)
            bad = "it needs more than 32 option slots";
        if (bad) {
            plan.dropped.push_back(who + ": " + bad);
            plan.textures.resize(tex_base);
            plan.passes.resize(pass_base);
            continue;
        }
        PlanInstance inst;
        inst.effect = spec.effect;
        inst.label = spec.label;
        inst.source = spec.index;
        inst.decl = fx;
        pack_slots(spec.params, inst.options);
        plan.instances.push_back(std::move(inst));
        for (size_t i = pass_base; i < plan.passes.size(); i++)
            plan.passes[i].instance = int(plan.instances.size() - 1);
        for (size_t i = tex_base; i < plan.textures.size(); i++) {
            plan.textures[i].instance = int(plan.instances.size() - 1);
            plan.textures[i].name = std::to_string(plan.instances.size() - 1) + "/" + plan.textures[i].decl.name;
        }
    }
    return plan;
}

// What decides the passes, textures and pipelines a plan needs: two plans with the same shape differ
// only in their option values.
inline std::string plan_shape(const Plan &plan)
{
    std::string out;
    for (auto &i : plan.instances) {
        out += instance_name(i.effect, i.label) + "{";
        for (auto &p : i.decl->passes)
            out += p.dxil + ";";
        out += "}";
    }
    return out;
}

// For each texture of a new plan, the texture of the old plan it takes over (-1 for none): the same
// instance position running the same effect with the same texture declaration.
inline std::vector<int> reuse_textures(const Plan &old, const Plan &now)
{
    std::vector<int> from(now.textures.size(), -1);
    for (size_t i = 0; i < now.textures.size(); i++)
        for (size_t j = 0; j < old.textures.size(); j++)
            if (old.textures[j].name == now.textures[i].name && old.textures[j].effect == now.textures[i].effect &&
                old.textures[j].decl == now.textures[i].decl) {
                from[i] = int(j);
                break;
            }
    return from;
}

// Pass title for logs: "<config> · <effect>[.<label>] · <pass>".
inline std::string pass_title(const std::string &config, const std::string &effect, const std::string &label,
                              const std::string &pass)
{
    return config + " \xC2\xB7 " + instance_name(effect, label) + " \xC2\xB7 " + pass;
}

}
