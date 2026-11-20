#include "core/runtime/loader/scanner.hpp"

#include <bcrypt.h>

#include <algorithm>
#include <cstdio>
#include <cstring>
#include <mutex>
#include <vector>

#include "core/diagnostics/steps.hpp"
#include "core/runtime/cache_dir.hpp"
#include "core/runtime/lifecycle.hpp"
#include "core/runtime/loader/code_encryption.hpp"
#include "core/runtime/loader/host_api.hpp"
#include "core/runtime/loader/mixins.hpp"
#include "core/runtime/loader/resolve_locks.hpp"
#include "loader/data_folder.h"

namespace runtime::loader {

namespace {

constexpr size_t PRESCAN_MIN = 16;

}

Scanner &scanner()
{
    static Scanner instance;
    return instance;
}

std::string read_whole_file(const std::wstring &path)
{
    std::string out;
    if (FILE *f = _wfopen(path.c_str(), L"rb")) {
        char buf[65536];
        for (size_t n; (n = fread(buf, 1, sizeof buf, f));)
            out.append(buf, n);
        fclose(f);
    }
    return out;
}

std::string Scanner::exe_sha256(const std::wstring &exe, const std::wstring &cache, long long exe_bytes, long long exe_stamp)
{
    WIN32_FILE_ATTRIBUTE_DATA fa{};
    if (!GetFileAttributesExW(exe.c_str(), GetFileExInfoStandard, &fa))
        return "-";
    char key[96];
    snprintf(key, sizeof key, "%lld\t%llu\t%lld\t", exe_bytes,
             ((unsigned long long)fa.ftLastWriteTime.dwHighDateTime << 32) | fa.ftLastWriteTime.dwLowDateTime, exe_stamp);
    std::string kept = read_whole_file(cache);
    if (!kept.compare(0, strlen(key), key) && kept.size() >= strlen(key) + 64)
        return kept.substr(strlen(key), 64);
    HMODULE bc = LoadLibraryW(L"bcrypt.dll");
    auto open = reinterpret_cast<decltype(&BCryptOpenAlgorithmProvider)>(GetProcAddress(bc, "BCryptOpenAlgorithmProvider"));
    auto create = reinterpret_cast<decltype(&BCryptCreateHash)>(GetProcAddress(bc, "BCryptCreateHash"));
    auto data = reinterpret_cast<decltype(&BCryptHashData)>(GetProcAddress(bc, "BCryptHashData"));
    auto finish = reinterpret_cast<decltype(&BCryptFinishHash)>(GetProcAddress(bc, "BCryptFinishHash"));
    auto destroy = reinterpret_cast<decltype(&BCryptDestroyHash)>(GetProcAddress(bc, "BCryptDestroyHash"));
    auto close = reinterpret_cast<decltype(&BCryptCloseAlgorithmProvider)>(GetProcAddress(bc, "BCryptCloseAlgorithmProvider"));
    BCRYPT_ALG_HANDLE alg = nullptr;
    BCRYPT_HASH_HANDLE h = nullptr;
    if (!open || !create || !data || !finish || !destroy || !close || open(&alg, BCRYPT_SHA256_ALGORITHM, nullptr, 0))
        return "-";
    HANDLE f = CreateFileW(exe.c_str(), GENERIC_READ, FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE, nullptr,
                           OPEN_EXISTING, FILE_FLAG_SEQUENTIAL_SCAN, nullptr);
    bool ok = f != INVALID_HANDLE_VALUE && !create(alg, &h, nullptr, 0, nullptr, 0, 0);
    std::vector<uint8_t> buf(size_t(4) << 20);
    for (DWORD got = 0; ok && ReadFile(f, buf.data(), DWORD(buf.size()), &got, nullptr) && got;)
        ok = !data(h, buf.data(), got, 0);
    uint8_t digest[32];
    ok = ok && !finish(h, digest, sizeof digest, 0);
    if (h)
        destroy(h);
    close(alg, 0);
    if (f != INVALID_HANDLE_VALUE)
        CloseHandle(f);
    if (!ok)
        return "-";
    std::string hex;
    for (uint8_t c : digest) {
        char x[3];
        snprintf(x, sizeof x, "%02x", c);
        hex += x;
    }
    if (FILE *o = _wfopen(cache.c_str(), L"wb")) {
        fprintf(o, "%s%s\n", key, hex.c_str());
        fclose(o);
    }
    return hex;
}

int Scanner::scan_now(const char *section, const char *pattern, uintptr_t *out)
{
    if (section && pattern) {
        std::string key = std::string(section) + '|' + pattern;
        AcquireSRWLockExclusive(&lock_);
        auto it = prescanned_.find(key);
        bool have = it != prescanned_.end();
        Prescanned p = have ? it->second : Prescanned{};
        if (have)
            prescanned_.erase(it);
        ReleaseSRWLockExclusive(&lock_);
        if (have) {
            if (p.hits == 1)
                api->log("scan %s [%s] -> 1 hit at 0x%llx (base+0x%llx)", section, pattern, (unsigned long long)p.first,
                         (unsigned long long)(p.first - image().base()));
            else if (p.hits)
                api->log("scan %s [%s] -> %d hits (first at 0x%llx), rejected: need exactly 1", section, pattern, p.hits,
                         (unsigned long long)p.first);
            else
                api->log("scan %s [%s] -> 0 hits, rejected: need exactly 1", section, pattern);
            if (out)
                *out = p.hits == 1 ? p.first : 0;
            return p.hits;
        }
    }
    return loader_scan_(section, pattern, out);
}

/* A cache miss after a game update would scan the whole section once per binding (thousands of passes); the
   loader's SwoftyLoaderScanMany gives every pattern's hits and first hit from one pass, the same answers its scan
   gives, and nothing writes the game code while a binding pass runs. */
void Scanner::prescan(const std::string &section, const std::vector<std::string> &patterns)
{
    if (!loader_scan_many_)
        return;
    std::vector<const char *> todo;
    {
        std::unordered_map<std::string, bool> seen;
        AcquireSRWLockShared(&lock_);
        for (auto &p : patterns) {
            uint64_t known = 0;
            if ((cache_on_ && cache_.find(section, p, known)) || prescanned_.count(section + '|' + p) ||
                !seen.emplace(p, true).second)
                continue;
            todo.push_back(p.c_str());
        }
        ReleaseSRWLockShared(&lock_);
    }
    if (todo.size() < PRESCAN_MIN)
        return;
    perf::Stopwatch took;
    std::vector<int> hits(todo.size());
    std::vector<uintptr_t> first(todo.size());
    if (!loader_scan_many_(section.c_str(), todo.data(), int(todo.size()), hits.data(), first.data())) {
        api->log("[core] scan: one pass over %s for %zu pattern(s) was not possible; each is scanned on its own", section.c_str(),
                 todo.size());
        return;
    }
    size_t kept = 0;
    AcquireSRWLockExclusive(&lock_);
    for (size_t i = 0; i < todo.size(); i++)
        if (hits[i] >= 0)
            prescanned_[section + '|' + todo[i]] = {hits[i], first[i]}, kept++;
    ReleaseSRWLockExclusive(&lock_);
    double ms = took.total_ms();
    api->log("[core] scan: one pass over %s answered %zu pattern(s) the scan cache does not know in %.1f ms", section.c_str(),
             kept, ms);
    api->log(perf::STEP_LINE, ms, ("scan one pass " + section).c_str());
}

void Scanner::end_prescan()
{
    AcquireSRWLockExclusive(&lock_);
    prescanned_.clear();
    ReleaseSRWLockExclusive(&lock_);
}

int Scanner::scan(const char *section, const char *pattern, uintptr_t *out)
{
    if (out)
        *out = 0;
    if (!cache_on_ || !section || !pattern)
        return scan_now(section, pattern, out);
    uint64_t known = 0;
    AcquireSRWLockShared(&lock_);
    bool have = cache_.find(section, pattern, known);
    ReleaseSRWLockShared(&lock_);
    if (have) {
        auto pat = scancache::tokens(pattern);
        uintptr_t at = image().base() + known;
        if (image().section_named(at, section) && page_readable(at, pat.size()) &&
            scancache::matches(reinterpret_cast<const uint8_t *>(at), pat)) {
            api->log("scan %s [%s] -> 1 hit at 0x%llx (base+0x%llx), the address cached for this exe", section, pattern,
                     (unsigned long long)at, (unsigned long long)known);
            if (out)
                *out = at;
            return 1;
        }
    }
    uintptr_t at = 0;
    int hits = scan_now(section, pattern, &at);
    AcquireSRWLockExclusive(&lock_);
    cache_.found(section, pattern, hits, hits == 1 ? image().rva(at) : 0);
    ReleaseSRWLockExclusive(&lock_);
    if (out && hits == 1)
        *out = at;
    return hits;
}

bool Scanner::recall(const char *section, const char *pattern, uint64_t &rva)
{
    AcquireSRWLockShared(&lock_);
    bool have = cache_on_ && cache_.find(section, pattern, rva);
    ReleaseSRWLockShared(&lock_);
    return have;
}

void Scanner::remember(const char *section, const char *pattern, int hits, uint64_t rva)
{
    AcquireSRWLockExclusive(&lock_);
    if (cache_on_)
        cache_.found(section, pattern, hits, rva);
    ReleaseSRWLockExclusive(&lock_);
}

void Scanner::read_section_map(const json::Value &doc, const std::string &file,
                               const std::unordered_map<std::string, size_t> &uses, bool any_binding)
{
    const auto &given = doc["sections"].type == json::Value::Obj ? doc["sections"] : doc["build"]["sections"];
    for (auto &[from, to] : given.o)
        if (to.type == json::Value::Str && from != to.s && image().section_by_name(to.s))
            section_alias_[from] = to.s;
    const char *source = "the set's section map";
    auto top = std::max_element(uses.begin(), uses.end(), [](auto &x, auto &y) { return x.second < y.second; });
    if (section_alias_.empty() && any_binding && !uses.count(".xtls") && top != uses.end()) {
        auto s = image().section_by_name(top->first);
        if (s && (s->Characteristics & IMAGE_SCN_MEM_EXECUTE))
            section_alias_[".xtls"] = top->first, source = "inferred: no binding of the set scans .xtls";
    }
    for (auto &[from, to] : section_alias_)
        api->log("[core] %s: scans of %s by core code and addons go to %s on this build (%s)", file.c_str(), from.c_str(),
                 to.c_str(), source);
}

/* A scan of encrypted bytes matches garbage or nothing, so on a packed build it is refused while its section
   reads encrypted, and a hit on a page that still does is dropped. An addon init that hit one is called again
   once the code decrypts, as for a deferred binding. */
bool Scanner::refused(const char *section, const char *pattern, uintptr_t hit)
{
    char why[160];
    if (!hit) {
        auto s = section ? image().section_by_name(section) : image().code_section();
        if (!s || !(s->Characteristics & IMAGE_SCN_MEM_EXECUTE))
            return false;
        double e = code().section_now(GameImage::section_name(s));
        if (e >= 0 && e <= CodeEncryption::ENCRYPTED)
            return false;
        snprintf(why, sizeof why, "%s reads %s (entropy %.2f)", GameImage::section_name(s).c_str(), CodeEncryption::reads(e),
                 e < 0 ? 0 : e);
    } else {
        if (!image().in_exec(hit) || !code().page_encrypted(hit))
            return false;
        snprintf(why, sizeof why, "its hit at base+0x%llx is on a page that still reads encrypted", image().rva(hit));
    }
    Addon *current = current_addon();
    if (current)
        current->hit_deferred = true;
    api->log("[core] scan %s [%s]%s%s refused: %s on this packed build; nothing resolves in encrypted code",
             section ? section : "<exec>", pattern, current ? " for " : "", current ? current->name.c_str() : "", why);
    return true;
}

/* Core code and addons written for Steam scan Steam's section names; the set's section map sends them to this
   build's section. Bindings already name their own build's section and scan without the map. */
int Scanner::addon_scan(const char *section, const char *pattern, uintptr_t *out)
{
    auto it = section ? section_alias_.find(section) : section_alias_.end();
    const char *real = it == section_alias_.end() ? section : it->second.c_str();
    if (!code().packed())
        return scan(real, pattern, out);
    std::lock_guard<std::recursive_mutex> g(resolve_lock());
    if (out)
        *out = 0;
    uintptr_t at = 0;
    if (refused(real, pattern, 0))
        return 0;
    int hits = scan(real, pattern, &at);
    if (hits == 1 && refused(real, pattern, at))
        return 0;
    if (out)
        *out = at;
    return hits;
}

const std::wstring &Scanner::caches(const std::wstring &dir)
{
    if (!cache_dir_.empty())
        return cache_dir_;
    cache_dir_ = dir + L"..\\..\\cache\\";
    CreateDirectoryW(cache_dir_.c_str(), nullptr);
    HANDLE probe = CreateFileW((cache_dir_ + L"write-test.tmp").c_str(), GENERIC_WRITE, 0, nullptr, CREATE_ALWAYS,
                               FILE_ATTRIBUTE_TEMPORARY | FILE_FLAG_DELETE_ON_CLOSE, nullptr);
    if (probe != INVALID_HANDLE_VALUE) {
        CloseHandle(probe);
        return cache_dir_;
    }
    DWORD error = GetLastError();
    wchar_t folder[MAX_PATH * 2];
    if (!swofty_data_folder(folder, MAX_PATH * 2)) {
        api->log("[core] caches: the game can't create files in swofty\\cache (error %lu) and LOCALAPPDATA is not set; "
                 "the scan and exe hash caches are not kept",
                 error);
        return cache_dir_;
    }
    std::wstring data = folder;
    CreateDirectoryW(data.c_str(), nullptr);
    CreateDirectoryW((data += L"\\cache").c_str(), nullptr);
    CreateDirectoryW((data += L"\\game").c_str(), nullptr);
    cache_dir_ = data + L"\\";
    cache_name_ = "%LOCALAPPDATA%\\Swofty\\cache\\game";
    api->log("[core] caches: the game can't create files in swofty\\cache (error %lu); the scan and exe hash caches "
             "are in %s",
             error, cache_name_);
    return cache_dir_;
}

}

namespace runtime {

const std::wstring &cache_dir() { return loader::scanner().caches(Runtime::instance().dir()); }

}

namespace runtime::loader {

bool Scanner::open(SwoftyApi *a, const std::wstring &dir, long long exe_bytes)
{
    loader_scan_ = a->scan;
    HMODULE loader = nullptr;
    GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
                       reinterpret_cast<LPCWSTR>(reinterpret_cast<void *>(a->scan)), &loader);
    loader_scan_many_ = loader ? reinterpret_cast<int (*)(const char *, const char *const *, int, int *, uintptr_t *)>(
                                     reinterpret_cast<void *>(GetProcAddress(loader, "SwoftyLoaderScanMany")))
                               : nullptr;
    a->scan = [](const char *section, const char *pattern, uintptr_t *out) {
        return scanner().addon_scan(section, pattern, out);
    };
    char v[16] = {0};
    if (GetEnvironmentVariableA("SWOFTY_RESOLVE", v, sizeof v) && !_stricmp(v, "scan")) {
        api->log("[core] binding resolution: SWOFTY_RESOLVE=scan, every pattern is scanned and the scan cache is off");
        return false;
    }
    uint64_t key = image().exe_key(exe_bytes);
    cache_path_ = caches(dir) + L"scan-cache.tsv";
    std::string text = read_whole_file(cache_path_);
    if (text.empty()) {
        cache_.parse("", key);
        api->log("[core] scan cache: none yet for this exe (key %016llx); patterns found by a scan are kept in "
                 "%s\\scan-cache.tsv",
                 (unsigned long long)key, cache_name_);
    } else if (!cache_.parse(text, key)) {
        api->log("[core] scan cache: %s\\scan-cache.tsv belongs to another exe; starting over (key %016llx)", cache_name_,
                 (unsigned long long)key);
    } else {
        api->log("[core] scan cache: %zu address(es) for this exe (key %016llx), each checked against the pattern before use",
                 cache_.rva.size(), (unsigned long long)key);
    }
    cache_on_ = true;
    return true;
}

void Scanner::save()
{
    AcquireSRWLockExclusive(&lock_);
    std::string text;
    size_t n = cache_.rva.size();
    if (cache_on_ && cache_.dirty)
        text = cache_.serialize(), cache_.dirty = false;
    ReleaseSRWLockExclusive(&lock_);
    if (text.empty())
        return;
    std::wstring tmp = cache_path_ + L".tmp";
    HANDLE f = CreateFileW(tmp.c_str(), GENERIC_WRITE, 0, nullptr, CREATE_ALWAYS, FILE_ATTRIBUTE_NORMAL, nullptr);
    DWORD w = 0;
    bool ok = f != INVALID_HANDLE_VALUE && WriteFile(f, text.data(), DWORD(text.size()), &w, nullptr) && w == text.size();
    if (f != INVALID_HANDLE_VALUE)
        CloseHandle(f);
    ok = ok && MoveFileExW(tmp.c_str(), cache_path_.c_str(), MOVEFILE_REPLACE_EXISTING);
    if (!ok)
        DeleteFileW(tmp.c_str());
    if (ok)
        api->log("[core] scan cache: %zu address(es) saved to %s\\scan-cache.tsv", n, cache_name_);
    else
        api->log("[core] scan cache: writing %s\\scan-cache.tsv failed (%zu address(es) kept for this session)", cache_name_, n);
}

/* Core and addon scans name Steam's .xtls; the set's section map sends it to this build's code section. */
std::string Scanner::game_code() const
{
    auto it = section_alias_.find(".xtls");
    if (it != section_alias_.end())
        return it->second;
    return image().section_by_name(".xtls") ? ".xtls" : GameImage::section_name(image().code_section());
}

}
