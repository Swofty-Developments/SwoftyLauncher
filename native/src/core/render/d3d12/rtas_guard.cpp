#include "core/hud/hardware_settings.hpp"
#include "core/render/d3d12/hooking.hpp"
#include "core/render/d3d12_hooks.hpp"
#include "core/render/rtas_guard_logic.hpp"
#include "core/runtime/runtime.hpp"

#include <swofty/mixin.hpp>

#include <atomic>
#include <cstdio>
#include <cstdlib>
#include <mutex>
#include <vector>

namespace render::d3d12 {

namespace {

constexpr size_t BUILD_RTAS_SLOT = 72;
constexpr int RTAS_DETAILED = 8;
constexpr uint32_t RTAS_NAMED = 800;

using RtasDesc = D3D12_BUILD_RAYTRACING_ACCELERATION_STRUCTURE_DESC;
using GeoDesc = D3D12_RAYTRACING_GEOMETRY_DESC;
using PostDesc = D3D12_RAYTRACING_ACCELERATION_STRUCTURE_POSTBUILD_INFO_DESC;

const GeoDesc &geo_at(const D3D12_BUILD_RAYTRACING_ACCELERATION_STRUCTURE_INPUTS &in, UINT i)
{
    return in.DescsLayout == D3D12_ELEMENTS_LAYOUT_ARRAY ? in.pGeometryDescs[i] : *in.ppGeometryDescs[i];
}

rtas_guard::Geo geo_of(const GeoDesc &d)
{
    rtas_guard::Geo g;
    if (d.Type == D3D12_RAYTRACING_GEOMETRY_TYPE_TRIANGLES) {
        g.type = rtas_guard::kTriangles;
        g.vertices = d.Triangles.VertexBuffer.StartAddress;
        g.vertex_count = d.Triangles.VertexCount;
        g.indices = d.Triangles.IndexBuffer;
        g.index_count = d.Triangles.IndexCount;
        g.indexed = d.Triangles.IndexFormat != DXGI_FORMAT_UNKNOWN;
    } else if (d.Type == D3D12_RAYTRACING_GEOMETRY_TYPE_PROCEDURAL_PRIMITIVE_AABBS) {
        g.type = rtas_guard::kAabbs;
        g.aabbs = d.AABBs.AABBs.StartAddress;
        g.aabb_count = d.AABBs.AABBCount;
    } else
        g.type = -1;
    return g;
}

const char *level_name(const RtasDesc &desc)
{
    return desc.Inputs.Type == D3D12_RAYTRACING_ACCELERATION_STRUCTURE_TYPE_TOP_LEVEL ? "top-level" : "bottom-level";
}

class RtasGuard : public runtime::loader::HookSet<> {
public:
    bool arm();
    void build(ID3D12GraphicsCommandList4 *l, const RtasDesc *desc, UINT posts, const PostDesc *post);
    std::string recent_text(const std::vector<runtime::dred::BuildStop> &stops);

private:
    rtas_guard::Entry entry_of(const void *list, const RtasDesc &desc, bool top);
    void name_build(const rtas_guard::Entry &e, uint64_t nth, const RtasDesc &desc);
    void log_hole(uint64_t nth, const RtasDesc &desc, const rtas_guard::Call &c, rtas_guard::Act a);
    void log_bad_posts(uint64_t nth, const RtasDesc &desc, UINT posts, size_t kept);
    void log_faulty(uint64_t nth, const RtasDesc &desc, const std::vector<rtas_guard::Geo> &geos, const rtas_guard::Plan &plan);

    safetyhook::InlineHook hook;
    rtas_guard::Mode mode = rtas_guard::Mode::on;
    rtas_guard::Book book;
    rtas_guard::Trace named{RTAS_NAMED};
    std::atomic<bool> named_full{false};
    rtas_guard::ByList<64, 32> recent;
    std::atomic<size_t> tracked{0};
    std::atomic<uint64_t> builds{0}, faulty{0}, holes{0}, bad_posts{0}, all{0};
    uint64_t trace_builds = 0;
};

RtasGuard guard;

void STDMETHODCALLTYPE build_rtas_hook(ID3D12GraphicsCommandList4 *l, const RtasDesc *desc, UINT posts, const PostDesc *post)
{
    guard.build(l, desc, posts, post);
}

void RtasGuard::log_hole(uint64_t nth, const RtasDesc &desc, const rtas_guard::Call &c, rtas_guard::Act a)
{
    if (!rtas_guard::detailed(nth, RTAS_DETAILED)) {
        if (rtas_guard::milestone(nth))
            swofty::log("[gfx] rtas guard: %llu builds so far had a null destination, scratch, source or instance list",
                        (unsigned long long)nth);
        return;
    }
    const char *what = mode == rtas_guard::Mode::log ? "sent as is (SWOFTY_RTAS_GUARD=log)" : rtas_guard::act_name(a);
    swofty::log("[gfx] rtas guard: %s build%s has %s (dest 0x%llx, scratch 0x%llx, source 0x%llx, %u %s at 0x%llx); %s",
                level_name(desc), c.update ? " (update)" : "", rtas_guard::hole_name(rtas_guard::hole(c)),
                (unsigned long long)c.dest, (unsigned long long)c.scratch, (unsigned long long)c.source, c.count,
                c.level == rtas_guard::kTop ? "instances" : "geometries", (unsigned long long)c.instances, what);
}

void RtasGuard::log_bad_posts(uint64_t nth, const RtasDesc &desc, UINT posts, size_t kept)
{
    if (!rtas_guard::detailed(nth, RTAS_DETAILED)) {
        if (rtas_guard::milestone(nth))
            swofty::log("[gfx] rtas guard: %llu builds so far asked for postbuild info into a null buffer",
                        (unsigned long long)nth);
        return;
    }
    swofty::log("[gfx] rtas guard: %s build into 0x%llx asks for %u postbuild info(s), %zu into a null buffer; %s",
                level_name(desc), (unsigned long long)desc.DestAccelerationStructureData, posts, posts - kept,
                mode == rtas_guard::Mode::log ? "asked as is (SWOFTY_RTAS_GUARD=log)" : "those are dropped");
}

void RtasGuard::log_faulty(uint64_t nth, const RtasDesc &desc, const std::vector<rtas_guard::Geo> &geos,
                           const rtas_guard::Plan &plan)
{
    if (!rtas_guard::detailed(nth, RTAS_DETAILED)) {
        if (rtas_guard::milestone(nth))
            swofty::log("[gfx] rtas guard: %llu bottom-level builds so far had geometries with a null buffer",
                        (unsigned long long)nth);
        return;
    }
    uint32_t first = plan.emptied.empty() ? 0 : plan.emptied.front();
    for (uint32_t i = 0; i < geos.size(); i++)
        if (rtas_guard::fault(geos[i]) != rtas_guard::Fault::none) {
            first = i;
            break;
        }
    const rtas_guard::Geo &g = geos[first];
    const char *what = mode == rtas_guard::Mode::log
                           ? "left as is (SWOFTY_RTAS_GUARD=log); on NVIDIA under Windows the GPU reads address 0 and "
                             "the device is removed"
                           : "emptied, so the build reads nothing for them";
    swofty::log("[gfx] rtas guard: bottom-level build into 0x%llx%s (%zu geometries): %u with a null buffer%s, first #%u "
                "has %s (%u vertices at 0x%llx, %u indices at 0x%llx); %s",
                (unsigned long long)desc.DestAccelerationStructureData,
                (desc.Inputs.Flags & D3D12_RAYTRACING_ACCELERATION_STRUCTURE_BUILD_FLAG_PERFORM_UPDATE) ? " (update)" : "",
                geos.size(), plan.faulty, plan.kept_empty ? ", plus geometries emptied in the build it updates" : "", first,
                rtas_guard::fault_name(rtas_guard::fault(g)), g.vertex_count, (unsigned long long)g.vertices, g.index_count,
                (unsigned long long)g.indices, what);
}

rtas_guard::Entry RtasGuard::entry_of(const void *list, const RtasDesc &desc, bool top)
{
    const auto &in = desc.Inputs;
    rtas_guard::Entry e;
    e.seq = all.fetch_add(1, std::memory_order_relaxed);
    e.frame = frames_presented();
    e.list = reinterpret_cast<uint64_t>(list);
    e.dest = desc.DestAccelerationStructureData;
    e.flags = unsigned(in.Flags);
    e.count = in.NumDescs;
    e.top = top;
    if (top)
        return e;
    for (UINT i = 0; i < in.NumDescs && i < UINT(rtas_guard::kBriefGeos); i++) {
        const GeoDesc &g = geo_at(in, i);
        rtas_guard::Brief &b = e.geo[i];
        b.flags = unsigned(g.Flags);
        if (g.Type == D3D12_RAYTRACING_GEOMETRY_TYPE_TRIANGLES) {
            const auto &t = g.Triangles;
            b.vertices = t.VertexCount;
            b.indices = t.IndexCount;
            b.stride = uint32_t(t.VertexBuffer.StrideInBytes);
            b.vertex_format = int(t.VertexFormat);
            b.index_format = int(t.IndexFormat);
            b.transform = t.Transform3x4 != 0;
        } else {
            b.type = rtas_guard::kAabbs;
            b.aabbs = uint32_t(g.AABBs.AABBCount);
        }
    }
    return e;
}

void RtasGuard::name_build(const rtas_guard::Entry &e, uint64_t nth, const RtasDesc &desc)
{
    const auto &in = desc.Inputs;
    rtas_guard::Shape shape;
    for (UINT i = 0; i < in.NumDescs; i++)
        shape.add(geo_of(geo_at(in, i)));
    bool want, full;
    {
        std::lock_guard<std::mutex> g(lock);
        want = named.want(shape.hash);
        full = named.full();
    }
    if (full)
        named_full.store(true, std::memory_order_relaxed);
    if (!want)
        return;
    swofty::log("[gfx] rtas build %llu%s on list 0x%llx into 0x%llx (flags 0x%x), %u geometries:%s", (unsigned long long)nth,
                (e.flags & D3D12_RAYTRACING_ACCELERATION_STRUCTURE_BUILD_FLAG_PERFORM_UPDATE) ? " (update)" : "",
                (unsigned long long)e.list, (unsigned long long)e.dest, e.flags, e.count, rtas_guard::geometry_text(e).c_str());
    if (full)
        swofty::log("[gfx] rtas build log: %u shapes named; later ones are not", RTAS_NAMED);
}

void RtasGuard::build(ID3D12GraphicsCommandList4 *l, const RtasDesc *desc, UINT posts, const PostDesc *post)
{
    if (!desc) {
        hook.call<void>(l, desc, posts, post);
        return;
    }
    bool top = desc->Inputs.Type == D3D12_RAYTRACING_ACCELERATION_STRUCTURE_TYPE_TOP_LEVEL;
    rtas_guard::Call call{top ? rtas_guard::kTop : rtas_guard::kBottom,
                          desc->DestAccelerationStructureData,
                          desc->ScratchAccelerationStructureData,
                          desc->SourceAccelerationStructureData,
                          top ? desc->Inputs.InstanceDescs : 0,
                          desc->Inputs.NumDescs,
                          (desc->Inputs.Flags & D3D12_RAYTRACING_ACCELERATION_STRUCTURE_BUILD_FLAG_PERFORM_UPDATE) != 0};
    rtas_guard::Entry entry = entry_of(l, *desc, top);
    {
        std::lock_guard<std::mutex> g(lock);
        recent.push(entry);
    }
    rtas_guard::Act a = rtas_guard::act(call);
    if (a != rtas_guard::Act::send) {
        log_hole(holes.fetch_add(1, std::memory_order_relaxed) + 1, *desc, call, a);
        if (mode == rtas_guard::Mode::on && a == rtas_guard::Act::skip)
            return;
    }
    std::vector<PostDesc> kept_post;
    if (posts && post) {
        std::vector<uint64_t> dests(posts);
        for (UINT i = 0; i < posts; i++)
            dests[i] = post[i].DestBuffer;
        std::vector<uint32_t> kept = rtas_guard::kept_posts(dests);
        if (kept.size() != posts) {
            log_bad_posts(bad_posts.fetch_add(1, std::memory_order_relaxed) + 1, *desc, posts, kept.size());
            if (mode == rtas_guard::Mode::on) {
                for (uint32_t i : kept)
                    kept_post.push_back(post[i]);
                posts = UINT(kept_post.size());
                post = kept_post.empty() ? nullptr : kept_post.data();
            }
        }
    }
    if (a == rtas_guard::Act::no_instances && mode == rtas_guard::Mode::on) {
        RtasDesc patched = *desc;
        patched.Inputs.NumDescs = 0;
        hook.call<void>(l, &patched, posts, post);
        return;
    }
    if (top || !desc->Inputs.NumDescs) {
        hook.call<void>(l, desc, posts, post);
        return;
    }
    uint64_t nth = builds.fetch_add(1, std::memory_order_relaxed);
    if (nth == 0)
        swofty::log("[gfx] rtas guard: the game builds bottom-level ray tracing structures (first: %u geometries)",
                    desc->Inputs.NumDescs);
    const auto &in = desc->Inputs;
    if (nth < trace_builds) {
        char line[1024];
        int at = snprintf(
            line, sizeof line,
            "[gfx] rtas trace %llu: dest 0x%llx src 0x%llx scratch 0x%llx flags 0x%x, %u geometries:", (unsigned long long)nth,
            (unsigned long long)desc->DestAccelerationStructureData, (unsigned long long)desc->SourceAccelerationStructureData,
            (unsigned long long)desc->ScratchAccelerationStructureData, unsigned(in.Flags), in.NumDescs);
        for (UINT i = 0; i < in.NumDescs && i < 6 && at > 0 && at < int(sizeof line) - 160; i++) {
            const GeoDesc &g = geo_at(in, i);
            if (g.Type == D3D12_RAYTRACING_GEOMETRY_TYPE_TRIANGLES)
                at += snprintf(line + at, sizeof line - at, " [vb 0x%llx s%llu n%u f%d ib 0x%llx n%u f%d t 0x%llx fl%u]",
                               (unsigned long long)g.Triangles.VertexBuffer.StartAddress,
                               (unsigned long long)g.Triangles.VertexBuffer.StrideInBytes, g.Triangles.VertexCount,
                               int(g.Triangles.VertexFormat), (unsigned long long)g.Triangles.IndexBuffer, g.Triangles.IndexCount,
                               int(g.Triangles.IndexFormat), (unsigned long long)g.Triangles.Transform3x4, unsigned(g.Flags));
            else
                at += snprintf(line + at, sizeof line - at, " [aabb 0x%llx n%llu]",
                               (unsigned long long)g.AABBs.AABBs.StartAddress, (unsigned long long)g.AABBs.AABBCount);
        }
        swofty::log("%s", line);
    }
    if (!named_full.load(std::memory_order_relaxed))
        name_build(entry, nth, *desc);
    bool any = false;
    for (UINT i = 0; i < in.NumDescs && !any; i++)
        any = rtas_guard::fault(geo_of(geo_at(in, i))) != rtas_guard::Fault::none;
    if (!any && !tracked.load(std::memory_order_acquire)) {
        hook.call<void>(l, desc, posts, post);
        return;
    }
    std::vector<rtas_guard::Geo> geos(in.NumDescs);
    for (UINT i = 0; i < in.NumDescs; i++)
        geos[i] = geo_of(geo_at(in, i));
    rtas_guard::Build b{desc->DestAccelerationStructureData, desc->SourceAccelerationStructureData,
                        (in.Flags & D3D12_RAYTRACING_ACCELERATION_STRUCTURE_BUILD_FLAG_PERFORM_UPDATE) != 0};
    rtas_guard::Plan plan;
    if (mode == rtas_guard::Mode::on) {
        std::lock_guard<std::mutex> g(lock);
        plan = book.plan(b, geos);
        tracked.store(book.tracked(), std::memory_order_release);
    } else
        for (uint32_t i = 0; i < geos.size(); i++)
            if (rtas_guard::fault(geos[i]) != rtas_guard::Fault::none) {
                plan.emptied.push_back(i);
                plan.faulty++;
            }
    if (plan.faulty)
        log_faulty(faulty.fetch_add(1, std::memory_order_relaxed) + 1, *desc, geos, plan);
    if (mode != rtas_guard::Mode::on || plan.emptied.empty()) {
        hook.call<void>(l, desc, posts, post);
        return;
    }
    std::vector<GeoDesc> copy(in.NumDescs);
    for (UINT i = 0; i < in.NumDescs; i++)
        copy[i] = geo_at(in, i);
    for (uint32_t i : plan.emptied) {
        if (copy[i].Type == D3D12_RAYTRACING_GEOMETRY_TYPE_TRIANGLES) {
            copy[i].Triangles.IndexCount = 0;
            copy[i].Triangles.VertexCount = 0;
        } else if (copy[i].Type == D3D12_RAYTRACING_GEOMETRY_TYPE_PROCEDURAL_PRIMITIVE_AABBS)
            copy[i].AABBs.AABBCount = 0;
    }
    RtasDesc patched = *desc;
    patched.Inputs.DescsLayout = D3D12_ELEMENTS_LAYOUT_ARRAY;
    patched.Inputs.pGeometryDescs = copy.data();
    hook.call<void>(l, &patched, posts, post);
}

bool RtasGuard::arm()
{
    if (!gate.offer())
        return live();
    char env[8] = {};
    GetEnvironmentVariableA("SWOFTY_RTAS_GUARD", env, sizeof env);
    mode = rtas_guard::mode(env[0] ? env : nullptr);
    char trace[16] = {};
    if (GetEnvironmentVariableA("SWOFTY_RTAS_TRACE", trace, sizeof trace))
        trace_builds = strtoull(trace, nullptr, 10);
    std::wstring settings = hud::hardware_settings::path();
    std::string rt = settings.empty() ? std::string() : rtas_guard::rt_options(runtime::read_file(settings, 1 << 20));
    swofty::log("[gfx] the game's hardware settings have ray tracing options: %s", rt.empty() ? "none found" : rt.c_str());
    if (mode == rtas_guard::Mode::off) {
        swofty::log("[gfx] rtas guard is off (SWOFTY_RTAS_GUARD=0)");
        return settle(*this, false, "SWOFTY_RTAS_GUARD=0");
    }
    ID3D12Device *dev = probe_device();
    ID3D12CommandAllocator *alloc = nullptr;
    ID3D12GraphicsCommandList *list = nullptr;
    ID3D12GraphicsCommandList4 *list4 = nullptr;
    bool ok = false;
    if (dev && SUCCEEDED(dev->CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT, IID_PPV_ARGS(&alloc))) &&
        SUCCEEDED(dev->CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, alloc, nullptr, IID_PPV_ARGS(&list))) &&
        SUCCEEDED(list->QueryInterface(IID_PPV_ARGS(&list4)))) {
        void *target = (*reinterpret_cast<void ***>(list4))[BUILD_RTAS_SLOT];
        ok = install(hook, target, reinterpret_cast<void *>(&build_rtas_hook),
                     "ID3D12GraphicsCommandList4::BuildRaytracingAccelerationStructure");
        if (ok)
            swofty::log("[gfx] rtas guard armed%s: BuildRaytracingAccelerationStructure (vt+0x%zx, in %s); builds "
                        "with a null buffer, destination, scratch, source or instance list are %s",
                        mode == rtas_guard::Mode::log ? " in log mode" : "", BUILD_RTAS_SLOT * 8, module_of(target).c_str(),
                        mode == rtas_guard::Mode::log ? "logged" : "logged and kept off address 0");
        list->Close();
    } else
        swofty::log("[gfx] rtas guard: no ID3D12GraphicsCommandList4 to hook; not armed");
    release(list4);
    release(list);
    release(alloc);
    release(dev);
    return settle(*this, ok);
}

std::string RtasGuard::recent_text(const std::vector<runtime::dred::BuildStop> &stops)
{
    std::vector<rtas_guard::ByList<64, 32>::Kept> lists;
    uint64_t pushed;
    {
        std::unique_lock<std::mutex> g(lock, std::try_to_lock);
        lists = recent.newest_lists_first();
        pushed = recent.pushed();
    }
    return rtas_guard::recent_section(lists, pushed, 12, rtas_guard::stopped_lines(lists, stops));
}

}

bool arm_rtas_guard() { return guard.arm(); }

std::string rtas_recent_text(const std::vector<runtime::dred::BuildStop> &stops) { return guard.recent_text(stops); }

}

namespace runtime {

void rtas_guard_setup() { render::d3d12::arm_rtas_guard(); }

}
