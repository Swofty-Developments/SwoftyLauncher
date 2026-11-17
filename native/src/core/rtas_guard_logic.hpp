#pragma once

#include <cstdint>
#include <unordered_map>
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

namespace runtime::rtas_guard {

enum GeoType : int { kTriangles = 0, kAabbs = 1 };
enum Level : int { kTop = 0, kBottom = 1 };

struct Geo {
    int type = kTriangles;
    uint64_t vertices = 0;
    uint32_t vertex_count = 0;
    uint64_t indices = 0;
    uint32_t index_count = 0;
    bool indexed = false;
    uint64_t aabbs = 0;
    uint64_t aabb_count = 0;
};

enum class Fault { none, vertices, indices, aabbs };

inline const char *fault_name(Fault f)
{
    switch (f) {
    case Fault::vertices:
        return "a null vertex buffer";
    case Fault::indices:
        return "a null index buffer";
    case Fault::aabbs:
        return "a null AABB buffer";
    case Fault::none:
        break;
    }
    return "no fault";
}

inline Fault fault(const Geo &g)
{
    if (g.type == kAabbs)
        return g.aabb_count && !g.aabbs ? Fault::aabbs : Fault::none;
    if (g.type != kTriangles)
        return Fault::none;
    if (g.indexed) {
        if (!g.index_count)
            return Fault::none;
        if (!g.vertices)
            return Fault::vertices;
        return g.indices ? Fault::none : Fault::indices;
    }
    return g.vertex_count && !g.vertices ? Fault::vertices : Fault::none;
}

inline void empty(Geo &g)
{
    g.vertex_count = 0;
    g.index_count = 0;
    g.aabb_count = 0;
}

struct Build {
    uint64_t dest = 0, source = 0;
    bool update = false;
};

struct Plan {
    std::vector<uint32_t> emptied;
    uint32_t faulty = 0, kept_empty = 0;
};

// The bottom-level structures (by destination address) with geometries the guard emptied, and which.
class Book {
public:
    Plan plan(const Build &b, const std::vector<Geo> &geos)
    {
        Plan p;
        std::vector<bool> marked(geos.size(), false);
        for (uint32_t i = 0; i < geos.size(); i++)
            if (fault(geos[i]) != Fault::none) {
                marked[i] = true;
                p.faulty++;
            }
        if (b.update) {
            auto it = emptied_.find(b.source ? b.source : b.dest);
            if (it != emptied_.end())
                for (uint32_t i : it->second)
                    if (i < marked.size() && !marked[i]) {
                        marked[i] = true;
                        p.kept_empty++;
                    }
        }
        for (uint32_t i = 0; i < marked.size(); i++)
            if (marked[i])
                p.emptied.push_back(i);
        if (p.emptied.empty())
            emptied_.erase(b.dest);
        else
            emptied_[b.dest] = p.emptied;
        return p;
    }

    size_t tracked() const { return emptied_.size(); }

private:
    std::unordered_map<uint64_t, std::vector<uint32_t>> emptied_;
};

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

// The bottom-level builds the build log names: each structure (by destination address) once for each shape, so a
// structure refitted or rebuilt every frame is named once, and nothing once the cap is reached.
class Trace {
public:
    explicit Trace(uint32_t cap) : cap_(cap) {}

    bool want(uint64_t dest, uint64_t shape)
    {
        if (named_ >= cap_ || !seen_.insert(dest * 0x9e3779b97f4a7c15ull ^ shape).second)
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
