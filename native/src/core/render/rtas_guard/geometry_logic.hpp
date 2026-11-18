#pragma once

#include <cstdint>
#include <unordered_map>
#include <vector>

namespace render::rtas_guard {

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

}
