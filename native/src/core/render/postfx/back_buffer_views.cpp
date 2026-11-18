#include "core/render/postfx/back_buffer_views.hpp"

namespace render::postfx {

void BackBufferViews::named(ID3D12Resource *res)
{
    for (auto &v : recent_views)
        if (v.res.load(std::memory_order_relaxed) == res)
            pin(v.at.load(std::memory_order_relaxed), res);
}

void BackBufferViews::pin(SIZE_T at, ID3D12Resource *res)
{
    if (!at)
        return;
    for (auto &v : back_views)
        if (v.at.load(std::memory_order_relaxed) == at) {
            v.res.store(res, std::memory_order_relaxed);
            return;
        }
    if (!res)
        return;
    for (auto &v : back_views) {
        SIZE_T none = 0;
        if (v.at.compare_exchange_strong(none, at)) {
            v.res.store(res, std::memory_order_relaxed);
            SIZE_T lo = views_lo.load(std::memory_order_relaxed), hi = views_hi.load(std::memory_order_relaxed);
            while (at < lo && !views_lo.compare_exchange_weak(lo, at))
                ;
            while (at > hi && !views_hi.compare_exchange_weak(hi, at))
                ;
            return;
        }
    }
}

void BackBufferViews::created(ID3D12Resource *res, D3D12_CPU_DESCRIPTOR_HANDLE at, const ResourceWatch &watched)
{
    ViewAt &r = recent_views[recent_next.fetch_add(1, std::memory_order_relaxed) % 64];
    r.at.store(at.ptr, std::memory_order_relaxed);
    r.res.store(res, std::memory_order_relaxed);
    pin(at.ptr, res && (watched.bits(res) & W_BACK) ? res : nullptr);
}

ID3D12Resource *BackBufferViews::bound(SIZE_T at) const
{
    if (at < views_lo.load(std::memory_order_relaxed) || at > views_hi.load(std::memory_order_relaxed))
        return nullptr;
    for (auto &v : back_views)
        if (v.at.load(std::memory_order_relaxed) == at)
            return v.res.load(std::memory_order_relaxed);
    return nullptr;
}

}
