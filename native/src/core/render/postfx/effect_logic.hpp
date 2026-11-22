#pragma once
#include "core/render/postfx/texture_logic.hpp"
#include "core/runtime/json.hpp"

#include <cstddef>
#include <cstdint>
#include <string>
#include <vector>

namespace render::postfx {

inline constexpr int IN_NONE = -1, IN_ORIGINAL = -2, IN_PREVIOUS = -3;
inline constexpr size_t MAX_INPUTS = 8, SLOTS = 32;

// Root constant indices of swofty_options and swofty_params in the SwoftyPostFx cbuffer.
inline constexpr size_t OPTIONS_AT = 8, PARAMS_AT = 36, CONSTANT_COUNT = 56;
// Root constant index of swofty_data_size: the bytes of the pass's data in t10, 0 without any.
inline constexpr size_t DATA_SIZE_AT = 35;

struct TexDecl {
    std::string name, file;
    double scale = 1;
    uint32_t width = 0, height = 0;
    int format = 10;
    bool persistent = false;
    uint32_t mips = 1;

    bool operator==(const TexDecl &o) const
    {
        return name == o.name && file == o.file && scale == o.scale && width == o.width && height == o.height &&
               format == o.format && persistent == o.persistent && mips == o.mips;
    }
};

struct PassDecl {
    std::string name, dxil, output;
    std::vector<std::string> inputs;
    uint32_t threads[2] = {8, 8};
    bool raw = false;
    double budget_ms = 1.5;
};

struct EffectDecl {
    std::string id, name, addon;
    unsigned slots = 0;
    std::vector<TexDecl> textures;
    std::vector<PassDecl> passes;
};

// The effects of swofty/shaders/effects.json by id; an entry without an id or without passes is left out.
inline std::vector<EffectDecl> parse_effects(const std::string &text)
{
    std::vector<EffectDecl> out;
    json::Value root;
    if (!json::parse(text, root))
        return out;
    for (auto &e : root["effects"].a) {
        EffectDecl d;
        d.id = e["id"].str();
        d.name = e["name"].str(d.id.c_str());
        d.addon = e["addon"].str();
        d.slots = unsigned(e["slots"].num());
        for (auto &t : e["textures"].a) {
            TexDecl x;
            x.name = t["name"].str();
            x.file = t["file"].str();
            if (t["scale"].type == json::Value::Num)
                x.scale = t["scale"].n;
            if (t["width"].type == json::Value::Num && t["height"].type == json::Value::Num) {
                x.width = uint32_t(t["width"].n);
                x.height = uint32_t(t["height"].n);
            }
            x.persistent = t["persistent"].type == json::Value::Bool && t["persistent"].b;
            if (t["mips"].type == json::Value::Num && t["mips"].n >= 1)
                x.mips = t["mips"].n > MAX_MIPS ? MAX_MIPS : uint32_t(t["mips"].n);
            int f = format_of(t["format"].str("rgba16f"));
            x.format = f ? f : 10;
            d.textures.push_back(std::move(x));
        }
        for (auto &p : e["passes"].a) {
            PassDecl x;
            x.name = p["name"].str("pass");
            x.dxil = p["dxil"].str();
            x.output = p["output"].str();
            for (auto &in : p["inputs"].a)
                x.inputs.push_back(in.str());
            const auto &th = p["threads"].a;
            if (th.size() == 2 && th[0].n >= 1 && th[1].n >= 1) {
                x.threads[0] = uint32_t(th[0].n);
                x.threads[1] = uint32_t(th[1].n);
            }
            x.raw = p["raw"].type == json::Value::Bool && p["raw"].b;
            if (p["budget_ms"].type == json::Value::Num && p["budget_ms"].n > 0)
                x.budget_ms = p["budget_ms"].n;
            d.passes.push_back(std::move(x));
        }
        if (!d.id.empty() && !d.passes.empty())
            out.push_back(std::move(d));
    }
    return out;
}

struct InstanceSpec {
    std::string effect, label;
    std::vector<float> params;
    int index = 0;
};

}
