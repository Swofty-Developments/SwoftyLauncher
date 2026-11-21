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
#include "core/testing/save_sandbox_logic.hpp"

#ifndef SWOFTY_CORE_VERSION
#define SWOFTY_CORE_VERSION ""
#endif

namespace career::plain_saves {

namespace {

struct Config {
    bool on = false;
    fs::path root;
    std::string profile, session, root_text;
    std::vector<std::string> addons;
};

std::mutex write_lock;

std::string env_text(const char *name)
{
    char v[16] = {};
    DWORD n = GetEnvironmentVariableA(name, v, sizeof v);
    return n && n < sizeof v ? std::string(v, n) : std::string();
}

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
        std::string other = off_for_session(runtime::settings_json_text(), env_text(runtime::save_sandbox::OFF_ENV));
        if (!other.empty()) {
            swofty::log("[saves] plain store off: %s (%s); nothing is written or put back", other.c_str(), s.root.c_str());
            return c;
        }
        c.on = true;
        c.root_text = s.root;
        c.root = fs::path(runtime::wide(s.root));
        json::Value doc;
        if (json::parse(runtime::settings_json_text(), doc))
            c.profile = doc["launcher_saves"]["profile"].str();
        if (doc["addons"].type == json::Value::Arr)
            for (const json::Value &a : doc["addons"].a)
                if (a["enabled"].type == json::Value::Bool && a["enabled"].b && a["id"].type == json::Value::Str)
                    c.addons.push_back(a["id"].s);
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
    h.loaded_addons = c.addons;
    h.written_at = int64_t(time(nullptr));
    return h;
}

}

bool enabled() { return config().on; }

Plan make_plan()
{
    Plan p;
    const Config &c = config();
    if (!c.on)
        return p;
    p.on = true;
    Root root(c.root);
    p.newest = newest_core_save(root);
    Taken t = take_requests(root);
    if (!t.why.empty())
        swofty::log("[saves] inject requests not taken: %s; they wait for the next profile load", t.why.c_str());
    if (t.left_over)
        swofty::log("[saves] %s is left from a profile load that stopped while applying it; its requests are not applied "
                    "again (the launcher answers them as withdrawn)%s",
                    INJECT_APPLYING, t.taken ? "; the new inject.json replaces it" : "");
    if (t.taken) {
        p.had_inject_file = true;
        p.requests = std::move(t.requests);
        p.handled.assign(p.requests.size(), false);
        swofty::log("[saves] applying %zu inject request(s); %s holds them until injected.json has the answers, so a game "
                    "that stops before then does not apply them again",
                    p.requests.size(), INJECT_APPLYING);
    }
    return p;
}

const Entry *newest_entry(const Plan &plan, const std::string &name)
{
    if (!plan.newest)
        return nullptr;
    for (const Entry &e : plan.newest->containers)
        if (e.name == name)
            return &e;
    return nullptr;
}

std::optional<std::vector<uint8_t>> fetch_newest(const Plan &plan, const Entry &entry)
{
    if (!plan.newest)
        return std::nullopt;
    return read_container(Root(config().root), plan.newest->generation, entry);
}

int request_for(const Plan &plan, const std::string &name)
{
    for (size_t i = 0; i < plan.requests.size(); i++)
        if (plan.requests[i].container == name && !plan.handled[i])
            return int(i);
    return -1;
}

std::optional<std::vector<uint8_t>> fetch_request(const Plan &plan, int index)
{
    const Request &q = plan.requests[size_t(index)];
    Root root(config().root);
    if (auto m = read_generation(root, q.generation))
        for (const Entry &e : m->containers)
            if (e.name == q.container)
                return read_container(root, q.generation, e);
    return std::nullopt;
}

void request_done(Plan &plan, int index, bool ok, const std::string &detail)
{
    plan.handled[size_t(index)] = true;
    plan.results.push_back({plan.requests[size_t(index)].id, ok, detail});
    swofty::log("[saves] inject request %s%s: %s", plan.requests[size_t(index)].id.c_str(), ok ? "" : " refused", detail.c_str());
}

std::optional<std::vector<Part>> fetch_group(uint64_t generation, const std::string &name)
{
    Root root(config().root);
    auto m = read_generation(root, generation);
    if (!m)
        return std::nullopt;
    std::string pair = pair_of(name);
    std::vector<Part> out;
    bool found = false;
    for (const Entry &e : m->containers) {
        if (e.name != name && (pair.empty() || pair_of(e.name) != pair))
            continue;
        auto bytes = read_container(root, generation, e);
        if (!bytes)
            return std::nullopt;
        found |= e.name == name;
        out.push_back({e.name, e.sha256, e.careers, std::move(*bytes)});
    }
    if (!found)
        return std::nullopt;
    return out;
}

std::vector<Added> at_load(Plan &plan, const std::map<std::string, std::string> &registered,
                           const std::map<std::string, std::string> &kept)
{
    std::vector<Added> out;
    if (!plan.on)
        return out;
    Root root(config().root);
    std::map<std::string, std::string> tree = registered;
    tree.insert(kept.begin(), kept.end());
    std::set<std::string> added_now;
    if (plan.newest) {
        PutBack pb = put_back(*plan.newest, tree);
        for (const Skip &k : pb.skipped)
            swofty::log("[saves] did not put back %s: %s", k.name.c_str(), k.reason.c_str());
        for (const Pick &p : pb.picks) {
            auto bytes = read_container(root, p.generation, p.entry);
            if (!bytes) {
                swofty::log("[saves] could not put back %s: generation %llu has no intact copy of it", p.entry.name.c_str(),
                            (unsigned long long)p.generation);
                continue;
            }
            out.push_back({p.entry.name, std::move(*bytes), false, false, p.entry.careers, p.generation});
            added_now.insert(p.entry.name);
        }
    }
    auto loaded_sha = [&](const std::string &name) -> std::string {
        auto r = registered.find(name);
        if (r != registered.end())
            return r->second;
        auto k = kept.find(name);
        return k != kept.end() ? k->second : std::string();
    };
    std::set<std::string> changed_pairs;
    for (const Request &q : plan.requests) {
        std::string pair = pair_of(q.container);
        bool held = registered.count(q.container) || kept.count(q.container);
        if (q.replace && held && !base_matches(q, loaded_sha(q.container)))
            changed_pairs.insert(pair.empty() ? q.container : pair);
    }
    for (size_t i = 0; i < plan.requests.size(); i++) {
        if (plan.handled[i])
            continue;
        const Request &q = plan.requests[i];
        std::string pair = pair_of(q.container);
        if (q.replace && changed_pairs.count(pair.empty() ? q.container : pair)) {
            plan.handled[i] = true;
            Result r{q.id, false, changed_since_detail(q)};
            plan.results.push_back(r);
            swofty::log("[saves] inject request %s: %s", q.id.c_str(), r.detail.c_str());
            continue;
        }
        auto group = fetch_group(q.generation, q.container);
        Held held = registered.count(q.container)                               ? Held::registered
                    : (kept.count(q.container) || added_now.count(q.container)) ? Held::kept
                                                                                : Held::none;
        Decision d = decide(q, held, bool(group));
        plan.handled[i] = true;
        if (d.action != Action::none) {
            for (const Part &part : *group) {
                if (part.name == q.container)
                    continue;
                Held h = registered.count(part.name)                             ? Held::registered
                         : (kept.count(part.name) || added_now.count(part.name)) ? Held::kept
                                                                                 : Held::none;
                if (h == Held::registered && !q.replace) {
                    d = decide(Request{q.id, part.name, q.generation, false, {}}, h, true);
                    break;
                }
            }
        }
        plan.results.push_back(d.result);
        swofty::log("[saves] inject request %s%s: %s", q.id.c_str(), d.result.ok ? "" : " refused", d.result.detail.c_str());
        if (d.action == Action::none)
            continue;
        for (Part &part : *group) {
            Held h = registered.count(part.name)                             ? Held::registered
                     : (kept.count(part.name) || added_now.count(part.name)) ? Held::kept
                                                                             : Held::none;
            if (h == Held::none || q.replace || part.name == q.container) {
                out.erase(std::remove_if(out.begin(), out.end(), [&](const Added &a) { return a.name == part.name; }), out.end());
                out.push_back({part.name, std::move(part.bytes), q.replace && h != Held::none, true, 0, q.generation});
                added_now.insert(part.name);
            }
        }
    }
    if (plan.had_inject_file && !answer(root, plan.results))
        swofty::log("[saves] could not write injected.json; %s is left for the launcher to answer", INJECT_APPLYING);
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
