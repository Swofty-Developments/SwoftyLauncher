#include "core/render/postfx/shader_config.hpp"

#include "core/render/postfx/option_logic.hpp"
#include "core/render/postfx/pipeline.hpp"
#include "core/runtime/runtime.hpp"
#include "core/runtime/settings.hpp"

#include <swofty/mixin.hpp>

#include <algorithm>
#include <cstring>
#include <iterator>

namespace render::postfx {

namespace {

bool read_tex_file(PassTexture &t)
{
    std::string bytes = runtime::read_file(runtime::under_swofty(t.file), size_t(64) << 20);
    uint32_t head[4] = {};
    if (bytes.size() < 16 || memcmp(bytes.data(), "SWTX", 4) != 0)
        return false;
    memcpy(head, bytes.data(), 16);
    t.width = head[1];
    t.height = head[2];
    if (!t.width || !t.height || t.width > 8192 || t.height > 8192 || bytes.size() < 16 + size_t(t.width) * t.height * 4)
        return false;
    t.pixels = bytes.substr(16, size_t(t.width) * t.height * 4);
    t.format = DXGI_FORMAT_R8G8B8A8_UNORM;
    return true;
}

bool setting_value(const std::string &addon, const std::string &option, std::string &text)
{
    size_t current = 0;
    std::vector<std::string> values;
    if (!settings_state(addon, option, current, nullptr, nullptr, &values) || current >= values.size())
        return false;
    text = values[current];
    return true;
}

void drop_textures(std::vector<IUnknown *> &dead, PassTexture &t)
{
    dead.push_back(t.res);
    dead.push_back(t.upload);
    t.res = t.upload = nullptr;
}

PassTexture fresh_texture(const postfx::PlanTex &pt)
{
    PassTexture t;
    t.name = pt.name;
    t.file = pt.decl.file;
    t.scale = pt.decl.scale;
    t.width = pt.decl.width;
    t.height = pt.decl.height;
    t.persistent = pt.decl.persistent;
    t.mips = pt.decl.mips;
    t.format = DXGI_FORMAT(pt.decl.format);
    if (!t.file.empty() && !read_tex_file(t)) {
        swofty::log("[shaders] texture %s is left out: %s is missing or not a texture", t.name.c_str(), t.file.c_str());
        t.file.clear();
        t.name += " (missing)";
    }
    return t;
}

bool game_in_front()
{
    DWORD pid = 0;
    GetWindowThreadProcessId(GetForegroundWindow(), &pid);
    return pid == GetCurrentProcessId();
}

}

std::string ShaderConfig::describe(const RenderPass &p)
{
    return p.inst >= 0 ? postfx::pass_title(active.name, p.effect, p.label, p.name) : p.addon + "'s pass " + p.name;
}

bool ShaderConfig::addon_off(const std::string &id)
{
    return std::any_of(toggles.begin(), toggles.end(), [&](const AddonToggle &t) { return t.off && t.addon_id == id; });
}

ID3D12PipelineState *ShaderConfig::effect_pso(const std::string &path)
{
    auto it = effect_psos.find(path);
    if (it != effect_psos.end())
        return it->second;
    ID3D12PipelineState *pso = nullptr;
    std::string bytes = runtime::read_file(runtime::under_swofty(path), size_t(4) << 20);
    if (bytes.size() < 32 || memcmp(bytes.data(), "DXBC", 4) != 0)
        swofty::log("[shaders] %s %s", path.c_str(), bytes.empty() ? "has no DXIL" : "is not a DXIL container");
    else {
        D3D12_COMPUTE_PIPELINE_STATE_DESC pd{};
        pd.pRootSignature = pipeline.gpu.root;
        pd.CS.pShaderBytecode = bytes.data();
        pd.CS.BytecodeLength = bytes.size();
        HRESULT hr = pipeline.gpu.device->CreateComputePipelineState(&pd, IID_PPV_ARGS(&pso));
        if (FAILED(hr))
            swofty::log("[shaders] the pipeline of %s could not be made (0x%08lx)", path.c_str(), hr);
    }
    effect_psos[path] = pso;
    return pso;
}

void ShaderConfig::drop_instance(int inst)
{
    for (auto &p : passes)
        if (p.inst == inst)
            p.off = true;
    swofty::log("[shaders] instance %d (%s) is off: a pipeline of its effect could not be made", inst,
                inst >= 0 && size_t(inst) < plan.instances.size()
                    ? postfx::instance_name(plan.instances[size_t(inst)].effect, plan.instances[size_t(inst)].label).c_str()
                    : "?");
}

std::vector<int> ShaderConfig::live_passes(const frontend::view::State &v, unsigned *idle)
{
    std::vector<int> live;
    if (shaders_off)
        return live;
    auto consider = [&](size_t i) {
        RenderPass &p = passes[i];
        if (p.inst >= 0 && !p.pso && !p.off) {
            p.pso = effect_pso(p.shader);
            if (!p.pso)
                drop_instance(p.inst);
        }
        const char *why = p.off                   ? "off"
                          : p.ruled_off           ? "a rule turned it off"
                          : !p.on                 ? "its option turns it off"
                          : addon_off(p.addon_id) ? "its addon's key turned it off"
                                                  : postfx::ineligible(p.when, v.kind, v.flags, v.valid);
        if (!why) {
            live.push_back(int(i));
            return;
        }
        if (idle)
            *idle |= 1u << (p.off                   ? postfx::IDLE_PASS_OFF
                            : p.ruled_off           ? postfx::IDLE_RULE
                            : !p.on                 ? postfx::IDLE_OPTION
                            : addon_off(p.addon_id) ? postfx::IDLE_ADDON_KEY
                                                    : postfx::IDLE_VIEW);
        if (!p.off && pipeline.report.tracing())
            pipeline.report.trace.skipped += (pipeline.report.trace.skipped.empty() ? "" : ", ") + p.name + " (" + why + ")";
    };
    for (size_t i = legacy_passes; i < passes.size() && live.size() < MAX_PASSES; i++)
        consider(i);
    for (size_t i = 0; i < legacy_passes && live.size() < MAX_PASSES; i++)
        consider(i);
    return live;
}

void ShaderConfig::refresh_live()
{
    std::string text;
    float v = 0;
    for (auto &p : passes) {
        for (auto &l : p.live)
            if (l.slot < 16 && setting_value(p.addon_id, l.option, text) && postfx::option_value(text, v))
                p.options[l.slot] = v;
        if (!p.enabled.empty() && setting_value(p.addon_id, p.enabled, text) && postfx::option_value(text, v))
            p.on = v != 0;
    }
    for (auto &t : toggles)
        if (setting_value(t.addon_id, t.option, text)) {
            t.vk = postfx::key_code(text);
            t.key = text;
        }
}

void ShaderConfig::install_plan(postfx::Plan next)
{
    if (postfx::plan_shape(next) == postfx::plan_shape(plan) && next.instances.size() == plan.instances.size()) {
        for (size_t i = 0; i < next.instances.size(); i++)
            memcpy(plan.instances[i].options, next.instances[i].options, sizeof plan.instances[i].options);
        for (size_t i = legacy_passes; i < passes.size(); i++)
            memcpy(passes[i].options, plan.instances[size_t(passes[i].inst)].options, sizeof passes[i].options);
        return;
    }
    std::vector<int> from = postfx::reuse_textures(plan, next);
    std::vector<PassTexture> old_tex(std::make_move_iterator(textures.begin() + long(legacy_textures)),
                                     std::make_move_iterator(textures.end()));
    std::vector<std::vector<ID3D12Resource *>> old_own(FRAMES);
    for (UINT k = 0; k < FRAMES; k++)
        if (pipeline.frame.work[k].orig)
            old_own[k].assign(pipeline.frame.work[k].tex.begin() + long(legacy_textures), pipeline.frame.work[k].tex.end());
    textures.resize(legacy_textures);
    passes.resize(legacy_passes);
    std::vector<IUnknown *> dead;
    std::vector<bool> taken(old_tex.size(), false);
    for (size_t i = 0; i < next.textures.size(); i++) {
        if (from[i] >= 0) {
            taken[size_t(from[i])] = true;
            textures.push_back(std::move(old_tex[size_t(from[i])]));
        } else
            textures.push_back(fresh_texture(next.textures[i]));
    }
    for (size_t j = 0; j < old_tex.size(); j++)
        if (!taken[j])
            drop_textures(dead, old_tex[j]);
    for (UINT k = 0; k < FRAMES; k++) {
        if (old_own[k].empty() && !pipeline.frame.work[k].orig)
            continue;
        SlotTextures &wk = pipeline.frame.work[k];
        std::vector<ID3D12Resource *> own(wk.tex.begin(), wk.tex.begin() + long(legacy_textures));
        for (size_t i = 0; i < next.textures.size(); i++)
            own.push_back(from[i] >= 0 ? old_own[k][size_t(from[i])] : nullptr);
        for (size_t j = 0; j < old_own[k].size(); j++)
            if (!taken[j])
                dead.push_back(old_own[k][j]);
        wk.tex = std::move(own);
    }
    pipeline.retire(std::move(dead));
    for (auto &pp : next.passes) {
        RenderPass p;
        p.inst = pp.instance;
        const postfx::PlanInstance &pi = next.instances[size_t(pp.instance)];
        p.addon_id = pi.decl->addon;
        p.effect = pi.effect;
        p.label = pi.label;
        p.name = pp.decl->name;
        p.shader = pp.decl->dxil;
        p.budget_ms = pp.decl->budget_ms;
        p.threads[0] = pp.decl->threads[0];
        p.threads[1] = pp.decl->threads[1];
        memcpy(p.options, pi.options, sizeof p.options);
        for (size_t i = 0; i < pp.n_inputs; i++)
            p.inputs[i] = pp.inputs[i] >= 0 ? int(legacy_textures) + pp.inputs[i] : pp.inputs[i];
        p.n_inputs = UINT(pp.n_inputs);
        p.output = pp.output >= 0 ? int(legacy_textures) + pp.output : -1;
        passes.push_back(std::move(p));
    }
    plan = std::move(next);
    uses_previous = legacy_previous || plan.uses_previous();
    for (auto &sl : pipeline.recorder.slots)
        sl.passes = 0;
    if (pipeline.frame.heap && !pipeline.frame.fit_frame()) {
        pipeline.broken = true;
        swofty::log("[shaders] the textures of the new config could not be made; post-processing is off");
    }
}

void ShaderConfig::apply_active(postfx::ActiveSpec next)
{
    postfx::Plan p = postfx::make_plan(next, effects);
    for (auto &d : p.dropped)
        swofty::log("[shaders] %s; left out", d.c_str());
    size_t n_passes = p.passes.size(), n_effects = p.instances.size();
    shader_vk = postfx::key_code(next.toggle_key);
    install_plan(std::move(p));
    active = std::move(next);
    pipeline.rules.reset(active, plan, passes);
    swofty::log("[shaders] config \"%s\" (serial %llu): %zu effect(s), %zu pass(es), toggle key %s", active.name.c_str(),
                (unsigned long long)active.serial, n_effects, n_passes, shader_vk ? active.toggle_key.c_str() : "none");
}

void ShaderConfig::check_active(ULONGLONG now, bool force)
{
    if (active_file.empty() || (!force && now - active_checked < 250))
        return;
    active_checked = now;
    WIN32_FILE_ATTRIBUTE_DATA fa{};
    if (!GetFileAttributesExW(active_file.c_str(), GetFileExInfoStandard, &fa))
        return;
    if (active_told && !CompareFileTime(&fa.ftLastWriteTime, &active_at))
        return;
    postfx::ActiveSpec next;
    if (!postfx::parse_active(runtime::read_file(active_file, size_t(8) << 20), next))
        return;
    active_at = fa.ftLastWriteTime;
    active_told = true;
    apply_active(std::move(next));
}

void ShaderConfig::load_effects()
{
    effects = postfx::parse_effects(runtime::read_file(runtime::under_swofty("shaders/effects.json"), size_t(16) << 20));
    size_t n = 0;
    for (auto &e : effects)
        n += e.passes.size();
    if (!effects.empty())
        swofty::log("[shaders] effects: %zu effect(s), %zu pass(es) ready to load", effects.size(), n);
}

void ShaderConfig::poll_toggles()
{
    bool front = game_in_front();
    for (auto &t : toggles) {
        bool raw = t.vk && front && (GetAsyncKeyState(t.vk) & 0x8000);
        if (raw && !t.down && !runtime::typing("a shader key")) {
            t.off = !t.off;
            swofty::log("[postfx] %s's passes are %s (its key %s)", t.addon.c_str(), t.off ? "off" : "on", t.key.c_str());
            runtime::hud_toast(t.off ? "SHADERS OFF" : "SHADERS ON", t.addon, 2.0f, t.addon_id);
        }
        t.down = raw;
    }
    bool raw = shader_vk && front && (GetAsyncKeyState(shader_vk) & 0x8000);
    if (raw && !shader_key_down && !runtime::typing("a shader key")) {
        shaders_off = !shaders_off;
        swofty::log("[shaders] shaders are %s (the toggle key %s)", shaders_off ? "off" : "on", active.toggle_key.c_str());
        runtime::hud_toast(shaders_off ? "SHADERS OFF" : "SHADERS ON", active.name, 2.0f, "");
    }
    shader_key_down = raw;
}

int ShaderConfig::texture_named(const std::string &name)
{
    if (name == "original")
        return IN_ORIGINAL;
    if (name == "previous")
        return IN_PREVIOUS;
    for (size_t i = 0; i < textures.size(); i++)
        if (textures[i].name == name)
            return int(i);
    return IN_NONE;
}

bool ShaderConfig::load_passes()
{
    const json::Value &cfg = runtime::config();
    for (auto &e : cfg["postfx_textures"].a) {
        PassTexture t;
        t.name = e["name"].str();
        t.file = e["file"].str();
        if (e["scale"].type == json::Value::Num)
            t.scale = e["scale"].n;
        if (e["width"].type == json::Value::Num && e["height"].type == json::Value::Num) {
            t.width = UINT(e["width"].n);
            t.height = UINT(e["height"].n);
        }
        t.persistent = e["persistent"].type == json::Value::Bool && e["persistent"].b;
        int f = postfx::format_of(e["format"].str("rgba16f"));
        t.format = f ? DXGI_FORMAT(f) : DXGI_FORMAT_R16G16B16A16_FLOAT;
        if (!t.file.empty() && !read_tex_file(t)) {
            swofty::log("[postfx] texture %s is left out: %s is missing or not a texture", t.name.c_str(), t.file.c_str());
            t.file.clear();
            t.name += " (missing)";
        }
        textures.push_back(std::move(t));
    }
    for (auto &e : cfg["postfx"].a) {
        RenderPass p;
        p.addon = e["addon"].str("an addon");
        p.addon_id = e["addon_id"].str();
        p.name = e["name"].str("pass");
        p.shader = e["shader"].str();
        p.enabled = e["enabled"].str();
        const json::Value &b = e["budget_ms"];
        if (b.type == json::Value::Num && b.n > 0)
            p.budget_ms = b.n;
        size_t k = 0;
        for (auto &o : e["options"].a)
            if (k < 16)
                p.options[k++] = o.type == json::Value::Num ? float(o.n) : 0.0f;
        for (auto &l : e["live"].a)
            if (l["slot"].type == json::Value::Num)
                p.live.push_back({UINT(l["slot"].n), l["option"].str()});
        const char *bad = nullptr;
        for (auto &in : e["inputs"].a) {
            int t = texture_named(in.str());
            if (t == IN_NONE || p.n_inputs >= INPUTS)
                bad = "names an input that is not there";
            else
                p.inputs[p.n_inputs++] = t;
        }
        std::string out = e["output"].str();
        if (!out.empty()) {
            p.output = texture_named(out);
            if (p.output < 0 || !textures[size_t(p.output)].file.empty())
                bad = "writes a texture that is not there or comes from a file";
            for (UINT i = 0; i < p.n_inputs; i++)
                if (p.output >= 0 && p.inputs[i] == p.output)
                    bad = "reads the texture it writes";
        }
        const auto &th = e["threads"].a;
        if (th.size() == 2 && th[0].n >= 1 && th[1].n >= 1) {
            p.threads[0] = UINT(th[0].n);
            p.threads[1] = UINT(th[1].n);
        }
        for (auto &v : e["when"]["views"].a)
            p.when.views |= postfx::view_bit(v.str());
        for (auto &f : e["when"]["skip"].a)
            p.when.skip |= postfx::flag_bit(f.str());
        std::string bytes = runtime::read_file(runtime::under_swofty(p.shader), size_t(4) << 20);
        if (bytes.size() < 32 || memcmp(bytes.data(), "DXBC", 4) != 0)
            bad = bytes.empty() ? "has no DXIL" : "is not a DXIL container";
        if (bad) {
            swofty::log("[postfx] %s's pass %s is off: it %s (%s)", p.addon.c_str(), p.name.c_str(), bad, p.shader.c_str());
            continue;
        }
        p.dxil.assign(bytes.begin(), bytes.end());
        uses_previous |= std::find(p.inputs, p.inputs + p.n_inputs, IN_PREVIOUS) != p.inputs + p.n_inputs;
        passes.push_back(std::move(p));
    }
    for (auto &e : cfg["postfx_toggles"].a) {
        AddonToggle t;
        t.addon_id = e["addon_id"].str();
        t.option = e["option"].str();
        t.addon = t.addon_id;
        for (auto &p : passes)
            if (p.addon_id == t.addon_id)
                t.addon = p.addon;
        if (!t.addon_id.empty() && !t.option.empty())
            toggles.push_back(std::move(t));
    }
    legacy_passes = passes.size();
    legacy_textures = textures.size();
    legacy_previous = uses_previous;
    return !passes.empty();
}

}
