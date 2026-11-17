#include "core/runtime/loader/bindings.hpp"

#include <algorithm>
#include <cstdio>
#include <cstring>

#include "core/runtime/loader/code_encryption.hpp"
#include "core/runtime/loader/host_api.hpp"
#include "core/runtime/loader/image.hpp"
#include "core/runtime/loader/mixins.hpp"
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

uintptr_t unresolved(const Binding &b, const char *why)
{
    api->log("[core] binding %s: %s", b.id.c_str(), why);
    return 0;
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
        api->log("[core] %s: %zu binding(s) are marked unresolvable on this build and are skipped", file_.c_str(),
                 unresolvable);
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

uintptr_t BindingSet::anchor_func(const std::string &str)
{
    auto cached = anchor_funcs_.find(str);
    if (cached != anchor_funcs_.end())
        return cached->second;
    const GameImage &img = image();
    uintptr_t sVA = img.locate_string(str), fbase = 0;
    bool conflict = false;
    long long sites = 0;
    if (sVA) {
        auto nt = img.nt_headers();
        auto s = IMAGE_FIRST_SECTION(nt);
        for (int i = 0; i < nt->FileHeader.NumberOfSections && !conflict; i++, s++) {
            DWORD ch = s->Characteristics;
            if (!(ch & IMAGE_SCN_MEM_EXECUTE) || !(ch & IMAGE_SCN_MEM_READ) || (ch & IMAGE_SCN_MEM_WRITE))
                continue;
            each_committed(img.base() + s->VirtualAddress, img.section_end(s), [&](uintptr_t p, size_t n) {
                if (n < 4)
                    return;
                for (uintptr_t q = p, last = p + n - 4; q <= last; q++) {
                    if (q + 4 + *reinterpret_cast<const int32_t *>(q) != sVA)
                        continue;
                    uintptr_t f = img.function_base_of(q);
                    if (!f)
                        continue;
                    sites++;
                    if (!fbase)
                        fbase = f;
                    else if (fbase != f)
                        conflict = true;
                }
            });
        }
    }
    uintptr_t out = (conflict || !sites) ? 0 : fbase;
    anchor_funcs_[str] = out;
    return out;
}

uintptr_t BindingSet::apply_match(const Binding &b, uintptr_t match, const char **why) const
{
    const GameImage &img = image();
    uintptr_t at = match + b.anchor, target = at, i = match + b.insn;
    if (b.rkind == "rel32" || b.rkind == "riprel") {
        if (!page_readable(std::min(at, i), (at > i ? at - i : i - at) + 16)) {
            *why = "anchor offset is not mapped on this build";
            return 0;
        }
    }
    if (b.rkind == "rel32") {
        bool branch = (byte_at(i) == 0xE8 || byte_at(i) == 0xE9) && at == i + 1;
        bool jcc = byte_at(i) == 0x0F && (byte_at(i + 1) & 0xF0) == 0x80 && at == i + 2;
        if (!branch && !jcc) {
            *why = "anchor is not the rel32 of a call or jump";
            return 0;
        }
        target = at + 4 + *reinterpret_cast<const int32_t *>(at);
    } else if (b.rkind == "riprel") {
        if (b.anchor <= b.insn || b.anchor + 4 > b.insn + b.len) {
            *why = "anchor is not inside the RIP-relative instruction";
            return 0;
        }
        target = i + b.len + *reinterpret_cast<const int32_t *>(at);
    } else if (b.rkind != "addr") {
        *why = "unknown resolve kind";
        return 0;
    }
    if (!b.expect.empty() &&
        (!page_readable(at, b.expect.size()) || memcmp(reinterpret_cast<const void *>(at), b.expect.data(), b.expect.size()))) {
        *why = "expect bytes differ";
        return 0;
    }
    bool code = b.kind == "func" || b.kind == "site" || b.kind == "call";
    auto own = img.section_of(target);
    bool own_exec_site = (b.kind == "site" || b.kind == "call") && own && (own->Characteristics & IMAGE_SCN_MEM_EXECUTE) &&
                         img.section_named(target, b.section);
    if (code ? !img.is_code(target) && !own_exec_site : !own) {
        *why = code ? "target is not in read-only code" : "target is outside the exe";
        return 0;
    }
    if (same_build_ && b.rva >= 0 && img.rva(target) != (unsigned long long)b.rva) {
        *why = "target differs from the KB build";
        return 0;
    }
    return target;
}

bool BindingSet::bytes_at(const Binding &b, uintptr_t at, bool &under) const
{
    auto pat = scancache::tokens(b.pattern);
    under = false;
    if (pat.empty() || !page_readable(at, pat.size()))
        return false;
    for (size_t k = 0; k < pat.size(); k++) {
        uintptr_t start = 0;
        auto orig = original_bytes_at(at + k, start);
        under |= orig != nullptr;
        uint8_t v = orig ? (*orig)[at + k - start] : byte_at(at + k);
        if (pat[k] >= 0 && v != pat[k])
            return false;
    }
    return true;
}

uintptr_t BindingSet::match_under_hooks(const Binding &b) const
{
    if (!same_build_ || b.rva < 0 || b.rkind != "addr")
        return 0;
    uintptr_t at = image().base() + b.rva - b.anchor;
    bool under = false;
    return bytes_at(b, at, under) && under ? at : 0;
}

uintptr_t BindingSet::kb_match(const Binding &b) const
{
    if (!kb_first_ || !same_build_ || b.match < 0 || b.pattern.empty())
        return 0;
    uintptr_t at = image().base() + b.match;
    bool under = false;
    if (!image().section_named(at, b.section) || !bytes_at(b, at, under)) {
        api->log("[core] binding %s: the pattern does not match at the KB address 0x%llx; scanning %s", b.id.c_str(),
                 (unsigned long long)at, b.section.c_str());
        return 0;
    }
    if (under && b.rkind != "addr")
        return 0;
    if (under)
        api->log("[core] binding %s: its bytes carry an installed hook; the pattern matches the bytes under it at the KB "
                 "address 0x%llx",
                 b.id.c_str(), (unsigned long long)at);
    return at;
}

bool BindingSet::guarded(const Binding &b) const
{
    auto s = code().packed() ? image().section_by_name(b.section) : nullptr;
    return s && (s->Characteristics & IMAGE_SCN_MEM_EXECUTE);
}

uintptr_t BindingSet::resolve_by_pattern(Binding &b, const char **why, bool *at_kb)
{
    Trace &t = b.tr;
    uintptr_t match = kb_match(b);
    *at_kb = match != 0;
    if (kb_first_ && same_build_ && b.match >= 0)
        t.match = t.site = b.match, t.verified = match ? 1 : 0;
    if (match) {
        t.method = "hint", t.matches = 1;
        uintptr_t target = apply_match(b, match, why);
        t.verified = target != 0;
        return target;
    }
    auto key = b.section + '|' + b.pattern;
    auto hit = scans_.find(key);
    if (hit == scans_.end()) {
        int hits = scanner().scan(b.section.c_str(), b.pattern.c_str(), &match);
        hit = scans_.emplace(key, ScanHit{match, hits}).first;
    }
    match = hit->second.at;
    t.matches = hit->second.hits;
    bool under = false;
    if (!match && (match = match_under_hooks(b))) {
        under = true;
        api->log("[core] binding %s: its bytes carry an installed hook; the pattern matches the bytes under it at the KB "
                 "address 0x%llx",
                 b.id.c_str(), (unsigned long long)match);
    }
    if (!match) {
        *why = "signature did not match exactly once";
        return 0;
    }
    t.method = under ? "hint" : "scan";
    t.match = t.site = image().rva(match);
    uintptr_t target = apply_match(b, match, why);
    t.verified = target != 0;
    return target;
}

uintptr_t BindingSet::resolve_by_anchor(Binding &b, const char **why)
{
    if (b.sa_string.empty()) {
        *why = "no string anchor";
        return 0;
    }
    uintptr_t f = anchor_func(b.sa_string);
    if (!f) {
        *why = "string anchor did not locate a unique function";
        return 0;
    }
    if (b.tr.site < 0)
        b.tr.site = image().rva(f + b.sa_func_off);
    if (same_build_ && b.sa_func_rva >= 0 && image().rva(f) != (unsigned long long)b.sa_func_rva) {
        *why = "string anchor function differs from the KB build";
        return 0;
    }
    return apply_match(b, f + b.sa_func_off, why);
}

/* One order for every build: the set's verified hint, then a section scan, then the string anchor. A set
   entry without a pattern is the old per-build format and can only use its string anchor. */
uintptr_t BindingSet::resolve(Binding &b)
{
    b.tr = {};
    if (!b.unresolvable.empty()) {
        b.tr.reason = "unresolvable: " + b.unresolvable;
        api->log("[core] binding %s: the set marks it unresolvable on this build (%s)", b.id.c_str(), b.unresolvable.c_str());
        return 0;
    }
    std::string missing;
    const char *pw = "signature did not match exactly once", *aw = "no string anchor";
    uintptr_t target = 0;
    const char *method = "";
    bool kb = false, legacy = b.pattern.empty();
    if (legacy) {
        pw = "the set gives no pattern (legacy format)";
    } else if (!image().section_by_name(b.section)) {
        missing = "section " + b.section + " is not in this exe";
        pw = missing.c_str();
    } else if ((target = resolve_by_pattern(b, &pw, &kb))) {
        method = kb ? "pattern at the KB address" : "pattern";
    }
    if (!target && !b.sa_string.empty() && (target = resolve_by_anchor(b, &aw))) {
        method = legacy ? "string anchor (legacy set entry, no pattern)" : "string anchor (pattern fell back)";
        b.tr.method = legacy ? "anchor" : "anchor-fallback";
        b.tr.match = -1, b.tr.verified = -1;
    }
    if (!target) {
        b.tr.method = "-";
        b.tr.reason = b.sa_string.empty() ? std::string(pw) : std::string("pattern: ") + pw + "; string anchor: " + aw;
        if (b.sa_string.empty())
            return unresolved(b, pw);
        api->log("[core] binding %s: unresolved (pattern: %s; string anchor: %s)", b.id.c_str(), pw, aw);
        return 0;
    }
    if (legacy)
        b.tr.reason = "legacy set entry without a pattern";
    api->log("[core] bind %s -> 0x%llx (base+0x%llx), %s %s via %s", b.id.c_str(), (unsigned long long)target,
             image().rva(target), b.kind.c_str(), b.status.c_str(), method);
    return target;
}

void BindingSet::settle(Binding &b, uintptr_t addr)
{
    b.addr = addr, b.resolved = true, b.deferred = false;
}

void BindingSet::defer(Binding &b, const std::string &why, bool final)
{
    scans_.erase(b.section + '|' + b.pattern);
    for (auto it = anchor_funcs_.begin(); it != anchor_funcs_.end();)
        it = it->second ? std::next(it) : anchor_funcs_.erase(it);
    if (final) {
        b.tr.method = "-";
        b.tr.reason = why + "; still so at the last retry, so it fails closed";
        if (b.tr.site < 0 && b.match >= 0)
            b.tr.site = b.match;
        api->log("[core] binding %s: %s", b.id.c_str(), b.tr.reason.c_str());
        return settle(b, 0);
    }
    b.tr = {};
    b.tr.reason = why;
    b.addr = 0, b.resolved = false, b.deferred = true;
}

/* A packed build's code reads encrypted until the game decrypts it, so its bindings wait for that instead of
   resolving to garbage: the section must read decrypted, and a resolved site's own page too. A pattern that
   fails while some of the section still reads encrypted is retried later, not given up. */
void BindingSet::attempt(Binding &b, bool final)
{
    if (!guarded(b) || !b.unresolvable.empty())
        return settle(b, resolve(b));
    char why[200];
    double e = code().section_now(b.section);
    if (e < 0 || e > CodeEncryption::ENCRYPTED) {
        snprintf(why, sizeof why, "code section %s reads %s (entropy %.2f)", b.section.c_str(), CodeEncryption::reads(e),
                 e < 0 ? 0 : e);
        b.tr = {};
        return defer(b, why, final);
    }
    uintptr_t target = resolve(b);
    uintptr_t base = image().base();
    if (target) {
        uintptr_t m = b.tr.match >= 0 ? base + b.tr.match : 0;
        uintptr_t bad = m && code().page_encrypted(m)                                   ? m
                        : image().in_exec(target) && code().page_encrypted(target) ? target
                                                                                       : 0;
        if (!bad)
            return settle(b, target);
        snprintf(why, sizeof why, "the page at base+0x%llx still reads encrypted", image().rva(bad));
        return defer(b, why, final);
    }
    bool partly = e > CodeEncryption::PLAIN || (b.tr.site >= 0 && code().page_encrypted(base + b.tr.site));
    if (!partly)
        return settle(b, 0);
    snprintf(why, sizeof why, "code section %s reads %s (entropy %.2f): ", b.section.c_str(), CodeEncryption::reads(e), e);
    defer(b, why + b.tr.reason, final);
}

void BindingSet::attempt_unresolved()
{
    for (auto &b : bindings_)
        if (!b.resolved && !b.deferred)
            attempt(b, false);
}

void BindingSet::retry_deferred(bool final)
{
    for (auto &b : bindings_)
        if (b.deferred)
            attempt(b, final);
}

std::string BindingSet::sample_deferred_sections()
{
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
                           " match=" + (b.deferred ? "-" : hex_rva(t.match)) +
                           " verified=" + (b.deferred || t.verified < 0 ? "-" : t.verified ? "yes" : "no") + " reason=\"" +
                           quoted(t.reason) + "\"";
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
