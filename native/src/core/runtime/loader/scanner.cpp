#include "core/runtime/loader/scanner.hpp"

#include <bcrypt.h>

#include <algorithm>
#include <cstdio>
#include <cstring>
#include <mutex>
#include <vector>

#include "core/runtime/loader/code_encryption.hpp"
#include "core/runtime/loader/host_api.hpp"
#include "core/runtime/loader/mixins.hpp"
#include "core/runtime/loader/resolve_locks.hpp"
#include "loader/data_folder.h"

namespace runtime::loader {

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

int Scanner::scan(const char *section, const char *pattern, uintptr_t *out)
{
    if (out)
        *out = 0;
    if (!cache_on_ || !section || !pattern)
        return loader_scan_(section, pattern, out);
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
    int hits = loader_scan_(section, pattern, &at);
    AcquireSRWLockExclusive(&lock_);
    cache_.found(section, pattern, hits, hits == 1 ? image().rva(at) : 0);
    ReleaseSRWLockExclusive(&lock_);
    if (out && hits == 1)
        *out = at;
    return hits;
}

void Scanner::read_section_map(const json::Value &doc, const std::string &file, const std::unordered_map<std::string, size_t> &uses,
                               bool any_binding)
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

bool Scanner::open(SwoftyApi *a, const std::wstring &dir, long long exe_bytes)
{
    loader_scan_ = a->scan;
    a->scan = [](const char *section, const char *pattern, uintptr_t *out) { return scanner().addon_scan(section, pattern, out); };
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
