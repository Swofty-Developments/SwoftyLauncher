#pragma once

#include "core/render/rtas_guard/geometry_logic.hpp"

#include <algorithm>
#include <array>
#include <cstdint>
#include <cstdio>
#include <string>
#include <unordered_map>
#include <vector>

namespace render::rtas_guard {

constexpr int kBriefGeos = 12;
constexpr uint32_t kPerformUpdate = 0x20;

struct Brief {
    int type = kTriangles;
    uint32_t vertices = 0, indices = 0, aabbs = 0, stride = 0, flags = 0;
    int vertex_format = 0, index_format = 0;
    bool transform = false;
};

struct Entry {
    uint64_t seq = 0, frame = 0, list = 0, dest = 0;
    uint32_t flags = 0, count = 0;
    bool top = false;
    Brief geo[kBriefGeos];
};

inline bool same_format(const Brief &a, const Brief &b)
{
    return a.vertex_format == b.vertex_format && a.stride == b.stride && a.index_format == b.index_format && a.flags == b.flags &&
           a.transform == b.transform;
}

// The geometries of a build: each one's counts, and its formats wherever they differ from the first triangle geometry's.
inline std::string geometry_text(const Entry &e)
{
    std::string s;
    char b[96];
    const Brief *first = nullptr;
    uint32_t shown = e.count < uint32_t(kBriefGeos) ? e.count : uint32_t(kBriefGeos);
    for (uint32_t i = 0; i < shown; i++) {
        const Brief &g = e.geo[i];
        if (g.type != kTriangles) {
            std::snprintf(b, sizeof b, " aabb%u", g.aabbs);
            s += b;
            continue;
        }
        std::snprintf(b, sizeof b, " v%u/i%u", g.vertices, g.indices);
        s += b;
        if (!first || !same_format(*first, g)) {
            std::snprintf(b, sizeof b, "(vf%d s%u if%d fl%u%s)", g.vertex_format, g.stride, g.index_format, g.flags,
                          g.transform ? " t" : "");
            s += b;
        }
        if (!first)
            first = &g;
    }
    if (shown < e.count) {
        std::snprintf(b, sizeof b, " +%u more", e.count - shown);
        s += b;
    }
    return s;
}

// The newest builds, kept so a crash report can show what the GPU was building when the device was removed.
template <size_t N> class Recent {
public:
    void push(const Entry &e)
    {
        ring_[pushed_ % N] = e;
        pushed_++;
    }

    uint64_t pushed() const { return pushed_; }

    std::vector<Entry> oldest_first() const
    {
        std::vector<Entry> v;
        for (uint64_t i = pushed_ > N ? pushed_ - N : 0; i < pushed_; i++)
            v.push_back(ring_[i % N]);
        return v;
    }

private:
    std::array<Entry, N> ring_{};
    uint64_t pushed_ = 0;
};

// Each build's place among the builds on its command list in its frame, counted from 1.
inline std::vector<uint32_t> list_ordinals(const std::vector<Entry> &v)
{
    std::vector<uint32_t> out(v.size(), 1);
    std::unordered_map<uint64_t, std::pair<uint64_t, uint32_t>> at;
    for (size_t i = 0; i < v.size(); i++) {
        auto &a = at[v[i].list];
        if (a.second && a.first == v[i].frame)
            a.second++;
        else
            a = {v[i].frame, 1};
        out[i] = a.second;
    }
    return out;
}

// The newest builds of each command list, so a list the game records rarely keeps its builds however many builds
// the busy lists record. Past the list cap, the list with the oldest newest build is dropped.
template <size_t PerList, size_t Lists> class ByList {
public:
    struct Kept {
        uint64_t list = 0, total = 0;
        std::vector<Entry> builds;
    };

    void push(const Entry &e)
    {
        auto it = lists_.find(e.list);
        if (it == lists_.end()) {
            if (lists_.size() >= Lists) {
                auto old = lists_.begin();
                for (auto i = lists_.begin(); i != lists_.end(); ++i)
                    if (i->second.last < old->second.last)
                        old = i;
                lists_.erase(old);
            }
            it = lists_.try_emplace(e.list).first;
        }
        it->second.ring.push(e);
        it->second.last = e.seq;
        pushed_++;
    }

    uint64_t pushed() const { return pushed_; }

    std::vector<Kept> newest_lists_first() const
    {
        std::vector<std::pair<uint64_t, uint64_t>> order;
        for (const auto &[list, slot] : lists_)
            order.push_back({slot.last, list});
        std::sort(order.rbegin(), order.rend());
        std::vector<Kept> out;
        for (const auto &o : order) {
            const Slot &slot = lists_.at(o.second);
            out.push_back({o.second, slot.ring.pushed(), slot.ring.oldest_first()});
        }
        return out;
    }

private:
    struct Slot {
        Recent<PerList> ring;
        uint64_t last = 0;
    };
    std::unordered_map<uint64_t, Slot> lists_;
    uint64_t pushed_ = 0;
};

// The build the GPU stopped on, for each command list DRED shows unfinished. Its build #nth of `of` is #nth of the
// list's newest kept frame when that frame keeps exactly `of` builds; otherwise the build is not known.
template <typename KeptList, typename Stops> std::string stopped_lines(const KeptList &lists, const Stops &stops)
{
    std::string s;
    char b[240];
    for (const auto &st : stops) {
        const typename KeptList::value_type *kept = nullptr;
        for (const auto &k : lists)
            if (k.list == st.list)
                kept = &k;
        if (!kept || kept->builds.empty()) {
            std::snprintf(b, sizeof b, "  the GPU stopped on list 0x%llx's build #%u of %u; no build of that list was kept\n",
                          (unsigned long long)st.list, st.nth, st.of);
            s += b;
            continue;
        }
        uint64_t frame = kept->builds.back().frame;
        size_t first = kept->builds.size();
        while (first && kept->builds[first - 1].frame == frame)
            first--;
        size_t in_frame = kept->builds.size() - first;
        bool whole = !(first == 0 && kept->total > kept->builds.size());
        if (!whole || in_frame != st.of || !st.nth || st.nth > st.of) {
            std::snprintf(b, sizeof b,
                          "  the GPU stopped on list 0x%llx's build #%u of %u; its newest kept frame %llu has %zu%s build(s), "
                          "so which build that is is not known\n",
                          (unsigned long long)st.list, st.nth, st.of, (unsigned long long)frame, in_frame, whole ? "" : "+");
            s += b;
            continue;
        }
        const Entry &e = kept->builds[first + st.nth - 1];
        std::snprintf(
            b, sizeof b,
            "  the GPU stopped on list 0x%llx's build #%u of %u: build %llu frame %llu %s%s into 0x%llx (flags 0x%x), %u %s:",
            (unsigned long long)st.list, st.nth, st.of, (unsigned long long)e.seq, (unsigned long long)e.frame,
            e.top ? "top-level" : "bottom-level", (e.flags & kPerformUpdate) ? " update" : "", (unsigned long long)e.dest,
            e.flags, e.count, e.top ? "instances" : "geometries");
        s += b;
        if (!e.top)
            s += geometry_text(e);
        s += "\n";
    }
    return s;
}

template <typename KeptList>
std::string recent_section(const KeptList &lists, uint64_t pushed, size_t max_lists = 12, const std::string &stopped = {})
{
    if (lists.empty())
        return stopped.empty() ? std::string() : "\n" + stopped;
    std::string s;
    char b[200];
    std::snprintf(b, sizeof b,
                  "\n  ray tracing builds before the crash, %zu command list(s), the list with the newest build first; "
                  "%llu build(s) in all\n",
                  lists.size(), (unsigned long long)pushed);
    s += b;
    s += "  each list shows its newest builds, oldest first; #n is the build's place on its list in that frame\n";
    s += stopped;
    size_t shown_lists = 0;
    for (const auto &k : lists) {
        if (shown_lists++ == max_lists) {
            std::snprintf(b, sizeof b, "    %zu more command list(s) not shown\n", lists.size() - max_lists);
            s += b;
            break;
        }
        std::snprintf(b, sizeof b, "    list 0x%llx: the newest %zu of %llu build(s)\n", (unsigned long long)k.list,
                      k.builds.size(), (unsigned long long)k.total);
        s += b;
        bool cut = k.total > k.builds.size();
        std::vector<uint32_t> nth = list_ordinals(k.builds);
        for (size_t i = 0; i < k.builds.size(); i++) {
            const Entry &e = k.builds[i];
            bool unsure = cut && e.frame == k.builds.front().frame;
            std::snprintf(b, sizeof b,
                          "      build %llu frame %llu #%u%s %s%s into 0x%llx (flags 0x%x), %u %s:", (unsigned long long)e.seq,
                          (unsigned long long)e.frame, nth[i], unsure ? "+" : "", e.top ? "top-level" : "bottom-level",
                          (e.flags & kPerformUpdate) ? " update" : "", (unsigned long long)e.dest, e.flags, e.count,
                          e.top ? "instances" : "geometries");
            s += b;
            if (!e.top)
                s += geometry_text(e);
            s += "\n";
        }
    }
    return s;
}

}
