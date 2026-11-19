#include "core/hud/overlay/overlay.hpp"
#include "core/runtime/runtime.hpp"
#include "core/runtime/settings.hpp"

#include <swofty/mixin.hpp>

#include <algorithm>
#include <cstddef>
#include <cstring>
#include <mutex>

namespace hud::overlay {

namespace {

struct Caller {
    std::string id, dll;
};

Caller caller(const void *from)
{
    Caller c;
    c.id = settings_addon(from, c.dll);
    return c;
}

std::string owner_of(const Caller &c) { return c.id.empty() ? c.dll : c.id; }

std::string addon_image(const Caller &c, const char *file, std::string &why)
{
    if (c.id.empty())
        return why = "no addon in settings.json lists " + c.dll + ", so it has no folder", std::string();
    std::string f = file ? file : "";
    std::replace(f.begin(), f.end(), '\\', '/');
    if (f.empty() || f[0] == '/' || f.find(':') != std::string::npos || ("/" + f + "/").find("/../") != std::string::npos)
        return why = "'" + f + "' is not a relative path inside swofty/addons/" + c.id + "/", std::string();
    std::string rel = "addons/" + c.id + "/" + f;
    uint64_t size = 0;
    if (!runtime::file_size(runtime::under_swofty(rel), &size))
        return why = "swofty/" + rel + " is missing", std::string();
    return rel;
}

int ov_available()
{
    bool arming = overlay.arming.load();
    if (!overlay.implemented || !(arming || overlay.armed.load()))
        return 0;
    return hud::drawlist::listed(settings().off, owner_of(caller(__builtin_return_address(0)))) ? 0 : 1;
}

int ov_on_paint(SwoftyPaintFn fn, void *user, uint32_t *token)
{
    Caller c = caller(__builtin_return_address(0));
    if (hud::drawlist::listed(settings().off, owner_of(c))) {
        swofty::log("[overlay] api: %s on_paint -> unavailable (turned off for this addon in the launcher)", owner_of(c).c_str());
        return SWOFTY_ASSET_UNAVAILABLE;
    }
    if (!overlay.implemented || !settings().gate)
        return SWOFTY_ASSET_UNAVAILABLE;
    if (!fn)
        return SWOFTY_ASSET_INVALID;
    uint32_t t;
    {
        std::lock_guard<std::mutex> g(overlay.reg_lock);
        t = overlay.reg.add(fn, user, owner_of(c));
    }
    if (token)
        *token = t;
    swofty::log("[overlay] api: %s on_paint -> ok (token %u)", c.id.empty() ? c.dll.c_str() : c.id.c_str(), t);
    return SWOFTY_ASSET_OK;
}

int ov_remove(uint32_t token)
{
    if (!overlay.implemented || !settings().gate)
        return SWOFTY_ASSET_UNAVAILABLE;
    std::lock_guard<std::mutex> g(overlay.reg_lock);
    return overlay.reg.remove(token) ? SWOFTY_ASSET_OK : SWOFTY_ASSET_INVALID;
}

void ov_size(const SwoftyPaint *paint, float *width, float *height)
{
    auto *f = reinterpret_cast<const hud::drawlist::Frame *>(paint);
    if (width)
        *width = f ? f->width : 0;
    if (height)
        *height = f ? f->height : 0;
}

void ov_rect(SwoftyPaint *paint, float x, float y, float w, float h, uint32_t rgba)
{
    if (paint)
        hud::drawlist::rect(*reinterpret_cast<hud::drawlist::Frame *>(paint), x, y, w, h, rgba);
}

void ov_rect_outline(SwoftyPaint *paint, float x, float y, float w, float h, float thickness, uint32_t rgba)
{
    if (paint)
        hud::drawlist::rect_outline(*reinterpret_cast<hud::drawlist::Frame *>(paint), x, y, w, h, thickness, rgba);
}

void ov_line(SwoftyPaint *paint, float x0, float y0, float x1, float y1, float thickness, uint32_t rgba)
{
    if (paint)
        hud::drawlist::line(*reinterpret_cast<hud::drawlist::Frame *>(paint), x0, y0, x1, y1, thickness, rgba);
}

void ov_text(SwoftyPaint *paint, float x, float y, float size, uint32_t rgba, const char *utf8)
{
    if (paint && utf8)
        hud::drawlist::text(*reinterpret_cast<hud::drawlist::Frame *>(paint), x, y, size, rgba, utf8);
}

void ov_measure(const SwoftyPaint *, float size, const char *utf8, float *width, float *height)
{
    hud::drawlist::measure(size, utf8 ? utf8 : "", width, height);
}

int ov_load_image(const char *id, const char *file)
{
    Caller c = caller(__builtin_return_address(0));
    if (!overlay.implemented || !settings().gate)
        return SWOFTY_ASSET_UNAVAILABLE;
    const char *who = c.id.empty() ? c.dll.c_str() : c.id.c_str();
    if (!id || !*id)
        return SWOFTY_ASSET_INVALID;
    std::string why, rel = addon_image(c, file, why);
    hud::img::Pixels px;
    if (!rel.empty() && !hud::img::decode(runtime::read_file(runtime::under_swofty(rel), size_t(64) << 20), px, why))
        why = "swofty/" + rel + ": " + why;
    if (rel.empty() || px.rgba.empty()) {
        swofty::log("[overlay] api: %s load_image %s -> invalid: %s", who, id, why.c_str());
        return SWOFTY_ASSET_INVALID;
    }
    uint32_t w = px.width, h = px.height;
    int slot;
    {
        std::lock_guard<std::mutex> g(overlay.reg_lock);
        slot = overlay.reg.set_image(owner_of(c) + "\n" + id, w, h, std::move(px.rgba));
    }
    if (slot + 1 >= int(TEX_MAX)) {
        swofty::log("[overlay] api: %s load_image %s -> invalid: the overlay holds at most %u images", who, id, TEX_MAX - 1);
        return SWOFTY_ASSET_INVALID;
    }
    swofty::log("[overlay] api: %s load_image %s (swofty/%s, %ux%u) -> ok (slot %d)", who, id, rel.c_str(), w, h, slot);
    return SWOFTY_ASSET_OK;
}

hud::drawlist::Frame *frame_of(SwoftyPaint *paint) { return reinterpret_cast<hud::drawlist::Frame *>(paint); }

struct Found {
    int slot = -1;
    float w = 0, h = 0;
};

Found find_image(const std::string &owner, const char *id)
{
    Found f;
    if (!id || !*id)
        return f;
    static thread_local std::string key;
    key.assign(owner).append(1, '\n').append(id);
    std::lock_guard<std::mutex> g(overlay.reg_lock);
    f.slot = overlay.reg.find_image(key);
    if (const hud::drawlist::Image *im = overlay.reg.image(f.slot)) {
        f.w = float(im->width);
        f.h = float(im->height);
    }
    return f;
}

int ov_image(SwoftyPaint *paint, const char *id, float x, float y, float w, float h, uint32_t tint)
{
    if (!overlay.implemented || !settings().gate)
        return SWOFTY_ASSET_UNAVAILABLE;
    Found f = find_image(paint ? frame_of(paint)->owner : owner_of(caller(__builtin_return_address(0))), id);
    if (f.slot < 0)
        return SWOFTY_ASSET_INVALID;
    if (paint)
        hud::drawlist::image(*frame_of(paint), f.slot, x, y, w, h, tint);
    return SWOFTY_ASSET_OK;
}

void ov_rect_rounded(SwoftyPaint *paint, float x, float y, float w, float h, float radius, uint32_t rgba)
{
    if (paint)
        hud::drawlist::rect_rounded(*frame_of(paint), x, y, w, h, radius, rgba);
}

void ov_rect_rounded_outline(SwoftyPaint *paint, float x, float y, float w, float h, float radius, float thickness, uint32_t rgba)
{
    if (paint)
        hud::drawlist::rect_rounded_outline(*frame_of(paint), x, y, w, h, radius, thickness, rgba);
}

void ov_rect_gradient(SwoftyPaint *paint, float x, float y, float w, float h, float radius, uint32_t from, uint32_t to,
                      uint32_t flags)
{
    if (paint)
        hud::drawlist::rect_gradient(*frame_of(paint), x, y, w, h, radius, from, to, flags);
}

void ov_circle(SwoftyPaint *paint, float cx, float cy, float radius, uint32_t rgba)
{
    if (paint)
        hud::drawlist::circle(*frame_of(paint), cx, cy, radius, rgba);
}

void ov_circle_outline(SwoftyPaint *paint, float cx, float cy, float radius, float thickness, uint32_t rgba)
{
    if (paint)
        hud::drawlist::circle_outline(*frame_of(paint), cx, cy, radius, thickness, rgba);
}

void ov_arc(SwoftyPaint *paint, float cx, float cy, float radius, float from, float to, float thickness, uint32_t rgba)
{
    if (paint)
        hud::drawlist::arc(*frame_of(paint), cx, cy, radius, from, to, thickness, rgba);
}

void ov_polyline(SwoftyPaint *paint, const float *xy, uint32_t count, float thickness, uint32_t rgba, uint32_t flags)
{
    if (paint)
        hud::drawlist::polyline(*frame_of(paint), xy, std::min<uint32_t>(count, hud::drawlist::MAX_VERTS / 18), thickness, rgba,
                                flags);
}

void ov_polygon(SwoftyPaint *paint, const float *xy, uint32_t count, uint32_t rgba)
{
    if (paint)
        hud::drawlist::polygon(*frame_of(paint), xy, std::min<uint32_t>(count, hud::drawlist::MAX_VERTS / 9), rgba);
}

hud::drawlist::TextStyle text_style(const SwoftyTextStyle *s)
{
    hud::drawlist::TextStyle t;
    if (!s || s->size < offsetof(SwoftyTextStyle, px) + sizeof(float)) {
        t.px = 0;
        return t;
    }
    auto has = [&](size_t end) { return s->size >= end; };
    t.align = s->align;
    t.px = s->px;
    if (has(offsetof(SwoftyTextStyle, rgba) + 4))
        t.rgba = s->rgba;
    if (has(offsetof(SwoftyTextStyle, tracking) + 4))
        t.tracking = s->tracking;
    if (has(offsetof(SwoftyTextStyle, line_height) + 4))
        t.line_height = s->line_height;
    if (has(offsetof(SwoftyTextStyle, outline_px) + 4)) {
        t.outline_rgba = s->outline_rgba;
        t.outline_px = s->outline_px;
    }
    if (has(offsetof(SwoftyTextStyle, shadow_blur) + 4)) {
        t.shadow_rgba = s->shadow_rgba;
        t.shadow_dx = s->shadow_dx;
        t.shadow_dy = s->shadow_dy;
        t.shadow_blur = s->shadow_blur;
    }
    if (has(offsetof(SwoftyTextStyle, max_width) + 4))
        t.max_width = s->max_width;
    return t;
}

void ov_text_styled(SwoftyPaint *paint, float x, float y, const SwoftyTextStyle *style, const char *utf8)
{
    if (paint && utf8)
        hud::drawlist::text(*frame_of(paint), x, y, text_style(style), utf8);
}

void ov_measure_styled(const SwoftyPaint *, const SwoftyTextStyle *style, const char *utf8, float *width, float *height)
{
    hud::drawlist::measure(text_style(style), utf8 ? utf8 : "", width, height);
}

int ov_image_uv(SwoftyPaint *paint, const char *id, float x, float y, float w, float h, float u0, float v0, float u1, float v1,
                uint32_t tint)
{
    if (!overlay.implemented || !settings().gate)
        return SWOFTY_ASSET_UNAVAILABLE;
    Found f = find_image(paint ? frame_of(paint)->owner : owner_of(caller(__builtin_return_address(0))), id);
    if (f.slot < 0)
        return SWOFTY_ASSET_INVALID;
    if (paint)
        hud::drawlist::image_uv(*frame_of(paint), f.slot, x, y, w, h, u0, v0, u1, v1, tint);
    return SWOFTY_ASSET_OK;
}

int ov_image_nine(SwoftyPaint *paint, const char *id, float x, float y, float w, float h, float left, float top, float right,
                  float bottom, float scale, uint32_t tint)
{
    if (!overlay.implemented || !settings().gate)
        return SWOFTY_ASSET_UNAVAILABLE;
    Found f = find_image(paint ? frame_of(paint)->owner : owner_of(caller(__builtin_return_address(0))), id);
    if (f.slot < 0)
        return SWOFTY_ASSET_INVALID;
    if (paint)
        hud::drawlist::image_nine(*frame_of(paint), f.slot, f.w, f.h, x, y, w, h, left, top, right, bottom, scale, tint);
    return SWOFTY_ASSET_OK;
}

int ov_image_size(const char *id, float *width, float *height)
{
    if (!overlay.implemented || !settings().gate)
        return SWOFTY_ASSET_UNAVAILABLE;
    Found f = find_image(owner_of(caller(__builtin_return_address(0))), id);
    if (width)
        *width = f.w;
    if (height)
        *height = f.h;
    return f.slot < 0 ? SWOFTY_ASSET_INVALID : SWOFTY_ASSET_OK;
}

void ov_push_clip(SwoftyPaint *paint, float x, float y, float w, float h)
{
    if (paint)
        hud::drawlist::push_clip(*frame_of(paint), x, y, w, h);
}

void ov_pop_clip(SwoftyPaint *paint)
{
    if (paint)
        hud::drawlist::pop_clip(*frame_of(paint));
}

void ov_push_transform(SwoftyPaint *paint, const float *m)
{
    if (paint)
        hud::drawlist::push_transform(*frame_of(paint), m);
}

void ov_pop_transform(SwoftyPaint *paint)
{
    if (paint)
        hud::drawlist::pop_transform(*frame_of(paint));
}

void ov_push_opacity(SwoftyPaint *paint, float opacity)
{
    if (paint)
        hud::drawlist::push_opacity(*frame_of(paint), opacity);
}

void ov_pop_opacity(SwoftyPaint *paint)
{
    if (paint)
        hud::drawlist::pop_opacity(*frame_of(paint));
}

void ov_safe_area(const SwoftyPaint *paint, float *x, float *y, float *w, float *h)
{
    hud::drawlist::Frame empty;
    hud::drawlist::safe_area(paint ? *reinterpret_cast<const hud::drawlist::Frame *>(paint) : empty, x, y, w, h);
}

float ov_ui_scale(const SwoftyPaint *paint)
{
    return paint ? hud::drawlist::ui_scale(*reinterpret_cast<const hud::drawlist::Frame *>(paint)) : 1.0f;
}

int ov_input(const SwoftyPaint *paint, SwoftyInput *out)
{
    if (!overlay.implemented || !settings().gate)
        return SWOFTY_ASSET_UNAVAILABLE;
    if (!paint || !out || out->size < offsetof(SwoftyInput, x))
        return SWOFTY_ASSET_INVALID;
    SwoftyInput full{};
    full.size = sizeof full;
    overlay.input.view(reinterpret_cast<const hud::drawlist::Frame *>(paint)->owner, full);
    uint32_t size = out->size;
    memcpy(out, &full, std::min<size_t>(size, sizeof full));
    out->size = size;
    return SWOFTY_ASSET_OK;
}

int ov_focus(int on)
{
    Caller c = caller(__builtin_return_address(0));
    if (!overlay.implemented || !settings().gate)
        return SWOFTY_ASSET_UNAVAILABLE;
    std::string who = owner_of(c), before;
    bool changed = overlay.input.take_focus(who, on != 0, before);
    if (changed && before != (on ? who : std::string()))
        swofty::log("[overlay] api: %s focus %s -> ok%s%s", who.c_str(), on ? "on" : "off",
                    on && !before.empty() && before != who ? " (taken from " : "",
                    on && !before.empty() && before != who ? (before + ")").c_str() : "");
    if (!overlay.input.window_hooked())
        return SWOFTY_ASSET_LATE;
    return changed ? SWOFTY_ASSET_OK : SWOFTY_ASSET_INVALID;
}

const SwoftyOverlay table = {ov_available,
                             ov_on_paint,
                             ov_remove,
                             ov_size,
                             ov_rect,
                             ov_rect_outline,
                             ov_line,
                             ov_text,
                             ov_measure,
                             ov_load_image,
                             ov_image,
                             ov_rect_rounded,
                             ov_rect_rounded_outline,
                             ov_rect_gradient,
                             ov_circle,
                             ov_circle_outline,
                             ov_arc,
                             ov_polyline,
                             ov_polygon,
                             ov_text_styled,
                             ov_measure_styled,
                             ov_image_uv,
                             ov_image_nine,
                             ov_image_size,
                             ov_push_clip,
                             ov_pop_clip,
                             ov_push_transform,
                             ov_pop_transform,
                             ov_push_opacity,
                             ov_pop_opacity,
                             ov_safe_area,
                             ov_ui_scale,
                             ov_input,
                             ov_focus};

}

}

using hud::overlay::overlay;

extern "C" __declspec(dllexport) uint32_t SwoftyCoreOverlayVersion(void) { return SWOFTY_OVERLAY_VERSION; }

namespace runtime {

const SwoftyOverlay *overlay_table() { return &hud::overlay::table; }

int overlay_image(const std::string &id, const std::string &path, uint32_t *width, uint32_t *height)
{
    std::string key = id + "|" + path;
    {
        std::lock_guard<std::mutex> g(overlay.reg_lock);
        int slot = overlay.reg.find_image(id);
        const hud::drawlist::Image *im = slot >= 0 ? overlay.reg.image(slot) : nullptr;
        if (im && im->generation > 0 && im->source == key) {
            if (width)
                *width = im->width;
            if (height)
                *height = im->height;
            return slot;
        }
    }
    {
        std::lock_guard<std::mutex> f(overlay.image_failed_lock);
        if (std::find(overlay.image_failed.begin(), overlay.image_failed.end(), key) != overlay.image_failed.end())
            return -1;
    }
    std::string why;
    hud::img::Pixels px;
    std::wstring wpath = path.rfind("swofty:", 0) == 0 ? runtime::under_swofty(path.substr(7)) : runtime::wide(path);
    std::string data = runtime::read_file(wpath, size_t(8) << 20);
    if (data.empty() || !hud::img::decode(data, px, why)) {
        swofty::log("[overlay] main menu image %s: %s", path.c_str(), data.empty() ? "unreadable" : why.c_str());
        std::lock_guard<std::mutex> f(overlay.image_failed_lock);
        overlay.image_failed.push_back(key);
        return -1;
    }
    uint32_t w = px.width, h = px.height;
    int slot;
    {
        std::lock_guard<std::mutex> g(overlay.reg_lock);
        slot = overlay.reg.set_image(id, w, h, std::move(px.rgba));
        if (slot >= 0)
            overlay.reg.set_source(slot, key);
    }
    if (slot + 1 >= int(hud::overlay::TEX_MAX))
        return -1;
    swofty::log("[overlay] main menu image %s (%ux%u) loaded (slot %d)", path.c_str(), w, h, slot);
    if (width)
        *width = w;
    if (height)
        *height = h;
    return slot;
}

}
