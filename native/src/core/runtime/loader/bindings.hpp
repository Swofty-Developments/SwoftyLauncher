#pragma once

#include "core/runtime/json.hpp"
#include "core/runtime/loader/binding_policy.hpp"

#include <cstdint>
#include <string>
#include <unordered_map>
#include <utility>
#include <vector>

namespace runtime::loader {

struct Trace {
    const char *method = "-";
    int matches = -1, verified = -1;
    long long match = -1, site = -1;
    std::string reason;
};

struct Binding {
    std::string id, kind, status, section, pattern, rkind;
    std::vector<uint8_t> expect;
    long long anchor = 0, insn = 0, len = 0, rva = -1, slots = 0, match = -1;
    bool resolved = false;
    uintptr_t addr = 0;
    std::string sa_string = {};
    long long sa_func_rva = -1, sa_func_off = 0;
    bool sa_durable = false;
    std::string unresolvable = {};
    bool deferred = false;
    Trace tr = {};
};

// The running exe as the binding set and the file system name it, for the [bind] report lines.
struct BuildIdentity {
    std::string tag = "unknown";
    long long stamp = 0, bytes = -1;
    std::string sha = "-";
};

// The binding set chosen for the running exe (bindings*.json) and the resolution of each binding: the set's verified
// hint, then a section scan, then the string anchor, with a packed build's bindings deferred until their code decrypts.
class BindingSet {
public:
    // Reads the set whose build matches the running exe; returns the parsed document for the caller's build checks.
    json::Value load(const std::wstring &dir, long long running_stamp);
    void log_loaded(const json::Value &doc) const;
    void plan_kb_addresses(long long kb_size, long long exe_size);

    Binding *find(const std::string &id);
    // Logs the drop policy's [bind] line on the first use of a deprecated or dropped id; true when the set drops it.
    bool note_use(const std::string &id);
    void attempt(Binding &b, bool final);
    void attempt_unresolved();
    void retry_deferred(bool final);
    std::string sample_deferred_sections();
    size_t deferred_count() const;
    void report(const char *pass);

    const std::string &file() const { return file_; }
    bool same_build() const { return same_build_; }
    bool unreported() const { return unreported_; }
    void note_reported_change() { unreported_ = true; }
    bool kb_first() const { return kb_first_; }
    void use_kb_addresses(bool on) { kb_first_ = on; }
    std::unordered_map<std::string, size_t> section_uses() const;
    // Every resolved function binding as (rva, id), sorted by rva.
    std::vector<std::pair<uint32_t, std::string>> resolved_functions() const;
    bool empty() const { return bindings_.empty(); }

    BuildIdentity build;

private:
    struct ScanHit {
        uintptr_t at;
        int hits;
    };

    uintptr_t anchor_func(const std::string &str);
    uintptr_t apply_match(const Binding &b, uintptr_t match, const char **why) const;
    bool bytes_at(const Binding &b, uintptr_t at, bool &under) const;
    uintptr_t match_under_hooks(const Binding &b) const;
    uintptr_t kb_match(const Binding &b) const;
    bool guarded(const Binding &b) const;
    uintptr_t resolve_by_pattern(Binding &b, const char **why, bool *at_kb);
    uintptr_t resolve_by_anchor(Binding &b, const char **why);
    uintptr_t resolve(Binding &b);
    void prescan(bool deferred);
    void settle(Binding &b, uintptr_t addr);
    void defer(Binding &b, const std::string &why, bool final);
    std::string site_bytes(long long site) const;

    std::vector<Binding> bindings_;
    std::unordered_map<std::string, size_t> index_;
    BindingPolicy policy_;
    std::string file_ = "bindings.json";
    std::unordered_map<std::string, ScanHit> scans_;
    std::unordered_map<std::string, uintptr_t> anchor_funcs_;
    bool kb_first_ = true;
    bool same_build_ = false;
    bool unreported_ = false;
};

BindingSet &bindings();

}
