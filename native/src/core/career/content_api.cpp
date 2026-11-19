#include "core/career/content_state.hpp"
#include "core/runtime/runtime.hpp"
#include "core/runtime/settings.hpp"

#include <swofty/game.hpp>

#include <cstddef>
#include <cstring>
#include <mutex>
#include <string>

namespace career::content {

namespace {

std::string text(const char *s) { return s ? s : ""; }

int copy_out(const std::string &s, char *out, size_t cap)
{
    if (!out || !cap)
        return SWOFTY_ASSET_INVALID;
    size_t n = s.size() < cap - 1 ? s.size() : cap - 1;
    std::memcpy(out, s.data(), n);
    out[n] = 0;
    return n == s.size() ? SWOFTY_ASSET_OK : SWOFTY_ASSET_INVALID;
}

std::string caller(const void *from)
{
    std::string dll, id = settings_addon(from, dll);
    return id.empty() ? (dll.empty() ? std::string("unknown module") : dll) : id;
}

int api_available() { return runtime::enabled("career", nullptr) && !hooks.off(); }

int api_add(const SwoftyCareerContentDesc *d)
{
    std::string who = caller(__builtin_return_address(0));
    constexpr size_t careers_end = offsetof(SwoftyCareerContentDesc, careers) + sizeof(uint32_t);
    if (!d || d->size < offsetof(SwoftyCareerContentDesc, careers)) {
        swofty::log("[career] content from %s refused: no descriptor, or its size %u is below %zu", who.c_str(), d ? d->size : 0,
                    offsetof(SwoftyCareerContentDesc, careers));
        return SWOFTY_ASSET_INVALID;
    }
    if (!runtime::enabled("career", nullptr)) {
        swofty::log("[career] content '%s' from %s refused: the career feature is off", text(d->id).c_str(), who.c_str());
        return SWOFTY_ASSET_UNAVAILABLE;
    }
    content::Desc desc{text(d->id),
                       text(d->title),
                       text(d->description),
                       text(d->data_group),
                       text(d->suffix),
                       text(d->mode),
                       text(d->entitlement),
                       d->model,
                       d->car_group,
                       d->badge,
                       d->size >= careers_end ? d->careers : content::CAREER_DRIVER};
    return add_slot(desc, who, runtime::older_addon(__builtin_return_address(0)), nullptr);
}

int api_info(const char *id, SwoftyCareerContentInfo *out)
{
    if (!id || !out)
        return SWOFTY_ASSET_INVALID;
    for (size_t i = 0; i < used(); i++) {
        Slot &s = slots[i];
        if (s.desc.id != id)
            continue;
        resolve(s, true);
        std::lock_guard<std::mutex> g(hooks.lock);
        *out = {};
        out->index = uint32_t(i);
        out->state = s.state;
        out->mode_stat_id = s.mode;
        out->data_group_index = s.group;
        out->tiles = s.tiles;
        out->presses = s.presses;
        copy_out(s.desc.id, out->id, sizeof out->id);
        copy_out(s.desc.data_group, out->data_group, sizeof out->data_group);
        copy_out(s.why, out->why, sizeof out->why);
        return SWOFTY_ASSET_OK;
    }
    return SWOFTY_ASSET_INVALID;
}

int api_data_group(const void *career, char *out, size_t cap)
{
    if (!out || !cap)
        return SWOFTY_ASSET_INVALID;
    out[0] = 0;
    if (!runtime::enabled("career", nullptr))
        return SWOFTY_ASSET_UNAVAILABLE;
    if (!b.setup_desc)
        b.setup_desc = swofty::api->resolve("k_Career_CareerSetupComponentDesc");
    if (!b.setup_desc)
        return SWOFTY_ASSET_UNAVAILABLE;
    if (!career)
        career = runtime::career_table()->entity();
    if (!career)
        return SWOFTY_ASSET_LATE;
    content::GroupSeen seen = content::data_group(career, ACCESS);
    if (!seen.name)
        return SWOFTY_ASSET_INVALID;
    return copy_out(seen.name, out, cap);
}

int api_pressed(char *out, size_t cap)
{
    if (!out || !cap)
        return SWOFTY_ASSET_INVALID;
    out[0] = 0;
    if (!hooks.live())
        return SWOFTY_ASSET_UNAVAILABLE;
    std::lock_guard<std::mutex> l(pressed_lock);
    if (!pressed_any)
        return SWOFTY_ASSET_LATE;
    return copy_out(pressed_group, out, cap);
}

const SwoftyCareerContent table = {api_available, api_add, api_info, api_data_group, api_pressed};

}

}

using namespace career::content;

extern "C" __declspec(dllexport) const SwoftyCareerContent *SwoftyCoreCareerContent(void) { return &table; }

namespace runtime {

const SwoftyCareerContent *career_content_table() { return &table; }

}
