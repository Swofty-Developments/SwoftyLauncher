#include "core/diagnostics/crash/dred_state.hpp"

#include <string>
#include <vector>

// The removal markers in the marker folders: dred.flag after a removal, dred.checked naming the newest crash
// report read, and the newest crash report itself when no flag is left.
namespace runtime::dred {

namespace {

std::string read_file(const std::wstring &path, size_t cap)
{
    HANDLE f = CreateFileW(path.c_str(), GENERIC_READ, FILE_SHARE_READ | FILE_SHARE_WRITE, nullptr, OPEN_EXISTING,
                           FILE_ATTRIBUTE_NORMAL, nullptr);
    if (f == INVALID_HANDLE_VALUE)
        return {};
    std::string s(cap, '\0');
    DWORD n = 0;
    ReadFile(f, s.data(), DWORD(cap), &n, nullptr);
    CloseHandle(f);
    s.resize(n);
    return s;
}

DWORD write_text(const std::wstring &path, const std::string &text)
{
    HANDLE f = CreateFileW(path.c_str(), GENERIC_WRITE, FILE_SHARE_READ, nullptr, CREATE_ALWAYS, FILE_ATTRIBUTE_NORMAL, nullptr);
    if (f == INVALID_HANDLE_VALUE)
        return GetLastError();
    DWORD w = 0, err = 0;
    if (!WriteFile(f, text.data(), DWORD(text.size()), &w, nullptr))
        err = GetLastError();
    CloseHandle(f);
    return err;
}

bool put_marker(const wchar_t *name, const std::string &text)
{
    DWORD err = 0;
    int at = write_first(marker_folders, [&](const std::wstring &dir) {
        if (dir.size() > 4 && dir.find(L"\\state\\") != std::wstring::npos)
            CreateDirectoryW((dir.substr(0, dir.size() - 7)).c_str(), nullptr), CreateDirectoryW(dir.c_str(), nullptr);
        err = write_text(dir + name, text);
        if (err && log_fn)
            log_fn("[dred] could not write %ls%ls: error %lu", dir.c_str(), name, (unsigned long)err);
        return err == 0;
    });
    return at >= 0;
}

std::string read_marker(const wchar_t *name)
{
    for (const auto &d : marker_folders) {
        std::string s = read_file(d + name, 128);
        if (!s.empty())
            return s;
    }
    return {};
}

bool take_marker(const wchar_t *name)
{
    bool had = false;
    for (const auto &d : marker_folders)
        if (GetFileAttributesW((d + name).c_str()) != INVALID_FILE_ATTRIBUTES) {
            had = true;
            DeleteFileW((d + name).c_str());
        }
    return had;
}

std::wstring widen(const std::string &s) { return std::wstring(s.begin(), s.end()); }

std::string newest_in(const std::wstring &dir)
{
    std::vector<std::string> names;
    WIN32_FIND_DATAW fd;
    HANDLE h = FindFirstFileW((dir + L"crash-*.txt").c_str(), &fd);
    if (h == INVALID_HANDLE_VALUE)
        return {};
    do {
        std::string n;
        for (const wchar_t *c = fd.cFileName; *c; c++)
            n += *c < 0x80 ? char(*c) : '?';
        names.push_back(n);
    } while (FindNextFileW(h, &fd));
    FindClose(h);
    return newest_report(names);
}

}

bool prior_session_removed(const char **how)
{
    if (take_marker(L"dred.flag")) {
        *how = "dred.flag";
        return true;
    }
    std::string checked = read_marker(L"dred.checked");
    while (!checked.empty() && (checked.back() == '\n' || checked.back() == '\r'))
        checked.pop_back();
    std::wstring found_dir;
    std::string newest;
    for (const auto &d : marker_folders) {
        std::string c = newest_in(d);
        if (c > newest)
            newest = c, found_dir = d;
    }
    if (newest.empty() || newest == checked)
        return false;
    put_marker(L"dred.checked", newest + "\n");
    if (!should_arm_from(checked, newest, read_file(found_dir + widen(newest), 4 << 20)))
        return false;
    *how = "the newest crash report";
    return true;
}

void write_flag() { put_marker(L"dred.flag", "removed\n"); }

}
