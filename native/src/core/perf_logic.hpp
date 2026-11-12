#pragma once

#include <algorithm>
#include <cstdint>
#include <cstdio>
#include <string>
#include <vector>

// Formatting and aggregation for the once-a-minute [perf] lines in loader.log (perf.cpp): VirtualQuery calls made from
// swofty modules, the process's CPU use and the busiest probes with their call rates.

namespace perf::logic {

struct Count {
    std::string what;
    uint64_t calls = 0;
    double ms = 0;
};

struct VqMinute {
    double secs = 0;
    uint64_t calls = 0, game_calls = 0;
    double ms = 0, game_ms = 0, worst_ms = 0;
    std::vector<Count> modules, sites;
    bool exe_hooked = false;
    uint64_t exe_calls = 0;
    double exe_ms = 0;
    uint64_t page_resident = 0, page_region = 0;
};

struct ThreadCpu {
    std::string what;
    double cores = 0;
};

struct CpuMinute {
    double secs = 0, user = 0, kernel = 0, ours_user = 0, ours_kernel = 0;
    size_t threads = 0;
    std::vector<ThreadCpu> top;
};

struct Busy {
    std::string what;
    double ms = 0;
    uint64_t calls = 0;
};

inline uint64_t delta(uint64_t now, uint64_t was)
{
    return now > was ? now - was : 0;
}

inline std::string grouped(uint64_t n)
{
    std::string d = std::to_string(n), out;
    for (size_t i = 0; i < d.size(); i++) {
        if (i && (d.size() - i) % 3 == 0)
            out += ',';
        out += d[i];
    }
    return out;
}

inline std::string fmt(const char *f, double v)
{
    char b[48];
    snprintf(b, sizeof b, f, v);
    return b;
}

inline std::string rva_label(const std::string &module, uint64_t rva)
{
    char b[32];
    snprintf(b, sizeof b, "+0x%llx", (unsigned long long)rva);
    return module + b;
}

inline double cores(uint64_t filetime_100ns, double secs)
{
    return secs > 0 ? double(filetime_100ns) / 1e7 / secs : 0;
}

inline std::vector<Count> ranked(std::vector<Count> v, size_t n)
{
    std::erase_if(v, [](const Count &c) { return !c.calls; });
    std::stable_sort(v.begin(), v.end(), [](const Count &a, const Count &b) {
        return a.ms != b.ms ? a.ms > b.ms : a.calls > b.calls;
    });
    if (v.size() > n)
        v.resize(n);
    return v;
}

inline std::string counts(const std::vector<Count> &v)
{
    std::string out;
    for (const Count &c : v)
        out += (out.empty() ? "" : ", ") + c.what + " " + grouped(c.calls) + " calls " + fmt("%.1f ms", c.ms);
    return out;
}

inline std::string vq_line(const VqMinute &m)
{
    std::string out = "[perf] last " + fmt("%.0f", m.secs) + " s: VirtualQuery from swofty " + grouped(m.calls) +
                      " calls " + fmt("%.1f ms", m.ms);
    if (m.calls)
        out += ", worst " + fmt("%.2f ms", m.worst_ms);
    out += " (game threads " + grouped(m.game_calls) + " calls " + fmt("%.1f ms", m.game_ms) + ")";
    std::vector<Count> mods = ranked(m.modules, 8), sites = ranked(m.sites, 5);
    if (!mods.empty())
        out += ": " + counts(mods);
    if (!sites.empty())
        out += "; top sites " + counts(sites);
    if (m.exe_hooked)
        out += "; game exe " + grouped(m.exe_calls) + " calls " + fmt("%.1f ms", m.exe_ms);
    out += "; memory checks: " + grouped(m.page_resident) + " page(s) from the working set, " + grouped(m.page_region) +
           " from VirtualQuery";
    return out;
}

inline std::string cpu_line(const CpuMinute &c)
{
    std::string out = "[perf] last " + fmt("%.0f", c.secs) + " s: process CPU user " + fmt("%.2f", c.user) +
                      " kernel " + fmt("%.2f", c.kernel) + " cores; swofty threads " +
                      fmt("%.3f", c.ours_user + c.ours_kernel) + " cores (user " + fmt("%.3f", c.ours_user) +
                      " kernel " + fmt("%.3f", c.ours_kernel) + ") in " + std::to_string(c.threads) + " thread(s)";
    std::vector<ThreadCpu> top = c.top;
    std::stable_sort(top.begin(), top.end(), [](const ThreadCpu &a, const ThreadCpu &b) { return a.cores > b.cores; });
    std::string list;
    for (size_t i = 0; i < top.size() && i < 3; i++) {
        if (top[i].cores < 0.0005)
            break;
        list += (list.empty() ? "" : ", ") + top[i].what + " " + fmt("%.3f", top[i].cores);
    }
    if (!list.empty())
        out += ": " + list;
    return out;
}

inline std::string busiest(std::vector<Busy> v, double secs, size_t n)
{
    std::stable_sort(v.begin(), v.end(), [](const Busy &a, const Busy &b) { return a.ms > b.ms; });
    std::string out;
    for (size_t i = 0; i < v.size() && i < n; i++)
        out += (out.empty() ? "" : ", ") + v[i].what + " " + fmt("%.3f ms/s", secs > 0 ? v[i].ms / secs : 0) + " " +
               fmt("%.1f calls/s", secs > 0 ? double(v[i].calls) / secs : 0);
    return out;
}

inline size_t site_slot(uintptr_t at, unsigned bits)
{
    return size_t((uint64_t(at) * 0x9E3779B97F4A7C15ull) >> (64 - bits));
}

}
