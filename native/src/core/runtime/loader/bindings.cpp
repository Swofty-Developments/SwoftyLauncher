#include "core/runtime/loader/bindings.hpp"

#include <algorithm>
#include <cstdio>
#include <cstring>

#include "core/runtime/loader/code_encryption.hpp"
#include "core/runtime/loader/host_api.hpp"
#include "core/runtime/loader/image.hpp"
#include "core/runtime/loader/mixins.hpp"
#include "core/runtime/loader/resolve_locks.hpp"
#include "core/runtime/loader/scanner.hpp"
#include "core/runtime/scan_cache.hpp"

namespace runtime::loader {

namespace {

std::string utf8(const wchar_t *w)
{
    char buf[MAX_PATH * 3] = {};
    WideCharToMultiByte(CP_UTF8, 0, w, -1, buf, sizeof buf - 1, nullptr, nullptr);
    return buf;
}

std::string select_bindings(const std::wstring &dir, long long stamp, std::string &chosen)
{
    WIN32_FIND_DATAW fd;
    HANDLE h = FindFirstFileW((dir + L"bindings*.json").c_str(), &fd);
    std::string match_text, default_text, first_text;
    std::wstring match_name, first_name;
    int candidates = 0;
    if (h != INVALID_HANDLE_VALUE) {
        do {
            if (fd.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY)
                continue;
            std::string text = read_whole_file(dir + fd.cFileName);
            json::Value d;
            if (!json::parse(text, d) || d["build"].type == json::Value::Null)
                continue;
            candidates++;
            if (first_text.empty())
                first_text = text, first_name = fd.cFileName;
            if (!_wcsicmp(fd.cFileName, L"bindings.json"))
                default_text = text;
            if (match_text.empty() && (long long)d["build"]["pe_timestamp"].num() == stamp)
                match_text = text, match_name = fd.cFileName;
        } while (FindNextFileW(h, &fd));
        FindClose(h);
    }
    if (!match_text.empty()) {
        chosen = utf8(match_name.c_str());
        api->log("[core] bindings: %d set(s) present, selected %s by PE timestamp %lld", candidates, chosen.c_str(), stamp);
        return match_text;
    }
    if (!default_text.empty()) {
        chosen = "bindings.json";
        api->log("[core] bindings: %d set(s) present, none match PE timestamp %lld; using bindings.json", candidates, stamp);
        return default_text;
    }
    chosen = first_name.empty() ? "bindings.json" : utf8(first_name.c_str());
    if (candidates)
        api->log("[core] bindings: %d set(s) present, none match PE timestamp %lld and no bindings.json; using %s", candidates,
                 stamp, chosen.c_str());
    return first_text;
}

std::vector<uint8_t> hex_bytes(const std::string &s)
{
    std::vector<uint8_t> out;
    for (const char *p = s.c_str(); *p;) {
        char *end;
        long v = strtol(p, &end, 16);
        if (end == p)
            break;
        out.push_back((uint8_t)v);
        p = end;
    }
    return out;
}

std::string quoted(const std::string &s)
{
    std::string out;
    for (char c : s) {
        if (c == '"' || c == '\\')
            out += '\\';
        out += c == '\r' || c == '\n' ? ' ' : c;
    }
    return out;
}

std::string hex_rva(long long v)
{
    char buf[24];
    snprintf(buf, sizeof buf, "0x%llx", v);
    return v < 0 ? "-" : buf;
}

}

BindingSet &bindings()
{
    static BindingSet instance;
    return instance;
}

json::Value BindingSet::load(const std::wstring &dir, long long running_stamp)
{
    std::string chosen = "bindings.json";
    json::Value doc;
    if (!json::parse(select_bindings(dir, running_stamp, chosen), doc))
        api->log("[core] %ls%s missing or unreadable; no binding will resolve", dir.c_str(), chosen.c_str());
    file_ = chosen;
    for (auto &j : doc["bindings"].a) {
        Binding b{j["id"].str(),
                  j["kind"].str(),
                  j["status"].str(),
                  j["section"].str(),
                  j["pattern"].str(),
                  j["resolve"]["kind"].str("addr"),
                  hex_bytes(j["expect"].str()),
                  j["anchor"].num(),
                  j["resolve"]["insn"].num(),
                  j["resolve"]["len"].num(),
                  j["rva"].num(-1),
                  j["slots"].num(),
                  j["match"].num(-1)};
        const auto &sa = j["string_anchor"];
        if (sa.type == json::Value::Obj) {
            b.sa_string = sa["string"].str();
            b.sa_func_rva = sa["func_rva"].num(-1);
            b.sa_func_off = sa["func_offset"].num();
            b.sa_durable = sa["durable"].b;
        }
        if (b.status == "unresolvable")
            b.unresolvable = j["reason"].str("the set gives no reason");
        index_[b.id] = bindings_.size();
        bindings_.push_back(std::move(b));
    }
    policy_ = BindingPolicy::read(doc);
    same_build_ = doc["build"]["pe_timestamp"].num() == running_stamp;
    std::string dist = doc["build"]["distribution"].str();
    for (auto &c : dist)
        c = char(tolower((unsigned char)c));
    build.tag = dist == "steam" ? "steam" : dist == "ea app" || dist == "ea" ? "ea" : "unknown";
    return doc;
}

std::unordered_map<std::string, size_t> BindingSet::section_uses() const
{
    std::unordered_map<std::string, size_t> uses;
    for (auto &b : bindings_)
        uses[b.section]++;
    return uses;
}

std::vector<std::pair<uint32_t, std::string>> BindingSet::resolved_functions() const
{
    std::vector<std::pair<uint32_t, std::string>> out;
    for (auto &b : bindings_)
        if (b.resolved && b.addr && b.kind == "func")
            out.emplace_back(uint32_t(image().rva(b.addr)), b.id);
    std::sort(out.begin(), out.end());
    return out;
}

void BindingSet::log_loaded(const json::Value &doc) const
{
    size_t anchored = 0, durable = 0, legacy = 0, unresolvable = 0;
    for (auto &b : bindings_) {
        if (!b.sa_string.empty())
            anchored++, durable += b.sa_durable;
        legacy += b.pattern.empty();
        unresolvable += !b.unresolvable.empty();
    }
    api->log("[core] %s: %zu binding(s) for build %.16s, %s; %zu string-anchored (%zu build-agnostic), pattern first",
             file_.c_str(), bindings_.size(), doc["build"]["sha256"].str("?").c_str(),
             same_build_ ? "this exe (rva checked)" : "a different exe (patterns are scanned)", anchored, durable);
    if (legacy)
        api->log("[core] %s: %zu binding(s) carry no pattern (legacy set format); they resolve by string anchor only",
                 file_.c_str(), legacy);
    if (unresolvable)
        api->log("[core] %s: %zu binding(s) are marked unresolvable on this build and are skipped", file_.c_str(), unresolvable);
    if (std::string counts = policy_.load_line(file_); !counts.empty())
        api->log("%s", counts.c_str());
    for (auto &m : policy_.malformed())
        api->log("[core] %s: %s", file_.c_str(), m.c_str());
}

void BindingSet::plan_kb_addresses(long long kb_size, long long exe_size)
{
    size_t placed = 0;
    for (auto &b : bindings_)
        placed += b.match >= 0 && !b.pattern.empty();
    if (kb_first_ && same_build_ && kb_size > 0 && exe_size != kb_size) {
        kb_first_ = false;
        api->log("[core] binding resolution: the exe is %lld bytes and the KB build %lld; the KB addresses are not used, "
                 "every pattern is scanned",
                 exe_size, kb_size);
    }
    if (kb_first_ && same_build_)
        api->log("[core] binding resolution: %zu of %zu pattern(s) carry their KB address; each is checked there first and "
                 "scanned only when the bytes differ",
                 placed, bindings_.size());
}

Binding *BindingSet::find(const std::string &id)
{
    auto it = index_.find(id);
    return it == index_.end() ? nullptr : &bindings_[it->second];
}

bool BindingSet::note_use(const std::string &id)
{
    std::lock_guard<std::recursive_mutex> g(resolve_lock());
    if (std::string line = policy_.use(id); !line.empty())
        api->log("%s", line.c_str());
    return policy_.dropped(id);
}

std::string BindingSet::sample_deferred_sections()
{
    ImageProtectionPass pass;
    std::vector<std::string> seen;
    std::string sampled;
    for (auto &b : bindings_)
        if (b.deferred && std::find(seen.begin(), seen.end(), b.section) == seen.end()) {
            seen.push_back(b.section);
            double e = code().section_now(b.section);
            char one[96];
            snprintf(one, sizeof one, "%s%s entropy %.2f (%s)", sampled.empty() ? "" : ", ", b.section.c_str(), e < 0 ? 0 : e,
                     CodeEncryption::reads(e));
            sampled += one;
        }
    return sampled;
}

size_t BindingSet::deferred_count() const
{
    size_t n = 0;
    for (auto &b : bindings_)
        n += b.deferred;
    return n;
}

std::string BindingSet::site_bytes(long long site) const
{
    std::string out;
    for (int i = 0; i < 32; i++) {
        uintptr_t a = image().base() + site + i, start = 0;
        if (!page_readable(a, 1))
            break;
        auto orig = original_bytes_at(a, start);
        char h[4];
        snprintf(h, sizeof h, "%s%02X", i ? " " : "", orig ? (*orig)[a - start] : byte_at(a));
        out += h;
    }
    return out;
}

void BindingSet::report(const char *pass)
{
    ImageProtectionPass image_pass;
    unreported_ = false;
    size_t done = 0, waiting = 0, failed = 0;
    for (auto &b : bindings_) {
        const Trace &t = b.tr;
        bool ok = b.resolved && b.addr;
        const char *result = b.deferred ? "deferred" : ok ? "resolved" : "unresolved";
        (b.deferred ? waiting : ok ? done : failed)++;
        std::string line = "[bind] id=" + b.id + " build=" + build.tag + " pass=" + pass + " result=" + result +
                           " method=" + (ok ? t.method : "-") +
                           " matches=" + (b.deferred || t.matches < 0 ? "-" : std::to_string(t.matches)) +
                           " rva=" + (ok ? hex_rva((long long)image().rva(b.addr)) : "-") +
                           " match=" + (b.deferred ? "-" : hex_rva(t.match)) + " verified=" +
                           (b.deferred || t.verified < 0 ? "-"
                            : t.verified                 ? "yes"
                                                         : "no") +
                           " reason=\"" + quoted(t.reason) + "\"";
        if (!ok && !b.deferred)
            line += " site=" + hex_rva(t.site) + " site_bytes=\"" + (t.site >= 0 ? site_bytes(t.site) : "") + "\"";
        else
            line += " site_bytes=\"\"";
        api->log("%s", line.c_str());
    }
    api->log("[bind] summary build=%s pass=%s resolved=%zu deferred=%zu unresolved=%zu total=%zu pe_timestamp=%lld "
             "size=%lld sha256=%s",
             build.tag.c_str(), pass, done, waiting, failed, bindings_.size(), build.stamp, build.bytes, build.sha.c_str());
}

}
