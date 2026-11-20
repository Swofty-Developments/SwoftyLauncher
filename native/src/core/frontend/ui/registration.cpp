#include <windows.h>

#include <safetyhook.hpp>
#include <swofty/mixin.hpp>

#include <string>
#include <string_view>
#include <utility>
#include <vector>

#include "core/diagnostics/perf.hpp"
#include "core/frontend/ui/screen_loader.hpp"
#include "core/frontend/ui_vm.hpp"

namespace frontend::ui {

namespace {

bool contains(const uint8_t *data, size_t size, const std::string &name)
{
    if (!data || !size || name.empty() || name.size() > 250 || !readable(data, size))
        return false;
    std::string needle;
    needle.push_back(char(name.size()));
    needle += name;
    needle.push_back('\0');
    return std::string_view(reinterpret_cast<const char *>(data), size).find(needle) != std::string_view::npos;
}

}

void on_parsed(void *ctx, int status) { loader.parsed(ctx, status); }

void handler_hook(void *self, int64_t *resid, void *a3, uint8_t *frags, int64_t count, uint8_t *res)
{
    loader.loaded(self, resid, a3, frags, count, res);
}

void *ScreenLoader::screen_lookup(uint32_t h, bool *ok)
{
    *ok = false;
    if (!screen_sys || !readable(screen_sys, 8) || !*screen_sys || !readable(*screen_sys, 8))
        return nullptr;
    void *sys = *screen_sys;
    auto fn = reinterpret_cast<Table_t *>((*static_cast<void ***>(sys))[0x48 / 8]);
    uint8_t *t = fn(sys);
    if (!t || !readable(t + 0x98, 0x38))
        return nullptr;
    uint64_t mul = *reinterpret_cast<uint64_t *>(t + 0x98), mask = *reinterpret_cast<uint64_t *>(t + 0xb8);
    auto *entries = *reinterpret_cast<uint8_t **>(t + 0xa0), *meta = *reinterpret_cast<uint8_t **>(t + 0xa8);
    uint32_t step = *reinterpret_cast<uint32_t *>(t + 0xc8);
    uint8_t shift = *(t + 0xcc);
    *ok = true;
    if (!mask || !entries || !meta)
        return nullptr;
    uint64_t u = uint64_t(h) * mul;
    u = (u >> 0x21) ^ u;
    uint32_t dist = (uint32_t(u) & 0x1f) >> (shift & 0x3f);
    uint32_t d0 = step + dist, d1 = dist + step * 2;
    uint64_t i = (u >> 5) & mask;
    for (unsigned guard = 0; guard < 4096; guard++) {
        uint8_t b0 = meta[i], b1 = meta[i + 1];
        if (d0 == b0 && *reinterpret_cast<uint32_t *>(entries + i * 16) == h)
            return *reinterpret_cast<void **>(entries + i * 16 + 8);
        if (d1 == b1 && *reinterpret_cast<uint32_t *>(entries + (i + 1) * 16) == h)
            return *reinterpret_cast<void **>(entries + (i + 1) * 16 + 8);
        d0 += step * 2;
        d1 += step * 2;
        uint8_t b2 = meta[i + 2];
        i += 2;
        if (d0 > b2)
            break;
    }
    return nullptr;
}

void ScreenLoader::parsed(void *ctx, int)
{
    auto *s = static_cast<Screen *>(ctx);
    bool ok = false;
    void *scr = screen_lookup(s->hash, &ok);
    s->state = scr ? State::in_table : State::dropped;
    if (!scr)
        s->failed_resid = s->resid;
    if (scr)
        swofty::log("[ui] screen %s (%s): build job finished, in screen table: yes", s->name.c_str(), s->flow.c_str());
    else if (!s->started)
        swofty::log("[ui] screen %s (%s): build job finished without reading its root node (the job never reached "
                    "its screen); in screen table: %s",
                    s->name.c_str(), s->flow.c_str(), ok ? "no" : "table unreadable");
    else
        swofty::log("[ui] screen %s (%s): build job finished, in screen table: %s, screen result %s0x%x", s->name.c_str(),
                    s->flow.c_str(), ok ? "no" : "table unreadable", s->result_seen ? "" : "not reached, last node code ",
                    s->result);
}

void ScreenLoader::register_pending(const char *why)
{
    if (!mgr || !do_register)
        return;
    for (auto &s : screens) {
        if (s.state != State::armed)
            continue;
        if (s.view_model && !s.vm_attached)
            s.vm_attached = ui_vm_attach(s.name);
        if (s.view_model && !s.vm_attached) {
            s.state = State::idle;
            if (!s.vm_noted)
                swofty::log("[ui] screen %s (%s) needs a view model and %s; not registered, so routes to it are "
                            "cancelled and the front end stays where it is",
                            s.name.c_str(), s.flow.c_str(),
                            ui_vm_ready() ? "its view model is not attached yet" : "view models are off on this build");
            s.vm_noted = true;
            continue;
        }
        bool ok = false;
        if (screen_lookup(s.hash, &ok)) {
            s.state = State::in_table;
            s.engine = !s.attempts;
            swofty::log("[ui] screen %s (%s) is already in the screen table%s; nothing to do", s.name.c_str(), s.flow.c_str(),
                        s.engine ? " (registered by the engine)" : "");
            continue;
        }
        s.state = State::idle;
        if (s.data.empty()) {
            swofty::log("[ui] screen %s (%s): no data (%s missing or empty); not registered", s.name.c_str(), s.flow.c_str(),
                        s.file.c_str());
            continue;
        }
        void *pkg = nullptr;
        int rc = reinterpret_cast<PkgOpen_t *>(a_pkgopen)(s.data.data(), &pkg);
        if (rc || !pkg) {
            swofty::log("[ui] screen %s (%s): %s is not a valid !pkg (code %d); not registered", s.name.c_str(), s.flow.c_str(),
                        s.file.c_str(), rc);
            continue;
        }
        Callback cb{on_parsed, &s};
        s.attempts++;
        s.state = State::pending;
        rc = reinterpret_cast<Parser_t *>(a_parser)(mgr, pkg, s.resid, &cb, 0);
        if (rc && rc != 0x23) {
            s.state = State::dropped;
            s.failed_resid = s.resid;
            swofty::log("[ui] screen %s (%s): engine screen loader refused it (code %d)", s.name.c_str(), s.flow.c_str(), rc);
            continue;
        }
        s.started = s.result_seen = false;
        s.result = 0;
        auto *jb = *reinterpret_cast<uint8_t **>(static_cast<uint8_t *>(mgr) + 0xb900);
        auto *je = *reinterpret_cast<uint8_t **>(static_cast<uint8_t *>(mgr) + 0xb908);
        size_t jobs = jb && je > jb ? size_t(je - jb) / 0xd0 : 0;
        swofty::log("[ui] registered screen %s (%s) via the engine screen loader (parse returned %d = build job queued, "
                    "position %zu of %zu), attempt %u, %zu bytes, resource %p, trigger %s",
                    s.name.c_str(), s.flow.c_str(), rc, jobs, jobs, s.attempts, s.data.size(), s.resid, why);
    }
}

void ScreenLoader::loaded(void *self, int64_t *resid, void *a3, uint8_t *frags, int64_t count, uint8_t *res)
{
    static const uint32_t probe = perf::probe("core", "ui screen loader handler");
    h_handler.call<void>(self, resid, a3, frags, count, res);
    if (!self)
        return;
    perf::Scope p(probe);
    AcquireSRWLockExclusive(&lock);
    if (!mgr) {
        mgr = *reinterpret_cast<void **>(static_cast<uint8_t *>(self) + 0x10);
        swofty::log("[ui] screen loader manager %p (handler %p)", mgr, self);
    }
    void *copy = res && readable(res + 0x38, 8) ? *reinterpret_cast<uint8_t **>(res + 0x38) : nullptr;
    for (int64_t k = 0; k < count && k < 16; k++) {
        uint8_t *e = frags + k * 32;
        if (!readable(e, 32) || *reinterpret_cast<uint32_t *>(e + 0x10) != 0x706d6574)
            continue;
        auto *data = *reinterpret_cast<uint8_t **>(e);
        size_t size = *reinterpret_cast<uint64_t *>(e + 8);
        std::vector<std::pair<const std::string *, bool>> seen;
        auto has_anchor = [&](const std::string &anchor) {
            for (auto &[a, hit] : seen)
                if (*a == anchor)
                    return hit;
            bool hit = contains(data, size, anchor);
            seen.push_back({&anchor, hit});
            return hit;
        };
        for (auto &s : screens) {
            if (!copy || !has_anchor(s.anchor))
                continue;
            void *rid = static_cast<uint8_t *>(copy) + 8;
            bool ok = false;
            switch (s.state) {
            case State::armed:
            case State::pending:
                break;
            case State::in_table:
                if (screen_lookup(s.hash, &ok) || !ok) {
                    if (s.noted_resid != rid)
                        swofty::log("[ui] screen %s: anchor '%s' resource %p loaded; already in the screen table, "
                                    "not registering again",
                                    s.name.c_str(), s.anchor.c_str(), rid);
                    s.noted_resid = rid;
                    break;
                }
                [[fallthrough]];
            case State::idle:
                s.resid = rid;
                s.state = State::armed;
                swofty::log("[ui] screen %s: anchor '%s' (%s) loaded, resource %p; registering at the next screen open",
                            s.name.c_str(), s.anchor.c_str(), s.flow.c_str(), rid);
                break;
            case State::dropped:
                if (rid == s.failed_resid || s.attempts >= MAX_ATTEMPTS) {
                    if (s.noted_resid != rid)
                        swofty::log("[ui] screen %s: anchor resource %p loaded; not retrying (dropped %u time(s); a "
                                    "retry needs a fresh flow load and fewer than %u attempts)",
                                    s.name.c_str(), rid, s.attempts, MAX_ATTEMPTS);
                    s.noted_resid = rid;
                    break;
                }
                s.resid = rid;
                s.state = State::armed;
                swofty::log("[ui] screen %s: flow reloaded (resource %p); retrying after %u failed attempt(s)", s.name.c_str(),
                            rid, s.attempts);
                break;
            }
        }
    }
    ReleaseSRWLockExclusive(&lock);
}

}
