#pragma once
#include <windows.h>

#include <safetyhook.hpp>

#include "core/frontend/name_hash.hpp"

#include <cstddef>
#include <cstdint>
#include <string>
#include <vector>

namespace frontend::ui {

struct NeStrHdr {
    uint32_t hash;
    uint16_t refs;
};

struct Callback {
    void (*fn)(void *ctx, int status);
    void *ctx;
};

enum class State { idle, armed, pending, in_table, dropped };

struct Screen {
    std::string name, file, flow, anchor;
    uint32_t hash = 0;
    std::string data;
    void *resid = nullptr, *failed_resid = nullptr, *noted_resid = nullptr;
    State state = State::idle;
    bool engine = false, started = false, result_seen = false, view_model = false, vm_attached = false, vm_noted = false;
    int result = 0;
    unsigned attempts = 0;
};

constexpr unsigned MAX_ATTEMPTS = 3;

struct Target {
    const char *what, *pattern;
    uintptr_t rva;
    uintptr_t anchor = 0;
};

using Handler_t = void(void *self, int64_t *resid, void *a3, uint8_t *frags, int64_t count, uint8_t *res);
using Parser_t = int(void *mgr, void *pkg, void *resid, Callback *cb, int flag);
using PkgOpen_t = int(const void *data, void **out);
using RouteExec_t = NeStrHdr **(void *self, NeStrHdr **err, void *cmd);
using Open_t = uint64_t(void *scene, void *screen, void *a3, void *a4);
using Table_t = uint8_t *(void *self);

constexpr unsigned MAX_ENGINE_FAILURES = 50;

using frontend::name_hash::djb2;
const char *chars(const void *s);
bool readable(const void *p, size_t n);

void on_parsed(void *ctx, int);
void handler_hook(void *self, int64_t *resid, void *a3, uint8_t *frags, int64_t count, uint8_t *res);
uint64_t open_hook(void *scene, void *screen, void *a3, void *a4);
NeStrHdr **resolve_hook(void *self, NeStrHdr **err, void *cmd);
NeStrHdr **openexec_hook(void *self, NeStrHdr **err, void *cmd);
void read_site(safetyhook::Context &c);
void init_site(safetyhook::Context &c);
void factory_site(safetyhook::Context &c);
void screen_site(safetyhook::Context &c);

class ScreenLoader {
public:
    void load();
    void parsed(void *ctx, int);
    void loaded(void *self, int64_t *resid, void *a3, uint8_t *frags, int64_t count, uint8_t *res);
    uint64_t opened(void *scene, void *screen, void *a3, void *a4);
    NeStrHdr **route_resolved(void *self, NeStrHdr **err, void *cmd);
    NeStrHdr **route_opened(void *self, NeStrHdr **err, void *cmd);
    void node_read(safetyhook::Context &c);
    void node_init(safetyhook::Context &c);
    void node_no_object(safetyhook::Context &c);
    void screen_built(safetyhook::Context &c);

private:
    uintptr_t find(const Target &t);
    void load_config();
    template <class F> bool install(safetyhook::InlineHook &h, uintptr_t at, F *fn, const char *what);
    void mid(safetyhook::MidHook &h, const Target &t, void (*fn)(safetyhook::Context &), const char *what);
    void *screen_lookup(uint32_t h, bool *ok);
    void register_pending(const char *why);
    bool guarded(const std::string &name);
    NeStrHdr **cancel_missing(NeStrHdr **err, const char *step);
    Screen *job_owner(uint8_t *job);
    void node_failure(uint8_t *job, uint8_t *rec, int code, const char *what);

    std::vector<Screen> screens;
    bool strict = false, do_register = true, recover = true, diagnostics = false, perf = false;
    SRWLOCK lock = SRWLOCK_INIT;
    uintptr_t base = 0, a_handler = 0, a_parser = 0, a_pkgopen = 0, a_resolve = 0, a_openexec = 0, a_open = 0;
    void **screen_sys = nullptr;
    void *mgr = nullptr;
    void *last_scene = nullptr, *last_screen = nullptr, *prev_scene = nullptr, *prev_screen = nullptr;
    safetyhook::InlineHook h_handler, h_resolve, h_openexec, h_open;
    safetyhook::MidHook m_read, m_init, m_factory, m_screen;
    unsigned engine_failures = 0;
    bool perf_probed = false;
};

extern ScreenLoader loader;

}
