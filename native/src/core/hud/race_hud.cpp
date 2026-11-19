#include "core/runtime/runtime.hpp"

#include <swofty/mixin.hpp>

#include <algorithm>
#include <atomic>
#include <cstdlib>
#include <cstring>
#include <deque>
#include <mutex>
#include <string>
#include <vector>

#include "core/diagnostics/perf.hpp"
#include "core/frontend/popup/message_system.hpp"
#include "core/frontend/ui_vm.hpp"
#include "core/hud/race_hud.hpp"

namespace hud::race_hud {

RaceHud state{frontend::popup::messages.queue_lock};

namespace {

constexpr const char *SET_PREFIXES[] = {"ui_menu_data.hud_", "view_model_data.hud.", "view_model_data.hud_"};
constexpr size_t MAX_SETS = 512;

int api_set(const char *path, uint32_t type, const void *value)
{
    std::string by = frontend::popup::who(__builtin_return_address(0));
    if (!path || !value)
        return SWOFTY_ASSET_INVALID;
    if (type == 0)
        return state.set_enqueue(path, 0, static_cast<const char *>(value), 0, by);
    return state.set_enqueue(path, int(type), "", *static_cast<const int32_t *>(value), by);
}

void apply_set(const Set &q)
{
    bool ok =
        q.type == 0 ? frontend::ui_vm_set_path(q.path, 0, q.text.c_str()) : frontend::ui_vm_set_path(q.path, q.type, &q.number);
    if (!ok || state.sets_told.fetch_add(1) < 16)
        swofty::log("[hud] set %s by %s: %s = %s%s", ok ? "applied" : "failed", q.by.c_str(), q.path.c_str(),
                    q.type == 0 ? ("\"" + q.text.substr(0, 60) + "\"").c_str() : std::to_string(q.number).c_str(),
                    ok ? "" : " (the parent node did not resolve)");
}

int leaves_register(const SwoftyHudLeaf *leaves, uint32_t n, uint32_t *ids)
{
    std::string by = frontend::popup::who(__builtin_return_address(0));
    if (!leaves || !ids)
        return SWOFTY_ASSET_INVALID;
    if (!frontend::popup::api_available())
        return SWOFTY_ASSET_UNAVAILABLE;
    int rc = SWOFTY_ASSET_OK;
    for (uint32_t i = 0; i < n; i++) {
        ids[i] = ~0u;
        if (!leaves[i].path || leaves[i].size < offsetof(SwoftyHudLeaf, text_cap) + sizeof(uint32_t)) {
            rc = SWOFTY_ASSET_INVALID;
            continue;
        }
        std::string path = leaves[i].path;
        if (!path.empty() && path[0] == '#')
            path.erase(0, 1);
        if (state.leaf_table.register_leaf(path, leaves[i].type, leaves[i].text_cap, by, ids[i]) != hud::leaves::OK) {
            swofty::log("[hud] leaf registration by %s refused: '%.120s' (type %u) is not a hud_* leaf or is already "
                        "registered with another type",
                        by.c_str(), path.c_str(), leaves[i].type);
            rc = SWOFTY_ASSET_INVALID;
        }
    }
    if (!state.leaves_logged.exchange(true) || n > 64)
        swofty::log("[hud] %u leaves registered by %s (%zu in the table)", n, by.c_str(), state.leaf_table.size());
    return rc;
}

int leaves_set(const uint32_t *ids, const void *const *values, uint32_t n)
{
    if (!ids || !values)
        return SWOFTY_ASSET_INVALID;
    return state.leaf_table.set(ids, values, n) == hud::leaves::OK ? SWOFTY_ASSET_OK : SWOFTY_ASSET_INVALID;
}

int leaves_begin()
{
    state.leaf_table.begin();
    return SWOFTY_ASSET_OK;
}

int leaves_end()
{
    state.leaf_table.end();
    return SWOFTY_ASSET_OK;
}

uint32_t leaves_generation() { return state.leaf_table.generation(); }

int leaves_stats(SwoftyHudStats *out)
{
    if (!out || out->size < offsetof(SwoftyHudStats, applied_last_tick) + sizeof(uint32_t))
        return SWOFTY_ASSET_INVALID;
    auto s = state.leaf_table.stats();
    SwoftyHudStats full{sizeof(SwoftyHudStats),
                        s.applied_last_tick,
                        s.queued,
                        state.sets_dropped.load(),
                        s.registered,
                        s.cap_per_tick,
                        s.failed,
                        s.generation,
                        s.resyncs,
                        uint32_t(state.sets_pending.load())};
    memcpy(out, &full, std::min<size_t>(out->size, sizeof full));
    return SWOFTY_ASSET_OK;
}

const SwoftyHud hud_table = {SWOFTY_HUD_VERSION,
                             frontend::popup::api_available,
                             frontend::popup::api_toast,
                             api_set,
                             leaves_register,
                             leaves_set,
                             leaves_begin,
                             leaves_end,
                             leaves_generation,
                             leaves_stats};

bool apply_leaf(const hud::leaves::Leaf &l)
{
    perf::Pause game;
    bool ok = l.type == 0 ? frontend::ui_vm_set_path(l.path, 0, l.want_text.c_str())
                          : frontend::ui_vm_set_path(l.path, int(l.type), &l.want_num);
    if (!ok && !l.failed && state.leaves_told.fetch_add(1) < 16)
        swofty::log("[hud] leaf %s by %s did not resolve yet (the parent node is missing); retrying", l.path.c_str(),
                    l.by.c_str());
    return ok;
}

}

int RaceHud::set_enqueue(std::string path, int type, std::string text, int32_t number, const std::string &by)
{
    if (!path.empty() && path[0] == '#')
        path.erase(0, 1);
    bool listed = std::any_of(std::begin(SET_PREFIXES), std::end(SET_PREFIXES),
                              [&](const char *p) { return path.rfind(p, 0) == 0 && path.size() > strlen(p); });
    if (!listed || path.find_first_of(" []") != std::string::npos || path.back() == '.' || type < 0 || type > 2) {
        swofty::log("[hud] set by %s refused: '%.120s' is not a hud_* leaf (ui_menu_data.hud_*, view_model_data.hud*)",
                    by.c_str(), path.c_str());
        return SWOFTY_ASSET_INVALID;
    }
    if (!frontend::popup::api_available())
        return SWOFTY_ASSET_UNAVAILABLE;
    int registered = leaf_table.type_of(path);
    if (registered >= 0 && registered != type) {
        swofty::log("[hud] set by %s refused: '%.120s' is registered as type %d, not %d", by.c_str(), path.c_str(), registered,
                    type);
        return SWOFTY_ASSET_INVALID;
    }
    if (text.size() > frontend::popup::MAX_TOAST_TEXT)
        text.resize(frontend::popup::MAX_TOAST_TEXT);
    std::lock_guard<std::mutex> g(queue_lock);
    for (auto &q : sets)
        if (q.path == path) {
            q = {path, std::move(text), by, type, number};
            return SWOFTY_ASSET_OK;
        }
    if (sets.size() >= MAX_SETS) {
        swofty::log("[hud] set queue full (%zu); dropping the oldest, from %s", sets.size(), sets.front().by.c_str());
        sets.pop_front();
        sets_dropped++;
    }
    sets.push_back({std::move(path), std::move(text), by, type, number});
    sets_pending = sets.size();
    return SWOFTY_ASSET_OK;
}

void RaceHud::leaves_watch(uint64_t now)
{
    EpochWatch &w = epoch_watch;
    if (!leaf_table.size() || now < epoch_next_check)
        return;
    epoch_next_check = now + 250;
    if (w.known != leaf_table.size()) {
        w.known = leaf_table.size();
        w.parents = leaf_table.parents();
        w.tokens.assign(w.parents.size(), 0);
        w.epoch = frontend::ui_vm_epoch();
        w.next = 0;
    }
    bool rebuilt = false;
    uintptr_t e = frontend::ui_vm_epoch();
    if (e != w.epoch) {
        if (w.epoch)
            rebuilt = true;
        w.epoch = e;
    }
    for (size_t k = 0; k < 4 && !w.parents.empty(); k++) {
        size_t i = w.next++ % w.parents.size();
        uintptr_t t = frontend::ui_vm_node_token(w.parents[i]);
        if (w.tokens[i] && t != w.tokens[i])
            rebuilt = true;
        if (t)
            w.tokens[i] = t;
    }
    if (rebuilt) {
        swofty::log("[hud] view model rebuilt (generation %u); re-sending %zu leaves", leaf_table.generation() + 1,
                    leaf_table.size());
        leaf_table.rebuilt();
        w.tokens.assign(w.parents.size(), 0);
    }
}

void RaceHud::set_test_tick()
{
    if (set_test_text.empty())
        return;
    uint64_t now = GetTickCount64();
    if (now < set_test_next)
        return;
    set_test_next = now + 5000;
    if (set_test_text.size() > 2 && set_test_text[1] == ':') {
        size_t at = 0;
        while (at < set_test_text.size()) {
            size_t end = set_test_text.find(';', at);
            std::string item = set_test_text.substr(at, end == std::string::npos ? std::string::npos : end - at);
            at = end == std::string::npos ? set_test_text.size() : end + 1;
            size_t eq = item.find('=');
            if (item.size() < 3 || item[1] != ':' || eq == std::string::npos)
                continue;
            int type = item[0] == 'b' ? 1 : item[0] == 'i' ? 2 : 0;
            std::string value = item.substr(eq + 1);
            set_enqueue(item.substr(2, eq - 2), type, value, type ? int32_t(atoi(value.c_str())) : 0, "SWOFTY_HUD_SET_TEST");
        }
        return;
    }
    int32_t off = 0;
    set_enqueue("ui_menu_data.hud_lap_number_visible", 1, "", off, "SWOFTY_HUD_SET_TEST");
    set_enqueue("ui_menu_data.hud_lap_description_loc_tag", 1, "", off, "SWOFTY_HUD_SET_TEST");
    set_enqueue("ui_menu_data.hud_lap_description_string", 0, set_test_text, 0, "SWOFTY_HUD_SET_TEST");
    set_enqueue("ui_menu_data.hud_total_lap_count_visible", 1, "", off, "SWOFTY_HUD_SET_TEST");
    set_enqueue("ui_menu_data.hud_top_left_position_visible", 1, "", off, "SWOFTY_HUD_SET_TEST");
}

void RaceHud::dev_set_parse(const std::string &spec)
{
    size_t at = 0;
    while (at < spec.size()) {
        size_t end = spec.find(';', at);
        std::string item = spec.substr(at, end == std::string::npos ? std::string::npos : end - at);
        at = end == std::string::npos ? spec.size() : end + 1;
        size_t eq = item.find('=');
        if (item.size() < 3 || item[1] != ':' || eq == std::string::npos)
            continue;
        Set q{item.substr(2, eq - 2), item.substr(eq + 1), "SWOFTY_HUD_DEV_SET", 0, 0};
        q.type = item[0] == 'b' ? 1 : item[0] == 'i' ? 2 : 0;
        if (q.type)
            q.number = int32_t(atoi(q.text.c_str()));
        dev_sets.push_back(q);
        swofty::log("[hud] SWOFTY_HUD_DEV_SET: %s = %s every 5 s (dev switch, any path)", q.path.c_str(), q.text.c_str());
    }
}

void RaceHud::dev_set_tick()
{
    if (dev_sets.empty())
        return;
    uint64_t now = GetTickCount64();
    if (now < dev_set_next)
        return;
    dev_set_next = now + 5000;
    for (auto &q : dev_sets)
        apply_set(q);
}

void RaceHud::read_switches()
{
    char dev[512] = {};
    if (GetEnvironmentVariableA("SWOFTY_HUD_DEV_SET", dev, sizeof dev) && dev[0])
        dev_set_parse(dev);
    char lap[128] = {};
    if (GetEnvironmentVariableA("SWOFTY_HUD_SET_TEST", lap, sizeof lap) && lap[0]) {
        set_test_text = lap;
        if (lap[1] == ':')
            swofty::log("[hud] SWOFTY_HUD_SET_TEST: writes %s through set every 5 s", lap);
        else
            swofty::log("[hud] SWOFTY_HUD_SET_TEST: the lap counter shows \"%s\", lap totals and the top-left position "
                        "tower hide (set every 5 s)",
                        lap);
    }
}

void RaceHud::tick()
{
    set_test_tick();
    dev_set_tick();
    if (leaf_table.size()) {
        static const uint32_t leaf_probe = perf::probe("core", "hud leaves");
        perf::Scope p(leaf_probe);
        uint64_t now = GetTickCount64();
        leaves_watch(now);
        leaf_table.drain(now, apply_leaf);
    }
    if (sets_pending.load(std::memory_order_relaxed)) {
        std::deque<Set> now_sets;
        {
            std::lock_guard<std::mutex> g(queue_lock);
            now_sets.swap(sets);
            sets_pending = 0;
        }
        static const uint32_t set_probe = perf::probe("core", "hud set");
        perf::Scope p(set_probe);
        for (auto &q : now_sets) {
            perf::Pause game;
            apply_set(q);
        }
    }
}

}

extern "C" __declspec(dllexport) const SwoftyHud *SwoftyCoreHud(void) { return &hud::race_hud::hud_table; }
