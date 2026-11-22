#pragma once
#include <cstddef>
#include <cwctype>
#include <string>
#include <unordered_map>
#include <utility>
#include <vector>

namespace render::postfx {

// The addon whose DLL is at `module`. The loader loads an addon's DLLs from <game folder>\swofty\addons\<id>\, the
// game folder being the exe's (`exe`), so it's the folder right under that one which holds the DLL, matched from the
// start of the path and without case: a DLL in a deeper folder named like another addon is still its own addon's.
// Empty for a DLL anywhere else (the game's, Windows', one straight in swofty\addons\). Paths as GetModuleFileNameW
// gives them.
inline std::string addon_of_module(std::wstring module, std::wstring exe)
{
    for (std::wstring *p : {&module, &exe})
        for (auto &ch : *p)
            ch = ch == L'/' ? L'\\' : wchar_t(towlower(ch));
    size_t slash = exe.rfind(L'\\');
    if (slash == std::wstring::npos)
        return {};
    std::wstring root = exe.substr(0, slash) + L"\\swofty\\addons\\";
    if (module.size() <= root.size() || module.compare(0, root.size(), root) != 0)
        return {};
    size_t end = module.find(L'\\', root.size());
    if (end == std::wstring::npos || end == root.size())
        return {};
    std::string id;
    for (size_t i = root.size(); i < end; i++)
        id += module[i] < 0x80 ? char(module[i]) : '?';
    return id;
}

// The bytes addons' code gives their passes, by addon id and pass name. Until the core has read the profile's passes
// (an addon's on-load can come first) it can't tell a pass the profile has from one it hasn't, so it keeps the bytes
// of any pass, of EARLY_PASSES passes an addon at most; listed() then drops those of passes the profile hasn't, and
// from then on only passes it has get any.
class DataStore {
public:
    static constexpr size_t EARLY_PASSES = 16;

    static std::string key(const std::string &addon_id, const std::string &pass) { return addon_id + '\n' + pass; }

    bool is_listed() const { return listed_; }

    // Before the passes are listed: keeps the bytes for any pass (size 0: drops them). False when that would be more
    // than EARLY_PASSES passes of the addon; nothing is kept then.
    bool set_early(const std::string &k, const void *data, size_t size)
    {
        if (size && !bytes_.count(k)) {
            std::string_view addon(k.data(), k.find('\n') + 1); // "<addon id>\n"
            size_t n = 0;
            for (const auto &e : bytes_)
                n += std::string_view(e.first).substr(0, addon.size()) == addon;
            if (n >= EARLY_PASSES)
                return false;
        }
        set(k, data, size);
        return true;
    }

    // Keeps the bytes of a pass the profile has (size 0: drops them).
    void set(const std::string &k, const void *data, size_t size)
    {
        if (size)
            bytes_[k].assign(static_cast<const char *>(data), size);
        else
            bytes_.erase(k);
    }

    // The passes are listed: has(key) says which the profile has. The bytes of the others go.
    template <class Has> void listed(Has has)
    {
        listed_ = true;
        for (auto it = bytes_.begin(); it != bytes_.end();)
            it = has(it->first) ? std::next(it) : bytes_.erase(it);
    }

    const std::string *find(const std::string &k) const
    {
        auto it = bytes_.find(k);
        return it == bytes_.end() ? nullptr : &it->second;
    }

    bool empty() const { return bytes_.empty(); }
    size_t size() const { return bytes_.size(); }

private:
    std::unordered_map<std::string, std::string> bytes_;
    bool listed_ = false;
};

}
