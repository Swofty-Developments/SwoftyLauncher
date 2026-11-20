#pragma once
#include <string>

// Pure path logic of the crash reporter (crash.cpp): the data-folder directory a crash report goes to when
// the game's swofty folder refuses it (the same state folder as loader-fallback.log), and how the log line
// names where a report went. perf.log (perf.cpp) goes to the same folder as perf-fallback.log. No Windows
// calls, so the unit test runs on the host.

namespace crash_dir {

inline std::wstring fallback(std::wstring data_folder)
{
    while (!data_folder.empty() && (data_folder.back() == L'\\' || data_folder.back() == L'/'))
        data_folder.pop_back();
    return data_folder.empty() ? std::wstring() : data_folder + L"\\state\\";
}

inline std::wstring perf_fallback(const std::wstring &data_folder)
{
    std::wstring dir = fallback(data_folder);
    return dir.empty() ? dir : dir + L"perf-fallback.log";
}

inline std::string logged(const std::string &dir, const std::string &swofty_dir, const std::string &name)
{
    return dir == swofty_dir ? "swofty\\" + name : dir + name;
}

}
