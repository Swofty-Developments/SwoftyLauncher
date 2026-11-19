#include "core/career/plain_saves.hpp"

#include <windows.h>

#include <swofty/mixin.hpp>

#include <chrono>
#include <ctime>
#include <mutex>
#include <thread>

#include "core/runtime/build.hpp"
#include "core/runtime/files.hpp"
#include "core/runtime/runtime.hpp"

#ifndef SWOFTY_CORE_VERSION
#define SWOFTY_CORE_VERSION ""
#endif

namespace career::plain_saves {

namespace {

struct Config {
    bool on = false;
    fs::path root;
    std::string profile, session, root_text;
};

std::mutex write_lock;

const Config &config()
{
    static const Config cfg = [] {
        Config c;
        Settings s = parse_settings(runtime::settings_json_text());
        if (!s.present)
            return c;
        if (!s.ok) {
            swofty::log("[saves] plain store off: %s", s.why.c_str());
            return c;
        }
        c.on = true;
        c.root_text = s.root;
        c.root = fs::path(runtime::wide(s.root));
        json::Value doc;
        if (json::parse(runtime::settings_json_text(), doc))
            c.profile = doc["launcher_saves"]["profile"].str();
        c.session = std::to_string(
            std::chrono::duration_cast<std::chrono::milliseconds>(std::chrono::system_clock::now().time_since_epoch()).count());
        swofty::log("[saves] plain store on: %s", s.root.c_str());
        return c;
    }();
    return cfg;
}

Header header_for(const Config &c, const char *event)
{
    Header h;
    h.event = event;
    h.core_version = SWOFTY_CORE_VERSION;
    h.exe_sha256 = runtime::build_exe_sha();
    h.profile = c.profile;
    h.session = c.session;
    h.written_at = int64_t(time(nullptr));
    return h;
}

}

bool enabled() { return config().on; }

std::vector<Added> at_load(const std::set<std::string> &registered, const std::set<std::string> &kept)
{
    std::vector<Added> out;
    const Config &c = config();
    if (!c.on)
        return out;
    Root root(c.root);
    std::set<std::string> tree = registered;
    tree.insert(kept.begin(), kept.end());
    std::set<std::string> added_now;
    if (auto newest = newest_core_save(root)) {
        for (const Pick &p : put_back(*newest, tree)) {
            auto bytes = read_container(root, p.generation, p.entry);
            if (!bytes) {
                swofty::log("[saves] could not put back %s: generation %llu has no intact copy of it", p.entry.name.c_str(),
                            (unsigned long long)p.generation);
                continue;
            }
            swofty::log("%s", put_back_line(p.entry.name, p.entry.careers, p.generation).c_str());
            out.push_back({p.entry.name, std::move(*bytes), false, false});
            added_now.insert(p.entry.name);
        }
    }
    std::error_code ec;
    if (!fs::is_regular_file(root.path() / INJECT, ec))
        return out;
    std::vector<Request> requests = parse_requests(read_text(root.path() / INJECT, size_t(1) << 20));
    std::vector<Result> results;
    for (const Request &q : requests) {
        Held held = registered.count(q.container)                               ? Held::registered
                    : (kept.count(q.container) || added_now.count(q.container)) ? Held::kept
                                                                                : Held::none;
        std::optional<std::vector<uint8_t>> bytes;
        if (auto m = read_generation(root, q.generation))
            for (const Entry &e : m->containers)
                if (e.name == q.container) {
                    bytes = read_container(root, q.generation, e);
                    break;
                }
        Decision d = decide(q, held, bool(bytes));
        results.push_back(d.result);
        if (d.action == Action::none)
            continue;
        if (d.action == Action::replace || held == Held::kept)
            out.erase(std::remove_if(out.begin(), out.end(), [&](const Added &a) { return a.name == q.container; }), out.end());
        out.push_back({q.container, std::move(*bytes), d.action == Action::replace, true});
        added_now.insert(q.container);
        swofty::log("[saves] inject request %s: %s", q.id.c_str(), d.result.detail.c_str());
    }
    for (const Result &r : results)
        if (!r.ok)
            swofty::log("[saves] inject request %s refused: %s", r.id.c_str(), r.detail.c_str());
    if (!answer(root, results))
        swofty::log("[saves] could not write injected.json; inject.json is left for the next load");
    return out;
}

void write_async(const char *event, std::vector<Body> bodies)
{
    const Config &c = config();
    if (!c.on)
        return;
    Header h = header_for(c, event);
    std::string ev = event;
    try {
        std::thread([c = &c, h = std::move(h), ev, bodies = std::move(bodies)]() mutable {
            std::lock_guard<std::mutex> g(write_lock);
            std::string why;
            uint64_t n = Root(c->root).write_generation(h, bodies, why);
            if (n)
                swofty::log("[saves] plain store: wrote generation %llu (%s, %zu container(s))", (unsigned long long)n,
                            ev.c_str(), bodies.size());
            else
                swofty::log("[saves] plain store: the %s generation was not written (%s); the game's save is untouched",
                            ev.c_str(), why.c_str());
        }).detach();
    } catch (...) {
        swofty::log("[saves] plain store: could not start the writer thread; the game's save is untouched");
    }
}

}
