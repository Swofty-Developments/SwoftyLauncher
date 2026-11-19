#include <windows.h>

#include <safetyhook.hpp>
#include <swofty/mixin.hpp>

#include <cstdio>
#include <string>

#include "core/diagnostics/perf.hpp"
#include "core/frontend/ui/screen_loader.hpp"

namespace frontend::ui {

namespace {

thread_local char cur_screen[96];

std::string fourcc_text(uint32_t v)
{
    std::string out;
    for (int i = 0; i < 4; i++) {
        char c = char((v >> (8 * i)) & 0xff);
        out.push_back(c >= 0x20 && c < 0x7f ? c : '?');
    }
    return out;
}

void node_text(uint8_t *rec, char *name, size_t n, uint32_t *fourcc, uint32_t *index)
{
    *fourcc = readable(rec + 0x70, 4) ? *reinterpret_cast<uint32_t *>(rec + 0x70) : 0;
    *index = readable(rec + 0x88, 4) ? *reinterpret_cast<uint32_t *>(rec + 0x88) : ~0u;
    const char *p = readable(rec + 0x68, 8) ? *reinterpret_cast<const char **>(rec + 0x68) : nullptr;
    snprintf(name, n, "%s", p && readable(p, 1) ? p : "?");
}

bool list_base(uint32_t cls)
{
    switch (cls) {
    case 0x47544955:
    case 0x42544955:
    case 0x544c4955:
    case 0x324c4955:
    case 0x4c454955:
        return true;
    }
    return false;
}

const char *init_reason(uint32_t cls, int code)
{
    if (list_base(cls) && code == 3)
        return "its 'data' property has no dbentry binding: this class needs data=@<array path> (every shipping "
               "UITG also binds selection=@<value path>); a literal or empty 'data' value fails. Code 3 can also come "
               "from the element base: the node's layout attach (slot 4) lacks componentId or nodeId";
    if (list_base(cls) && code == 5)
        return "its 'data' property is missing, is not a {value, dbentry} object, or has an empty dbentry";
    if (code == 3)
        return "the element base rejected it: the node's layout attach (slot 4) lacks componentId or nodeId, or a "
               "required property object lacks its value/dbentry key";
    if (cls == 0x4443564d)
        return "MVCD init rejected its data (its init body is Denuvo-relocated and not mapped; compare its properties "
               "with a shipping MVCD)";
    return "was rejected by its class (init from data returned non-zero)";
}

}

void read_site(safetyhook::Context &c) { loader.node_read(c); }

void init_site(safetyhook::Context &c) { loader.node_init(c); }

void factory_site(safetyhook::Context &c) { loader.node_no_object(c); }

void screen_site(safetyhook::Context &c) { loader.screen_built(c); }

Screen *ScreenLoader::job_owner(uint8_t *job)
{
    if (!job || !readable(job + 0xa0, 16) || *reinterpret_cast<void **>(job + 0xa0) != reinterpret_cast<void *>(&on_parsed))
        return nullptr;
    auto *ctx = *reinterpret_cast<Screen **>(job + 0xa8);
    for (auto &s : screens)
        if (&s == ctx)
            return ctx;
    return nullptr;
}

void ScreenLoader::node_failure(uint8_t *job, uint8_t *rec, int code, const char *what)
{
    Screen *s = job_owner(job);
    if (!s && !strict && ++engine_failures > MAX_ENGINE_FAILURES)
        return;
    char name[96];
    uint32_t cls, idx;
    node_text(rec, name, sizeof name, &cls, &idx);
    if (s)
        s->result = code;
    swofty::log("[ui] %s screen '%s'%s%s: node '%s' (index %u, class %s) %s, code %d (0x%x)", s ? "addon" : "engine", cur_screen,
                s ? " from " : "", s ? s->file.c_str() : "", name, idx, fourcc_text(cls).c_str(), what, code, code);
}

void ScreenLoader::node_read(safetyhook::Context &c)
{
    auto *job = reinterpret_cast<uint8_t *>(c.rbx), *rec = reinterpret_cast<uint8_t *>(c.r13);
    int rc = int(c.rax);
    uint32_t cls = *reinterpret_cast<const volatile uint32_t *>(rec + 0x70);
    if (!rc && cls != 0x43534955)
        return;
    Screen *s = job_owner(job);
    if (cls == 0x43534955) {
        const char *p = readable(rec + 0x68, 8) ? *reinterpret_cast<const char **>(rec + 0x68) : nullptr;
        snprintf(cur_screen, sizeof cur_screen, "%s", p && readable(p, 1) ? p : "?");
        if (s && !s->started) {
            s->started = true;
            swofty::log("[ui] screen %s (%s): build job started, root node '%s'", s->name.c_str(), s->flow.c_str(), cur_screen);
        }
    }
    if (rc == 0x55 && s)
        swofty::log("[ui] addon screen %s: a screen named '%s' is already registered with an equal or higher priority; "
                    "the engine skips this one",
                    s->name.c_str(), cur_screen);
    else if (rc && rc != 0x55)
        node_failure(job, rec, rc, "could not be read from the .ui data");
}

void ScreenLoader::node_init(safetyhook::Context &c)
{
    int rc = int(c.rax);
    if (!rc || rc == 0x23 || rc == 0x57)
        return;
    auto *rec = reinterpret_cast<uint8_t *>(c.r13);
    uint32_t cls = readable(rec + 0x70, 4) ? *reinterpret_cast<uint32_t *>(rec + 0x70) : 0;
    node_failure(reinterpret_cast<uint8_t *>(c.rbx), rec, rc, init_reason(cls, rc));
}

void ScreenLoader::node_no_object(safetyhook::Context &c)
{
    node_failure(reinterpret_cast<uint8_t *>(c.rbx), reinterpret_cast<uint8_t *>(c.r13), 5,
                 "has no registered class factory, or its factory returned no object");
}

void ScreenLoader::screen_built(safetyhook::Context &c)
{
    static const uint32_t probe = perf::probe("core", "ui screen built");
    perf::Scope p(probe);
    auto *job = reinterpret_cast<uint8_t *>(c.rbx);
    int rc = int(c.rbp);
    Screen *s = job_owner(job);
    if (!m_read)
        snprintf(cur_screen, sizeof cur_screen, "%s", s ? s->name.c_str() : "?");
    if (s) {
        s->result = rc;
        s->result_seen = true;
        swofty::log("[ui] screen %s (%s): root '%s' built with result 0x%x (%s)", s->name.c_str(), s->flow.c_str(), cur_screen,
                    rc,
                    rc == 0      ? "ok, the engine inserts it into the screen table next"
                    : rc == 0x55 ? "skipped: a same-name screen is already registered"
                                 : "failed: the engine drops it and ends this build job");
    } else if (rc && rc != 0x55 && (strict || ++engine_failures <= MAX_ENGINE_FAILURES)) {
        swofty::log("[ui] engine screen '%s' failed to build, result 0x%x; the engine drops it", cur_screen, rc);
    }
}

}
