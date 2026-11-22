#pragma once

#include "core/render/rtas_guard/geometry_logic.hpp"
#include "core/render/rtas_guard/recent_logic.hpp"

#include <cstdint>
#include <string>
#include <unordered_set>
#include <vector>

// A ray tracing build reads every active geometry's vertex and index buffers, the instance list of a
// top-level build and the structure an update starts from, and writes the destination, the scratch
// memory and any postbuild info. A null address in any of them makes the GPU touch address 0: on
// NVIDIA under Windows that is a page fault and the device is removed (DXGI_ERROR_DEVICE_HUNG).
// The guard gives a bottom-level geometry with a null buffer zero primitives, so the build reads
// nothing for it. An update build must keep the primitive counts of the build it updates, so a
// geometry emptied in a build stays empty in every update of it. A top-level build with a null
// instance list builds no instances. A build with a null destination, scratch or update source is
// not sent, and postbuild info with a null destination is dropped.

namespace render::rtas_guard {

// The primitive counts of a bottom-level build's geometries, hashed one geometry at a time.
struct Shape {
    uint64_t hash = 1469598103934665603ull;

    void mix(uint64_t v) { hash = (hash ^ v) * 1099511628211ull; }

    void add(const Geo &g)
    {
        mix(uint64_t(g.type));
        mix(g.vertex_count);
        mix(g.index_count);
        mix(g.aabb_count);
    }
};

// The shapes the build log has named, each once, up to the cap.
class Trace {
public:
    explicit constexpr Trace(uint32_t cap) : cap_(cap) {}

    bool want(uint64_t shape)
    {
        if (named_ >= cap_ || !seen_.insert(shape).second)
            return false;
        named_++;
        return true;
    }

    bool full() const { return named_ >= cap_; }
    uint32_t named() const { return named_; }

private:
    uint32_t cap_, named_ = 0;
    std::unordered_set<uint64_t> seen_;
};

// The ray tracing options in the game's hardware_settings_config.xml: each <rt_NAME enabled="..."/> as "NAME on"
// or "NAME off", in file order.
inline std::string rt_options(const std::string &xml)
{
    std::string out;
    for (size_t at = xml.find("<rt_"); at != std::string::npos; at = xml.find("<rt_", at + 4)) {
        size_t close = xml.find('>', at), name_end = xml.find_first_of(" \t\r\n/>", at + 4);
        if (close == std::string::npos || name_end == std::string::npos || name_end > close)
            break;
        std::string tag = xml.substr(at, close - at);
        size_t en = tag.find("enabled=\"");
        size_t end = en == std::string::npos ? std::string::npos : tag.find('"', en + 9);
        if (end == std::string::npos)
            continue;
        std::string value = tag.substr(en + 9, end - en - 9);
        out += (out.empty() ? "" : ", ") + xml.substr(at + 4, name_end - at - 4) +
               (value == "true"    ? " on"
                : value == "false" ? " off"
                                   : " " + value);
    }
    return out;
}

struct Call {
    int level = kBottom;
    uint64_t dest = 0, scratch = 0, source = 0, instances = 0;
    uint32_t count = 0;
    bool update = false;
};

enum class Hole { none, dest, scratch, source, instances };

inline const char *hole_name(Hole h)
{
    switch (h) {
    case Hole::dest:
        return "a null destination";
    case Hole::scratch:
        return "null scratch memory";
    case Hole::source:
        return "an update with a null source structure";
    case Hole::instances:
        return "a null instance list";
    case Hole::none:
        break;
    }
    return "no hole";
}

inline Hole hole(const Call &c)
{
    if (!c.dest)
        return Hole::dest;
    if (!c.scratch)
        return Hole::scratch;
    if (c.update && !c.source)
        return Hole::source;
    if (c.level == kTop && c.count && !c.instances)
        return Hole::instances;
    return Hole::none;
}

enum class Act { send, skip, no_instances };

inline Act act(const Call &c)
{
    switch (hole(c)) {
    case Hole::none:
        return Act::send;
    case Hole::instances:
        return c.update ? Act::skip : Act::no_instances;
    default:
        return Act::skip;
    }
}

inline const char *act_name(Act a)
{
    switch (a) {
    case Act::skip:
        return "not sent";
    case Act::no_instances:
        return "built with no instances";
    case Act::send:
        break;
    }
    return "sent";
}

inline std::vector<uint32_t> kept_posts(const std::vector<uint64_t> &dests)
{
    std::vector<uint32_t> kept;
    for (uint32_t i = 0; i < dests.size(); i++)
        if (dests[i])
            kept.push_back(i);
    return kept;
}

inline bool detailed(uint64_t nth, uint64_t limit) { return nth <= limit; }

inline bool milestone(uint64_t nth) { return nth && !(nth & (nth - 1)); }

enum class Mode { off, log, on };

inline bool same(const char *a, const char *b)
{
    for (; *a && *b; a++, b++)
        if ((*a | 0x20) != (*b | 0x20))
            return false;
    return !*a && !*b;
}

inline Mode mode(const char *env)
{
    if (!env)
        return Mode::on;
    if (same(env, "0") || same(env, "off"))
        return Mode::off;
    if (same(env, "log"))
        return Mode::log;
    return Mode::on;
}

}
