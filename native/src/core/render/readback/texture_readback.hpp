#pragma once
#include "core/render/d3d12_hooks.hpp"

#include <wrl/client.h>

#include <cstddef>
#include <cstdint>
#include <cstring>
#include <memory>
#include <mutex>
#include <optional>
#include <string>

// GPU-to-CPU copies of a game texture, recorded into the game's own command list and finished by a fence the core
// signals after that list executes. The badge images and the driver portraits share it: a feature derives its job from
// Job, records it with copy_at_transition (or allocate plus its own copy), schedules it from its ExecuteCommandLists
// subscriber and maps it once status() says it is done. Nothing here keeps a job alive; the feature's pending list does,
// and a job's GPU objects outlive a removed device or a failed Signal until the feature drops it.
namespace render::readback {

using Microsoft::WRL::ComPtr;

struct Job {
    ComPtr<ID3D12GraphicsCommandList> list;
    ComPtr<ID3D12Resource> buffer;
    ComPtr<ID3D12Fence> fence;
    D3D12_PLACED_SUBRESOURCE_FOOTPRINT footprint{};
    unsigned width{}, height{};
    bool submitted{}, scheduled{}, signal_failed{};
    uint64_t serial{};
};

inline const char *allocate(ID3D12Resource *owner, UINT64 bytes, Job &job)
{
    ComPtr<ID3D12Device> device;
    if (!owner || FAILED(owner->GetDevice(IID_PPV_ARGS(&device))))
        return "device_unavailable";
    D3D12_HEAP_PROPERTIES heap{};
    heap.Type = D3D12_HEAP_TYPE_READBACK;
    D3D12_RESOURCE_DESC buffer{};
    buffer.Dimension = D3D12_RESOURCE_DIMENSION_BUFFER;
    buffer.Width = bytes;
    buffer.Height = buffer.DepthOrArraySize = buffer.MipLevels = 1;
    buffer.SampleDesc.Count = 1;
    buffer.Layout = D3D12_TEXTURE_LAYOUT_ROW_MAJOR;
    if (FAILED(device->CreateCommittedResource(&heap, D3D12_HEAP_FLAG_NONE, &buffer, D3D12_RESOURCE_STATE_COPY_DEST, nullptr,
                                               IID_PPV_ARGS(&job.buffer))) ||
        FAILED(device->CreateFence(0, D3D12_FENCE_FLAG_NONE, IID_PPV_ARGS(&job.fence))))
        return "readback_allocation_failed";
    return nullptr;
}

inline const char *copy_at_transition(ID3D12GraphicsCommandList *list, const D3D12_RESOURCE_BARRIER &step,
                                      const D3D12_RESOURCE_DESC &desc, Job &job)
{
    auto *resource = step.Transition.pResource;
    ComPtr<ID3D12Device> device;
    if (FAILED(resource->GetDevice(IID_PPV_ARGS(&device))))
        return "device_unavailable";
    UINT64 bytes = 0;
    device->GetCopyableFootprints(&desc, 0, 1, 0, &job.footprint, nullptr, nullptr, &bytes);
    if (const char *why = allocate(resource, bytes, job))
        return why;
    job.list = list;
    job.width = unsigned(desc.Width);
    job.height = desc.Height;
    auto move = step;
    move.Transition.StateAfter = D3D12_RESOURCE_STATE_COPY_SOURCE;
    d3d12::barrier_original(list, 1, &move);
    D3D12_TEXTURE_COPY_LOCATION from{}, to{};
    from.pResource = resource;
    from.Type = D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX;
    to.pResource = job.buffer.Get();
    to.Type = D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT;
    to.PlacedFootprint = job.footprint;
    list->CopyTextureRegion(&to, 0, 0, 0, &from, nullptr);
    std::swap(move.Transition.StateBefore, move.Transition.StateAfter);
    d3d12::barrier_original(list, 1, &move);
    return nullptr;
}

// The D3D debug name of a resource (SetName, or the narrow name GUID), ASCII only; empty for none or a non-ASCII name.
inline std::optional<std::string> debug_name(ID3D12Object *object)
{
    static const GUID name_guid = {0x4cca5fd8, 0x921f, 0x42c8, {0x85, 0x66, 0x70, 0xca, 0xf2, 0xa9, 0xb7, 0x41}};
    wchar_t name[256]{};
    UINT size = sizeof name - sizeof(wchar_t);
    if (!object)
        return {};
    if (FAILED(object->GetPrivateData(name_guid, &size, name)) || size > sizeof name - sizeof(wchar_t) ||
        size % sizeof(wchar_t)) {
        static const GUID ansi_guid = {0x429b8c22, 0x9188, 0x4b0c, {0x87, 0x42, 0xac, 0xb0, 0xbf, 0x85, 0xc2, 0x00}};
        char narrow[256]{};
        size = sizeof narrow - 1;
        if (FAILED(object->GetPrivateData(ansi_guid, &size, narrow)) || size > sizeof narrow - 1)
            return {};
        return std::string(narrow, strnlen(narrow, size));
    }
    std::string ascii;
    for (unsigned i = 0; i < size / sizeof(wchar_t) && name[i]; ++i) {
        if (name[i] > 127)
            return {};
        ascii += char(name[i]);
    }
    return ascii;
}

// The ASCII text of a SetName argument, at most 256 characters; empty for a non-ASCII name.
inline std::optional<std::string> set_name_text(LPCWSTR name)
{
    if (!name)
        return {};
    std::string text;
    for (size_t i = 0; i < 256 && name[i]; ++i) {
        if (name[i] > 127)
            return {};
        text += char(name[i]);
    }
    return text;
}

enum class Status { waiting, lost, done };

inline Status status(const Job &job)
{
    if (!job.submitted || job.signal_failed || !job.fence)
        return Status::waiting;
    const UINT64 value = job.fence->GetCompletedValue();
    if (value == UINT64_MAX)
        return Status::lost;
    return value ? Status::done : Status::waiting;
}

template <class Rows> bool map(const Job &job, Rows &&rows)
{
    if (!job.height || !job.width)
        return false;
    const size_t pitch = job.footprint.Footprint.RowPitch;
    const size_t bytes = size_t(job.footprint.Offset) + pitch * (job.height - 1) + size_t(job.width) * 4;
    uint8_t *mapped = nullptr;
    D3D12_RANGE all{0, bytes};
    if (FAILED(job.buffer->Map(0, &all, reinterpret_cast<void **>(&mapped))))
        return false;
    rows(static_cast<const uint8_t *>(mapped + job.footprint.Offset), unsigned(pitch));
    D3D12_RANGE none{0, 0};
    job.buffer->Unmap(0, &none);
    return true;
}

template <class Pending>
void schedule(Pending &pending, std::mutex &mu, uint64_t &serial, UINT n, ID3D12CommandList *const *lists, d3d12::ExecPlan &plan)
{
    for (auto &job : pending) {
        if (job->submitted || job->scheduled)
            continue;
        for (UINT i = 0; i < n; ++i)
            if (static_cast<ID3D12CommandList *>(job->list.Get()) == lists[i]) {
                job->scheduled = true;
                plan.after(i, [job, &mu, &serial](ID3D12CommandQueue *q) {
                    std::lock_guard lock(mu);
                    job->serial = ++serial;
                    job->signal_failed = FAILED(q->Signal(job->fence.Get(), 1));
                    job->submitted = true;
                });
                break;
            }
    }
}

inline bool copy_capable(ID3D12CommandQueue *queue)
{
    const auto type = queue->GetDesc().Type;
    return type == D3D12_COMMAND_LIST_TYPE_DIRECT || type == D3D12_COMMAND_LIST_TYPE_COPY ||
           type == D3D12_COMMAND_LIST_TYPE_COMPUTE;
}

}
