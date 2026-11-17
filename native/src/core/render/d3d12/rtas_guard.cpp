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

namespace d3d12hooks {

namespace {

namespace rg = runtime::rtas_guard;

constexpr size_t BUILD_RTAS_SLOT = 72;
constexpr int RTAS_DETAILED = 8;
safetyhook::InlineHook h_build_rtas;
rg::Mode rtas_mode = rg::Mode::on;
std::mutex rtas_lock;
rg::Book rtas_book;
std::atomic<size_t> rtas_tracked{0};
std::atomic<uint64_t> rtas_builds{0}, rtas_faulty{0}, rtas_holes{0}, rtas_bad_posts{0};
uint64_t rtas_trace;

using RtasDesc = D3D12_BUILD_RAYTRACING_ACCELERATION_STRUCTURE_DESC;
using GeoDesc = D3D12_RAYTRACING_GEOMETRY_DESC;
using PostDesc = D3D12_RAYTRACING_ACCELERATION_STRUCTURE_POSTBUILD_INFO_DESC;

const GeoDesc &geo_at(const D3D12_BUILD_RAYTRACING_ACCELERATION_STRUCTURE_INPUTS &in, UINT i)
{
    return in.DescsLayout == D3D12_ELEMENTS_LAYOUT_ARRAY ? in.pGeometryDescs[i] : *in.ppGeometryDescs[i];
}

rg::Geo geo_of(const GeoDesc &d)
{
    rg::Geo g;
    if (d.Type == D3D12_RAYTRACING_GEOMETRY_TYPE_TRIANGLES) {
        g.type = rg::kTriangles;
        g.vertices = d.Triangles.VertexBuffer.StartAddress;
        g.vertex_count = d.Triangles.VertexCount;
        g.indices = d.Triangles.IndexBuffer;
        g.index_count = d.Triangles.IndexCount;
        g.indexed = d.Triangles.IndexFormat != DXGI_FORMAT_UNKNOWN;
    } else if (d.Type == D3D12_RAYTRACING_GEOMETRY_TYPE_PROCEDURAL_PRIMITIVE_AABBS) {
        g.type = rg::kAabbs;
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

void log_hole(uint64_t nth, const RtasDesc &desc, const rg::Call &c, rg::Act a)
{
    if (!rg::detailed(nth, RTAS_DETAILED)) {
        if (rg::milestone(nth))
            swofty::log("[gfx] rtas guard: %llu builds so far had a null destination, scratch, source or instance list",
                        (unsigned long long)nth);
        return;
    }
    const char *what = rtas_mode == rg::Mode::log ? "sent as is (SWOFTY_RTAS_GUARD=log)" : rg::act_name(a);
    swofty::log("[gfx] rtas guard: %s build%s has %s (dest 0x%llx, scratch 0x%llx, source 0x%llx, %u %s at 0x%llx); %s",
                level_name(desc), c.update ? " (update)" : "", rg::hole_name(rg::hole(c)), (unsigned long long)c.dest,
                (unsigned long long)c.scratch, (unsigned long long)c.source, c.count,
                c.level == rg::kTop ? "instances" : "geometries", (unsigned long long)c.instances, what);
}

void log_bad_posts(uint64_t nth, const RtasDesc &desc, UINT posts, size_t kept)
{
    if (!rg::detailed(nth, RTAS_DETAILED)) {
        if (rg::milestone(nth))
            swofty::log("[gfx] rtas guard: %llu builds so far asked for postbuild info into a null buffer",
                        (unsigned long long)nth);
        return;
    }
    swofty::log("[gfx] rtas guard: %s build into 0x%llx asks for %u postbuild info(s), %zu into a null buffer; %s",
                level_name(desc), (unsigned long long)desc.DestAccelerationStructureData, posts, posts - kept,
                rtas_mode == rg::Mode::log ? "asked as is (SWOFTY_RTAS_GUARD=log)" : "those are dropped");
}

void log_faulty(uint64_t nth, const RtasDesc &desc, const std::vector<rg::Geo> &geos, const rg::Plan &plan)
{
    if (!rg::detailed(nth, RTAS_DETAILED)) {
        if (rg::milestone(nth))
            swofty::log("[gfx] rtas guard: %llu bottom-level builds so far had geometries with a null buffer",
                        (unsigned long long)nth);
        return;
    }
    uint32_t first = plan.emptied.empty() ? 0 : plan.emptied.front();
    for (uint32_t i = 0; i < geos.size(); i++)
        if (rg::fault(geos[i]) != rg::Fault::none) {
            first = i;
            break;
        }
    const rg::Geo &g = geos[first];
    const char *what = rtas_mode == rg::Mode::log
                           ? "left as is (SWOFTY_RTAS_GUARD=log); on NVIDIA under Windows the GPU reads address 0 and "
                             "the device is removed"
                           : "emptied, so the build reads nothing for them";
    swofty::log("[gfx] rtas guard: bottom-level build into 0x%llx%s (%zu geometries): %u with a null buffer%s, first #%u "
                "has %s (%u vertices at 0x%llx, %u indices at 0x%llx); %s",
                (unsigned long long)desc.DestAccelerationStructureData,
                (desc.Inputs.Flags & D3D12_RAYTRACING_ACCELERATION_STRUCTURE_BUILD_FLAG_PERFORM_UPDATE) ? " (update)" : "",
                geos.size(), plan.faulty,
                plan.kept_empty ? ", plus geometries emptied in the build it updates" : "", first,
                rg::fault_name(rg::fault(g)), g.vertex_count, (unsigned long long)g.vertices, g.index_count,
                (unsigned long long)g.indices, what);
}

void STDMETHODCALLTYPE build_rtas_hook(ID3D12GraphicsCommandList4 *l, const RtasDesc *desc, UINT posts,
                                       const PostDesc *post)
{
    if (!desc) {
        h_build_rtas.call<void>(l, desc, posts, post);
        return;
    }
    bool top = desc->Inputs.Type == D3D12_RAYTRACING_ACCELERATION_STRUCTURE_TYPE_TOP_LEVEL;
    rg::Call call{top ? rg::kTop : rg::kBottom,
                  desc->DestAccelerationStructureData,
                  desc->ScratchAccelerationStructureData,
                  desc->SourceAccelerationStructureData,
                  top ? desc->Inputs.InstanceDescs : 0,
                  desc->Inputs.NumDescs,
                  (desc->Inputs.Flags & D3D12_RAYTRACING_ACCELERATION_STRUCTURE_BUILD_FLAG_PERFORM_UPDATE) != 0};
    rg::Act a = rg::act(call);
    if (a != rg::Act::send) {
        log_hole(rtas_holes.fetch_add(1, std::memory_order_relaxed) + 1, *desc, call, a);
        if (rtas_mode == rg::Mode::on && a == rg::Act::skip)
            return;
    }
    std::vector<PostDesc> kept_post;
    if (posts && post) {
        std::vector<uint64_t> dests(posts);
        for (UINT i = 0; i < posts; i++)
            dests[i] = post[i].DestBuffer;
        std::vector<uint32_t> kept = rg::kept_posts(dests);
        if (kept.size() != posts) {
            log_bad_posts(rtas_bad_posts.fetch_add(1, std::memory_order_relaxed) + 1, *desc, posts, kept.size());
            if (rtas_mode == rg::Mode::on) {
                for (uint32_t i : kept)
                    kept_post.push_back(post[i]);
                posts = UINT(kept_post.size());
                post = kept_post.empty() ? nullptr : kept_post.data();
            }
        }
    }
    if (a == rg::Act::no_instances && rtas_mode == rg::Mode::on) {
        RtasDesc patched = *desc;
        patched.Inputs.NumDescs = 0;
        h_build_rtas.call<void>(l, &patched, posts, post);
        return;
    }
    if (top || !desc->Inputs.NumDescs) {
        h_build_rtas.call<void>(l, desc, posts, post);
        return;
    }
    uint64_t nth = rtas_builds.fetch_add(1, std::memory_order_relaxed);
    if (nth == 0)
        swofty::log("[gfx] rtas guard: the game builds bottom-level ray tracing structures (first: %u geometries)",
                    desc->Inputs.NumDescs);
    const auto &in = desc->Inputs;
    if (nth < rtas_trace) {
        char line[1024];
        int at = snprintf(line, sizeof line, "[gfx] rtas trace %llu: dest 0x%llx src 0x%llx scratch 0x%llx flags 0x%x, %u geometries:",
                          (unsigned long long)nth, (unsigned long long)desc->DestAccelerationStructureData,
                          (unsigned long long)desc->SourceAccelerationStructureData,
                          (unsigned long long)desc->ScratchAccelerationStructureData, unsigned(in.Flags), in.NumDescs);
        for (UINT i = 0; i < in.NumDescs && i < 6 && at > 0 && at < int(sizeof line) - 160; i++) {
            const GeoDesc &g = geo_at(in, i);
            if (g.Type == D3D12_RAYTRACING_GEOMETRY_TYPE_TRIANGLES)
                at += snprintf(line + at, sizeof line - at, " [vb 0x%llx s%llu n%u f%d ib 0x%llx n%u f%d t 0x%llx fl%u]",
                               (unsigned long long)g.Triangles.VertexBuffer.StartAddress,
                               (unsigned long long)g.Triangles.VertexBuffer.StrideInBytes, g.Triangles.VertexCount,
                               int(g.Triangles.VertexFormat), (unsigned long long)g.Triangles.IndexBuffer,
                               g.Triangles.IndexCount, int(g.Triangles.IndexFormat),
                               (unsigned long long)g.Triangles.Transform3x4, unsigned(g.Flags));
            else
                at += snprintf(line + at, sizeof line - at, " [aabb 0x%llx n%llu]",
                               (unsigned long long)g.AABBs.AABBs.StartAddress, (unsigned long long)g.AABBs.AABBCount);
        }
        swofty::log("%s", line);
    }
    bool any = false;
    for (UINT i = 0; i < in.NumDescs && !any; i++)
        any = rg::fault(geo_of(geo_at(in, i))) != rg::Fault::none;
    if (!any && !rtas_tracked.load(std::memory_order_acquire)) {
        h_build_rtas.call<void>(l, desc, posts, post);
        return;
    }
    std::vector<rg::Geo> geos(in.NumDescs);
    for (UINT i = 0; i < in.NumDescs; i++)
        geos[i] = geo_of(geo_at(in, i));
    rg::Build b{desc->DestAccelerationStructureData, desc->SourceAccelerationStructureData,
                (in.Flags & D3D12_RAYTRACING_ACCELERATION_STRUCTURE_BUILD_FLAG_PERFORM_UPDATE) != 0};
    rg::Plan plan;
    if (rtas_mode == rg::Mode::on) {
        std::lock_guard<std::mutex> g(rtas_lock);
        plan = rtas_book.plan(b, geos);
        rtas_tracked.store(rtas_book.tracked(), std::memory_order_release);
    } else
        for (uint32_t i = 0; i < geos.size(); i++)
            if (rg::fault(geos[i]) != rg::Fault::none) {
                plan.emptied.push_back(i);
                plan.faulty++;
            }
    if (plan.faulty)
        log_faulty(rtas_faulty.fetch_add(1, std::memory_order_relaxed) + 1, *desc, geos, plan);
    if (rtas_mode != rg::Mode::on || plan.emptied.empty()) {
        h_build_rtas.call<void>(l, desc, posts, post);
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
    h_build_rtas.call<void>(l, &patched, posts, post);
}

}

bool arm_rtas_guard()
{
    static int state;
    if (state)
        return state > 0;
    state = -1;
    char env[8] = {};
    GetEnvironmentVariableA("SWOFTY_RTAS_GUARD", env, sizeof env);
    rtas_mode = rg::mode(env[0] ? env : nullptr);
    char trace[16] = {};
    if (GetEnvironmentVariableA("SWOFTY_RTAS_TRACE", trace, sizeof trace))
        rtas_trace = strtoull(trace, nullptr, 10);
    if (rtas_mode == rg::Mode::off) {
        swofty::log("[gfx] rtas guard is off (SWOFTY_RTAS_GUARD=0)");
        return false;
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
        ok = install(h_build_rtas, target, reinterpret_cast<void *>(&build_rtas_hook),
                     "ID3D12GraphicsCommandList4::BuildRaytracingAccelerationStructure");
        if (ok)
            swofty::log("[gfx] rtas guard armed%s: BuildRaytracingAccelerationStructure (vt+0x%zx, in %s); builds "
                        "with a null buffer, destination, scratch, source or instance list are %s",
                        rtas_mode == rg::Mode::log ? " in log mode" : "", BUILD_RTAS_SLOT * 8, module_of(target).c_str(),
                        rtas_mode == rg::Mode::log ? "logged" : "logged and kept off address 0");
        list->Close();
    } else
        swofty::log("[gfx] rtas guard: no ID3D12GraphicsCommandList4 to hook; not armed");
    release(list4);
    release(list);
    release(alloc);
    release(dev);
    state = ok ? 1 : -1;
    return ok;
}

}

namespace runtime {

void rtas_guard_setup()
{
    d3d12hooks::arm_rtas_guard();
}

}
