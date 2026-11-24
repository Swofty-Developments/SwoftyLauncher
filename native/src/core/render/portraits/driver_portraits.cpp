#include "core/render/portraits/driver_portraits.hpp"
#include "core/render/portraits/portrait_data.hpp"
#include "core/render/badges/badge_images.hpp"
#include "core/frontend/player_name/copies.hpp"
#include "core/frontend/ui_registry_walk.hpp"
#include "core/live/entrant_ids.hpp"
#include "core/render/d3d12_hooks.hpp"
#include "core/render/readback/pixels.hpp"
#include "core/render/readback/texture_readback.hpp"
#include "core/runtime/build.hpp"
#include "core/runtime/runtime.hpp"

#include <swofty/game.hpp>
#include <swofty/mixin.hpp>
#include <swofty_api.h>

#include <algorithm>
#include <atomic>
#include <cstdio>
#include <deque>
#include <iterator>
#include <map>
#include <memory>
#include <mutex>
#include <optional>
#include <string>
#include <vector>

// Capture: once an addon asks (start), the core follows SetName for textures named from the render target pool and
// copies each one when the game moves it from being written (render target, copy or UAV state) to being read by
// shaders, while a session loads or runs or when its size is one the game's thumbnail sets render. Pixels stay until
// the session unloads, when the game frees the targets and their pool names come free for the next session's.
namespace driver_portraits {
namespace {
namespace data = render::portraits::data;
using Microsoft::WRL::ComPtr;

std::mutex mu;
std::once_flag registered;
std::atomic<bool> armed{false}, window{false};
uint64_t epoch = 1, submission_serial = 0, image_revision = 0, name_serial = 0;

struct Seen {
    std::string name;
    uint64_t version = 0;
    unsigned captures = 0;
    const char *reason = "named";
};
std::map<ID3D12Resource *, Seen> seen;
std::map<std::string, uint64_t> versions;

struct Image {
    ComPtr<ID3D12Resource> resource;
    unsigned width{}, height{}, format{};
    uint64_t revision{}, serial{}, used{};
    std::vector<uint8_t> rgba;
};
std::map<std::string, Image> images;

struct Readback : render::readback::Job {
    std::string name;
    ComPtr<ID3D12Resource> identity;
    bool bgra{};
    unsigned format{}, list_type{}, before{}, after{};
    uint64_t epoch{}, version{};
};
std::deque<std::shared_ptr<Readback>> pending;
unsigned logged_names = 0, logged_other = 0, logged_rejects = 0;
std::map<std::string, unsigned> misses;

constexpr unsigned LOG_NAMES = 96, LOG_OTHER = 8, LOG_REJECTS = 32;

void trace(const char *fmt, auto... a) { swofty::log(("[portraits] " + std::string(fmt)).c_str(), a...); }

bool current(ID3D12Resource *resource, const std::string &name)
{
    auto debug = render::readback::debug_name(resource);
    auto canonical = debug ? data::resource(*debug) : std::nullopt;
    return canonical && *canonical == name;
}

void named(ID3D12Object *object, LPCWSTR text)
{
    auto ascii = render::readback::set_name_text(text);
    if (!object || !ascii)
        return;
    auto *raw = reinterpret_cast<ID3D12Resource *>(object);
    auto name = data::resource(*ascii);
    std::lock_guard guard(mu);
    if (!name) {
        auto at = seen.find(raw);
        if (at != seen.end()) {
            images.erase(at->second.name);
            seen.erase(at);
        }
        if (logged_other >= LOG_OTHER)
            return;
        ComPtr<ID3D12Resource> resource;
        if (FAILED(object->QueryInterface(IID_PPV_ARGS(&resource))))
            return;
        const auto desc = resource->GetDesc();
        if (desc.Dimension != D3D12_RESOURCE_DIMENSION_TEXTURE2D || !data::game_shape(unsigned(desc.Width), desc.Height))
            return;
        ++logged_other;
        trace("texture of a thumbnail size named '%s' (not a pool name): %llux%u format=%u flags=0x%x", ascii->c_str(),
              (unsigned long long)desc.Width, desc.Height, unsigned(desc.Format), unsigned(desc.Flags));
        return;
    }
    ComPtr<ID3D12Resource> resource;
    if (FAILED(object->QueryInterface(IID_PPV_ARGS(&resource))))
        return;
    const auto desc = resource->GetDesc();
    for (auto it = images.begin(); it != images.end();)
        if (it->first == *name || it->second.resource.Get() == resource.Get())
            it = images.erase(it);
        else
            ++it;
    const uint64_t version = versions[*name] = ++name_serial;
    if (seen.size() >= 4 * data::CACHE && !seen.contains(raw))
        seen.erase(seen.begin());
    seen[raw] = Seen{*name, version, 0, "named"};
    if (logged_names++ < LOG_NAMES)
        trace("named '%s' (%s): %llux%u layers=%u mips=%u format=%u flags=0x%x", ascii->c_str(), name->c_str(),
              (unsigned long long)desc.Width, desc.Height, unsigned(desc.DepthOrArraySize), unsigned(desc.MipLevels),
              unsigned(desc.Format), unsigned(desc.Flags));
}

void reject(Seen &s, const char *why, const D3D12_RESOURCE_DESC &desc, const D3D12_RESOURCE_BARRIER &step,
            ID3D12GraphicsCommandList *list)
{
    if (s.reason == why)
        return;
    s.reason = why;
    if (logged_rejects++ < LOG_REJECTS)
        trace("not captured '%s': %s (%llux%u format=%u list=%u states=0x%x->0x%x subresource=0x%x)", s.name.c_str(), why,
              (unsigned long long)desc.Width, desc.Height, unsigned(desc.Format), unsigned(list->GetType()),
              unsigned(step.Transition.StateBefore), unsigned(step.Transition.StateAfter), step.Transition.Subresource);
}

struct Alpha {
    unsigned clear = 0, opaque = 0, partial = 0;
    unsigned long long r = 0, g = 0, b = 0;
};

Alpha alpha_of(const std::vector<uint8_t> &rgba)
{
    Alpha a;
    for (size_t i = 0; i + 3 < rgba.size(); i += 4) {
        const uint8_t v = rgba[i + 3];
        (v == 0 ? a.clear : v == 255 ? a.opaque : a.partial)++;
        a.r += rgba[i];
        a.g += rgba[i + 1];
        a.b += rgba[i + 2];
    }
    return a;
}

void poll()
{
    for (auto it = pending.begin(); it != pending.end();) {
        auto &job = **it;
        const auto state = render::readback::status(job);
        if (state == render::readback::Status::waiting) {
            ++it;
            continue;
        }
        if (state == render::readback::Status::lost) {
            it = pending.erase(it);
            continue;
        }
        unsigned w = 0, h = 0;
        std::vector<uint8_t> rgba;
        render::readback::map(job, [&](const uint8_t *rows, unsigned pitch) {
            rgba = render::readback::fit(rows, job.width, job.height, pitch, job.bgra, data::MAX_SIDE, data::MAX_SIDE, w, h);
        });
        const auto version = versions.find(job.name);
        if (!rgba.empty() && job.epoch == epoch && version != versions.end() && version->second == job.version &&
            current(job.identity.Get(), job.name)) {
            if (!images.contains(job.name) && images.size() >= data::CACHE)
                images.erase(std::min_element(images.begin(), images.end(),
                                              [](auto &a, auto &b) { return a.second.used < b.second.used; }));
            auto &image = images[job.name];
            if (job.serial >= image.serial) {
                const Alpha a = alpha_of(rgba);
                const unsigned long long n = std::max<size_t>(1, rgba.size() / 4);
                image.resource = job.identity;
                image.width = w;
                image.height = h;
                image.format = job.format;
                image.serial = job.serial;
                image.rgba = std::move(rgba);
                image.revision = ++image_revision;
                image.used = GetTickCount64();
                trace("captured '%s': %ux%u format=%u list=%u states=0x%x->0x%x serial=%llu cached=%zu alpha 0:%u 255:%u "
                      "other:%u mean rgb %llu %llu %llu",
                      job.name.c_str(), w, h, job.format, job.list_type, job.before, job.after, (unsigned long long)job.serial,
                      images.size(), a.clear, a.opaque, a.partial, a.r / n, a.g / n, a.b / n);
            }
        }
        it = pending.erase(it);
    }
}

void barrier(ID3D12GraphicsCommandList *list, UINT n, const D3D12_RESOURCE_BARRIER *steps)
{
    if (!armed.load() || !list || !steps)
        return;
    std::lock_guard guard(mu);
    poll();
    for (UINT i = 0; i < n; ++i) {
        const auto &step = steps[i];
        if (step.Type != D3D12_RESOURCE_BARRIER_TYPE_TRANSITION || !step.Transition.pResource)
            continue;
        constexpr auto WRITTEN =
            D3D12_RESOURCE_STATE_RENDER_TARGET | D3D12_RESOURCE_STATE_COPY_DEST | D3D12_RESOURCE_STATE_UNORDERED_ACCESS;
        constexpr auto READ = D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE | D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE;
        if (!(step.Transition.StateBefore & WRITTEN) || !(step.Transition.StateAfter & READ))
            continue;
        const auto desc = step.Transition.pResource->GetDesc();
        auto at = seen.find(step.Transition.pResource);
        if (at == seen.end()) {
            if (desc.Dimension != D3D12_RESOURCE_DIMENSION_TEXTURE2D || !data::game_shape(unsigned(desc.Width), desc.Height))
                continue;
            auto debug = render::readback::debug_name(step.Transition.pResource);
            auto name = debug ? data::resource(*debug) : std::nullopt;
            if (!name || seen.size() >= 4 * data::CACHE)
                continue;
            auto version = versions.find(*name);
            at = seen.emplace(step.Transition.pResource,
                              Seen{*name, version != versions.end() ? version->second : versions[*name] = ++name_serial})
                     .first;
            if (logged_names++ < LOG_NAMES)
                trace("found '%s' by its debug name at a barrier (no SetName seen): %llux%u format=%u", name->c_str(),
                      (unsigned long long)desc.Width, desc.Height, unsigned(desc.Format));
        }
        auto &s = at->second;
        bool bgra = false;
        const auto type = list->GetType();
        if (!window.load() && !data::game_shape(unsigned(desc.Width), desc.Height)) {
            reject(s, "outside_session_and_not_a_thumbnail_size", desc, step, list);
            continue;
        }
        if (type != D3D12_COMMAND_LIST_TYPE_DIRECT && type != D3D12_COMMAND_LIST_TYPE_COMPUTE) {
            reject(s, "list_type", desc, step, list);
            continue;
        }
        if (step.Flags != D3D12_RESOURCE_BARRIER_FLAG_NONE) {
            reject(s, "split_barrier", desc, step, list);
            continue;
        }
        if (step.Transition.Subresource != D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES && step.Transition.Subresource != 0) {
            reject(s, "not_mip_zero", desc, step, list);
            continue;
        }
        if (desc.Dimension != D3D12_RESOURCE_DIMENSION_TEXTURE2D || !desc.Width || !desc.Height || desc.Width > data::MAX_SIDE ||
            desc.Height > data::MAX_SIDE || desc.DepthOrArraySize != 1 || desc.SampleDesc.Count != 1 || desc.MipLevels < 1) {
            reject(s, "unsupported_shape", desc, step, list);
            continue;
        }
        if (!data::format(unsigned(desc.Format), bgra)) {
            reject(s, "unsupported_format", desc, step, list);
            continue;
        }
        if (s.captures >= data::CAPTURES_PER_NAME) {
            reject(s, "capture_limit", desc, step, list);
            continue;
        }
        if (pending.size() >= data::PENDING) {
            reject(s, "pending_limit", desc, step, list);
            continue;
        }
        auto job = std::make_shared<Readback>();
        job->name = s.name;
        job->identity = step.Transition.pResource;
        job->bgra = bgra;
        job->format = unsigned(desc.Format);
        job->list_type = unsigned(type);
        job->before = unsigned(step.Transition.StateBefore);
        job->after = unsigned(step.Transition.StateAfter);
        job->epoch = epoch;
        job->version = s.version;
        if (const char *why = render::readback::copy_at_transition(list, step, desc, *job)) {
            reject(s, why, desc, step, list);
            continue;
        }
        ++s.captures;
        s.reason = "queued";
        pending.push_back(std::move(job));
    }
}

void execute(ID3D12CommandQueue *queue, UINT n, ID3D12CommandList *const *lists, render::d3d12::ExecPlan &plan)
{
    if (!armed.load() || !render::readback::copy_capable(queue))
        return;
    std::lock_guard guard(mu);
    poll();
    render::readback::schedule(pending, mu, submission_serial, n, lists, plan);
}

void reset(ID3D12GraphicsCommandList *list)
{
    std::lock_guard guard(mu);
    std::erase_if(pending, [&](auto &p) { return !p->scheduled && p->list.Get() == list; });
}

enum class Fit { full, small };

int copy(const char *reference, uint8_t *rgba, uint32_t cap, uint32_t *width, uint32_t *height, uint64_t *revision, Fit fit)
{
    if (!reference || !width || !height || !revision || strnlen(reference, 160) >= 160)
        return SWOFTY_ASSET_INVALID;
    auto name = data::resource(reference);
    if (!name || *name != reference)
        return SWOFTY_ASSET_UNAVAILABLE;
    std::lock_guard guard(mu);
    poll();
    auto found = images.find(*name);
    if (found != images.end() && !current(found->second.resource.Get(), *name)) {
        images.erase(found);
        found = images.end();
    }
    if (found == images.end()) {
        if (misses.size() < 64) {
            const unsigned count = ++misses[*name];
            if (count == 1 || count == 600) {
                const Seen *s = nullptr;
                for (auto &[_, v] : seen)
                    if (v.name == *name)
                        s = &v;
                trace("image unavailable '%s': miss=%u armed=%d window=%d named=%d captures=%u reason=%s cached=%zu pending=%zu",
                      name->c_str(), count, int(armed.load()), int(window.load()), int(s != nullptr), s ? s->captures : 0,
                      s ? s->reason : "never_named", images.size(), pending.size());
            }
        }
        return SWOFTY_ASSET_UNAVAILABLE;
    }
    auto &image = found->second;
    image.used = GetTickCount64();
    *revision = image.revision;
    if (fit == Fit::small) {
        unsigned w = 0, h = 0;
        auto small =
            render::readback::fit(image.rgba.data(), image.width, image.height, image.width * 4, false, data::MAX_SIDE, 64, w, h);
        *width = w;
        *height = h;
        if (!rgba || cap < small.size())
            return SWOFTY_ASSET_INVALID;
        std::memcpy(rgba, small.data(), small.size());
        return SWOFTY_ASSET_OK;
    }
    *width = image.width;
    *height = image.height;
    if (!rgba || cap < image.rgba.size())
        return SWOFTY_ASSET_INVALID;
    std::memcpy(rgba, image.rgba.data(), image.rgba.size());
    return SWOFTY_ASSET_OK;
}

inline constexpr runtime::build_proof::Gate VM_GATE{"the driver portraits' view model lookup",
                                                    &frontend::player_name::TT_NAME_LAYOUT};

struct Mapped {
    uint64_t at = 0, serial = 0;
    int status = SWOFTY_ASSET_UNAVAILABLE;
    std::string reference;
    uint32_t driver = 0;
};
std::mutex map_mu;
Mapped mapped[SWOFTY_LIVE_MAX_CARS];
std::map<uint64_t, std::string> map_logged;

int lookup(uint32_t vehicle, std::string &reference, uint32_t &driver_key)
{
    namespace ids = swofty_core::live::entrant_ids;
    namespace ur = frontend::ui_registry;
    ids::Ids car;
    std::string why;
    const auto read = ids::read_car(vehicle, car, &why);
    if (read != ids::Read::ok || !car.driver_read || car.driver == ids::NONE)
        return SWOFTY_ASSET_UNAVAILABLE;
    driver_key = uint32_t(car.driver_domain) | uint32_t(car.driver) << 16;
    if (!runtime::proven(VM_GATE) || !swofty::game::g_Vm_Registry.address())
        return SWOFTY_ASSET_UNAVAILABLE;
    const uint64_t base = swofty::api->module_base(), reg = uint64_t(*swofty::game::g_Vm_Registry);
    const ur::Layout lay = frontend::player_name::registry_layout(runtime::running_build(), base);
    if (!lay.object_vt || !reg)
        return SWOFTY_ASSET_UNAVAILABLE;
    auto rd = [](uint64_t at, void *out, size_t n) { return frontend::player_name::registry_read(at, out, n); };
    struct Leaf {
        std::optional<data::Path> path;
        std::optional<std::string> name;
        bool srgb = false;
    } leaves[2];
    for (size_t set = 0; set < 2; set++) {
        auto &leaf = leaves[set];
        leaf.path = data::path(car.driver_domain, car.driver, set);
        if (!leaf.path)
            return SWOFTY_ASSET_UNAVAILABLE;
        const ur::Step steps[] = {{ur::djb2(leaf.path->list)},
                                  {0, true, leaf.path->index},
                                  {ur::djb2("textures")},
                                  {ur::djb2(leaf.path->set)},
                                  {ur::djb2("Thumbnail")}};
        std::string value;
        if (ur::string_at(rd, reg, lay, steps, std::size(steps), value) == ur::Lookup::ok)
            leaf.name = data::resource(value, &leaf.srgb);
    }
    const Leaf &use = leaves[0].name ? leaves[0] : leaves[1];
    if (!use.name)
        return SWOFTY_ASSET_UNAVAILABLE;
    reference = *use.name;
    std::lock_guard guard(map_mu);
    auto &said = map_logged[uint64_t(vehicle) << 32 | driver_key];
    if (said != *use.name && map_logged.size() < 256) {
        said = *use.name;
        bool cached;
        {
            std::lock_guard g(mu);
            cached = images.contains(*use.name);
        }
        trace("car %u: driver %s[%u] -> %s.Thumbnail '%s' (%s view), pixels %s", vehicle, use.path->list, use.path->index,
              use.path->set, use.name->c_str(), use.srgb ? "srgb" : "unorm", cached ? "captured" : "not captured");
        if (leaves[0].name && leaves[1].name)
            trace("car %u: driver %s[%u] holds both GridRundownLeft '%s' and GridRundownRight '%s'; using GridRundownLeft",
                  vehicle, use.path->list, use.path->index, leaves[0].name->c_str(), leaves[1].name->c_str());
    }
    return SWOFTY_ASSET_OK;
}

}

int start(uint32_t flags)
{
    if (flags)
        return SWOFTY_ASSET_INVALID;
    std::call_once(registered, [] {
        render::d3d12::on_barrier_observe(barrier);
        render::d3d12::on_exec(execute);
        render::d3d12::on_reset(reset);
        render::d3d12::on_setname(named);
    });
    if (!armed.load()) {
        armed.store(render::d3d12::arm_device() && render::d3d12::arm_list_state());
        trace("capture %s", armed.load() ? "armed: following render target pool names" : "unavailable: device hooks off");
    }
    return armed.load() ? SWOFTY_ASSET_OK : SWOFTY_ASSET_UNAVAILABLE;
}

bool owns(const char *reference)
{
    if (!reference)
        return false;
    auto name = data::resource(reference);
    return name && *name == reference;
}

int image(const char *reference, uint8_t *rgba, uint32_t cap, uint32_t *width, uint32_t *height, uint64_t *revision)
{
    return copy(reference, rgba, cap, width, height, revision, Fit::small);
}

int image_full(const char *reference, uint8_t *rgba, uint32_t cap, uint32_t *width, uint32_t *height, uint64_t *revision)
{
    return copy(reference, rgba, cap, width, height, revision, Fit::full);
}

int driver_portrait(uint32_t vehicle, char *reference, uint32_t cap)
{
    if (!reference || cap < 2 || vehicle >= SWOFTY_LIVE_MAX_CARS)
        return SWOFTY_ASSET_INVALID;
    reference[0] = 0;
    const uint64_t now = GetTickCount64(), serial = runtime::session_serial();
    Mapped m;
    {
        std::lock_guard guard(map_mu);
        m = mapped[vehicle];
    }
    if (!m.at || m.serial != serial || now - m.at >= 1000) {
        m = Mapped{};
        m.at = now;
        m.serial = serial;
        m.status = lookup(vehicle, m.reference, m.driver);
        std::lock_guard guard(map_mu);
        mapped[vehicle] = m;
    }
    if (m.status != SWOFTY_ASSET_OK)
        return m.status;
    if (m.reference.size() + 1 > cap)
        return SWOFTY_ASSET_INVALID;
    std::memcpy(reference, m.reference.c_str(), m.reference.size() + 1);
    return SWOFTY_ASSET_OK;
}

void session_loading()
{
    {
        std::lock_guard guard(mu);
        logged_names = logged_other = logged_rejects = 0;
    }
    window.store(true);
    if (armed.load())
        trace("session loading: capture window open");
}

void session_unloaded()
{
    window.store(false);
    {
        std::lock_guard guard(mu);
        if (armed.load())
            trace("session unload: dropped %zu portrait(s), %zu pending", images.size(), pending.size());
        images.clear();
        misses.clear();
        ++epoch;
        poll();
    }
    std::lock_guard guard(map_mu);
    for (auto &m : mapped)
        m = Mapped{};
    map_logged.clear();
}

const SwoftyLiveImages *live_images()
{
    static const SwoftyLiveImages *badges = badge_images::api();
    static const SwoftyLiveImages api{
        2,
        badges->start,
        badges->sources,
        [](const char *reference, uint8_t *rgba, uint32_t cap, uint32_t *width, uint32_t *height, uint64_t *revision) {
            return owns(reference) ? image(reference, rgba, cap, width, height, revision)
                                   : badges->image(reference, rgba, cap, width, height, revision);
        },
        badges->clear,
        start,
        image_full};
    return &api;
}

}
