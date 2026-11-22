#pragma once

#include "core/runtime/json.hpp"

#include <cstddef>
#include <string>
#include <unordered_map>
#include <unordered_set>
#include <utility>
#include <vector>

// The binding set's drop policy: a bindings.json entry's "deprecated": {since, reason, replacement?} and the set's
// top-level "dropped": [{id, since, reason, replacement?}]. A deprecated id still resolves and a dropped one resolves
// to 0; the first use of either logs one [bind] line. since and replacement are printable ASCII tokens without a space,
// '"' or '\'; reason is printable ASCII, at most 300 characters, without '"' or '\'. A malformed entry is left out with
// a load line naming what is wrong.
namespace runtime::loader {

class BindingPolicy {
public:
    static constexpr size_t MAX_REASON = 300, MAX_TOKEN = 128;

    struct Notice {
        std::string since, reason, replacement;
    };

    // "" when reason is printable ASCII, 1 to MAX_REASON characters, with no '"' or '\'; else what is wrong with it.
    static std::string reason_fault(const std::string &reason)
    {
        if (reason.empty())
            return "is empty";
        if (reason.size() > MAX_REASON)
            return "is longer than 300 characters";
        for (unsigned char c : reason) {
            if (c == '"' || c == '\\')
                return "has a '\"' or '\\'";
            if (c < 0x20 || c > 0x7e)
                return "has a character outside printable ASCII";
        }
        return {};
    }

    static bool token(const std::string &s)
    {
        if (s.empty() || s.size() > MAX_TOKEN)
            return false;
        for (unsigned char c : s)
            if (c <= 0x20 || c > 0x7e || c == '"' || c == '\\')
                return false;
        return true;
    }

    // Reads {since, reason, replacement?}; "" when it is well formed, else what is wrong with it.
    static std::string read_notice(const json::Value &v, Notice &out)
    {
        if (v.type != json::Value::Obj)
            return "it is not an object";
        const json::Value &since = v["since"], &reason = v["reason"], &replacement = v["replacement"];
        if (since.type != json::Value::Str || !token(since.s))
            return "since is missing or not a printable token";
        if (reason.type != json::Value::Str)
            return "reason is missing";
        if (std::string fault = reason_fault(reason.s); !fault.empty())
            return "reason " + fault;
        if (replacement.type != json::Value::Null && (replacement.type != json::Value::Str || !token(replacement.s)))
            return "replacement is not a printable token";
        out = {since.s, reason.s, replacement.type == json::Value::Str ? replacement.s : std::string()};
        return {};
    }

    // Reads every binding's deprecated object and the dropped list of a parsed bindings.json.
    static BindingPolicy read(const json::Value &doc)
    {
        BindingPolicy p;
        std::unordered_set<std::string> listed;
        for (const json::Value &j : doc["bindings"].a) {
            std::string id = j["id"].str();
            listed.insert(id);
            const json::Value &d = j["deprecated"];
            if (d.type == json::Value::Null)
                continue;
            Notice n;
            std::string fault = read_notice(d, n);
            if (!token(id))
                fault = "its id is not a printable token";
            if (fault.empty())
                p.deprecated_[id] = std::move(n);
            else
                p.malformed_.push_back((token(id) ? id : "a binding") + ": malformed deprecated entry (" + fault +
                                       "); it resolves with no notice");
        }
        const json::Value &dropped = doc["dropped"];
        if (dropped.type != json::Value::Null && dropped.type != json::Value::Arr)
            p.malformed_.push_back("dropped is not an array; no binding is dropped");
        size_t at = 0;
        for (const json::Value &j : dropped.a) {
            std::string where = "dropped entry " + std::to_string(at++);
            const json::Value &id = j["id"];
            if (j.type != json::Value::Obj || id.type != json::Value::Str || !token(id.s)) {
                p.malformed_.push_back(where + ": malformed (no id that is a printable token); left out");
                continue;
            }
            Notice n;
            std::string fault = read_notice(j, n);
            if (fault.empty() && listed.count(id.s))
                fault = "the set still lists it";
            if (fault.empty() && p.dropped_.count(id.s))
                fault = "an earlier entry drops it";
            if (fault.empty())
                p.dropped_[id.s] = std::move(n);
            else
                p.malformed_.push_back(where + " (" + id.s + "): malformed (" + fault + "); left out");
        }
        return p;
    }

    size_t deprecated_count() const { return deprecated_.size(); }
    size_t dropped_count() const { return dropped_.size(); }
    bool dropped(const std::string &id) const { return dropped_.count(id) != 0; }
    // One clause per malformed entry, for "[core] <file>: <clause>".
    const std::vector<std::string> &malformed() const { return malformed_; }

    // "[core] <file>: <n> binding(s) deprecated, <m> dropped", or "" when the set has neither.
    std::string load_line(const std::string &file) const
    {
        if (deprecated_.empty() && dropped_.empty())
            return {};
        return "[core] " + file + ": " + std::to_string(deprecated_.size()) + " binding(s) deprecated, " +
               std::to_string(dropped_.size()) + " dropped";
    }

    // The [bind] line for the first use of a deprecated or dropped id; "" for any other id and for a later use.
    std::string use(const std::string &id)
    {
        const char *what = "deprecated";
        auto n = deprecated_.find(id);
        if (n == deprecated_.end()) {
            what = "dropped";
            n = dropped_.find(id);
            if (n == dropped_.end())
                return {};
        }
        if (!told_.insert(id).second)
            return {};
        const Notice &d = n->second;
        return "[bind] id=" + id + " " + what + " since=" + d.since +
               " replacement=" + (d.replacement.empty() ? "-" : d.replacement) + " reason=\"" + d.reason + "\"";
    }

private:
    std::unordered_map<std::string, Notice> deprecated_, dropped_;
    std::unordered_set<std::string> told_;
    std::vector<std::string> malformed_;
};

}
