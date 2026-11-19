#pragma once

#include <atomic>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <string>
#include <vector>

#include "core/runtime/json.hpp"
#include "swofty_api.h"

// The addon settings' model (settings.cpp): the options read from settings.json, the commands that set them, the
// requests.json text the launcher reads back and the value an addon's read gets. test/settings_logic_test.cpp pins
// it.
namespace runtime::settings_model {

enum { SET = 1, ENABLE, RESTART };

struct State {
    json::Value value;
    std::string text, command;
};

struct Watcher {
    SwoftySettingFn fn;
    void *user;
};

// The state an option last read as, with the stamp it was read at: (stamp << 8) | state index, 0 for none.
struct Memo {
    std::atomic<uint64_t> v{0};
    Memo() = default;
    Memo(const Memo &m) : v(m.v.load(std::memory_order_relaxed)) {}
    Memo &operator=(const Memo &m)
    {
        v.store(m.v.load(std::memory_order_relaxed), std::memory_order_relaxed);
        return *this;
    }
    bool at(uint64_t stamp, size_t &state) const
    {
        uint64_t m = v.load(std::memory_order_acquire);
        state = m & 0xff;
        return stamp && m >> 8 == stamp;
    }
    void keep(uint64_t stamp, size_t state)
    {
        uint64_t want = stamp << 8 | state, cur = v.load(std::memory_order_relaxed);
        while (cur >> 8 < stamp && !v.compare_exchange_weak(cur, want, std::memory_order_release, std::memory_order_relaxed)) {
        }
    }
};

struct Option {
    std::string id;
    uint32_t type;
    std::vector<State> states;
    size_t current;
    bool requested = false;
    std::vector<Watcher> watchers;
    Memo memo;
};

struct AddonSettings {
    std::string id;
    std::vector<std::wstring> dlls;
    std::vector<std::string> enable_commands;
    std::vector<Option> options;
    int enabled = -1;
};

struct Command {
    int kind;
    size_t addon, option, state;
};

struct MfdRow {
    size_t addon, option;
};

inline constexpr const char *type_names[] = {"?", "toggle", "slider", "choice"};

inline uint32_t djb2(const std::string &s)
{
    uint32_t h = 5381;
    for (unsigned char c : s)
        h = h * 33 + c;
    return h;
}

inline std::string quote(const std::string &s)
{
    std::string out = "\"";
    for (unsigned char c : s) {
        char esc[8];
        if (c == '"' || c == '\\')
            out += '\\', out += (char)c;
        else if (c < 0x20)
            snprintf(esc, sizeof esc, "\\u%04x", c), out += esc;
        else
            out += (char)c;
    }
    return out + '"';
}

inline std::string json_of(const json::Value &v)
{
    char b[40] = "null";
    if (v.type == json::Value::Bool)
        return v.b ? "true" : "false";
    if (v.type == json::Value::Str)
        return quote(v.s);
    if (v.type == json::Value::Num && v.n == std::floor(v.n) && std::fabs(v.n) < 1e15)
        snprintf(b, sizeof b, "%lld", (long long)v.n);
    else if (v.type == json::Value::Num)
        for (int p = 1; p <= 17; p++)
            if (snprintf(b, sizeof b, "%.*g", p, v.n), strtod(b, nullptr) == v.n)
                break;
    return b;
}

inline std::string object(const std::vector<std::string> &items)
{
    std::string out = "{";
    for (size_t i = 0; i < items.size(); i++)
        out += (i ? ", " : " ") + items[i];
    return out + (items.empty() ? "}" : " }");
}

inline std::string requests_text(const std::vector<AddonSettings> &addons, const std::string &profile, unsigned long long seq,
                                 bool relaunch)
{
    std::vector<std::string> options, enabled;
    for (auto &m : addons) {
        std::vector<std::string> mine;
        for (auto &o : m.options)
            if (o.requested)
                mine.push_back(quote(o.id) + ": " + json_of(o.states[o.current].value));
        if (!mine.empty())
            options.push_back(quote(m.id) + ": " + object(mine));
        if (m.enabled >= 0)
            enabled.push_back(quote(m.id) + ": " + (m.enabled ? "true" : "false"));
    }
    return "{\n  \"schema\": 1,\n  \"profile\": " + quote(profile) + ",\n  \"seq\": " + std::to_string(seq) +
           ",\n  \"options\": " + object(options) + ",\n  \"enabled\": " + object(enabled) +
           ",\n  \"relaunch\": " + (relaunch ? "true" : "false") + "\n}\n";
}

inline SwoftySettingValue value_of(const Option &o, size_t at)
{
    const State &s = o.states[at];
    SwoftySettingValue v{o.type, 0, 0, nullptr};
    if (o.type == SWOFTY_SETTING_TOGGLE)
        v.toggle = s.value.b;
    else if (o.type == SWOFTY_SETTING_SLIDER)
        v.slider = s.value.n;
    else
        v.choice = s.value.s.c_str();
    return v;
}

inline const char *invalid(const Option &o)
{
    if (!o.type)
        return "unknown type";
    if (o.current >= o.states.size())
        return "current is not a state index";
    auto want = o.type == SWOFTY_SETTING_TOGGLE   ? json::Value::Bool
                : o.type == SWOFTY_SETTING_SLIDER ? json::Value::Num
                                                  : json::Value::Str;
    for (auto &s : o.states)
        if (s.value.type != want || (want == json::Value::Num && !std::isfinite(s.value.n)))
            return "a state value does not fit the type";
    return nullptr;
}

}
