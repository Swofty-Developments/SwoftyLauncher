#pragma once
#include "core/frontend/view_state.hpp"
#include "core/render/postfx/active_logic.hpp"
#include "core/render/postfx/pass.hpp"
#include "core/render/postfx/plan_logic.hpp"

#include <windows.h>

#include <d3d12.h>

#include <string>
#include <unordered_map>
#include <vector>

namespace render::postfx {

class Pipeline;

// The passes and textures the pipeline runs: the profile's own (assets.json, "legacy"), then the instances of the
// shader config in swofty/shaders/active.json built from swofty/shaders/effects.json, with the addons' toggle keys,
// the shader toggle key and the options ADDON SETTINGS changes live.
class ShaderConfig {
public:
    explicit ShaderConfig(Pipeline &owner) : pipeline(owner) {}

    std::vector<RenderPass> passes;
    std::vector<PassTexture> textures;
    std::vector<AddonToggle> toggles;
    bool uses_previous = false, legacy_previous = false;
    size_t legacy_passes = 0, legacy_textures = 0;
    std::vector<EffectDecl> effects;
    ActiveSpec active;
    Plan plan;
    std::unordered_map<std::string, ID3D12PipelineState *> effect_psos;
    std::wstring active_file;
    FILETIME active_at{};
    ULONGLONG active_checked = 0;
    bool active_told = false, shaders_off = false, shader_key_down = false;
    int shader_vk = 0;

    bool load_passes();
    void load_effects();
    void check_active(ULONGLONG now, bool force);
    void apply_active(ActiveSpec next);
    // Replaces the instance passes and textures with a new plan's, keeping the textures of instances that
    // run the same effect where they did.
    void install_plan(Plan next);
    // Options the passes read live, their on/off options and the addons' toggle keys, from the settings
    // ADDON SETTINGS changes in game.
    void refresh_live();
    void poll_toggles();
    std::vector<int> live_passes(const frontend::view::State &v, unsigned *idle = nullptr);
    bool addon_off(const std::string &id);
    std::string describe(const RenderPass &p);
    int texture_named(const std::string &name);

private:
    ID3D12PipelineState *effect_pso(const std::string &path);
    void drop_instance(int inst);

    Pipeline &pipeline;
};

}
