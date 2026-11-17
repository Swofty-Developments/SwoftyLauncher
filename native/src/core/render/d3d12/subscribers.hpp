#pragma once
#include "core/render/d3d12_hooks.hpp"

#include <atomic>

namespace d3d12hooks {

template <class Fn> struct Subs {
    Fn fns[16] = {};
    std::atomic<int> n{0};

    void add(Fn fn)
    {
        int at = n.load();
        if (at < 16) {
            fns[at] = fn;
            n.store(at + 1);
        }
    }
};

struct Subscribers {
    Subs<PresentFn> presents;
    Subs<PresentedFn> presenteds;
    Subs<ResizeFn> resizes;
    Subs<ExecFn> execs;
    Subs<BarrierFn> barriers;
    Subs<SetNameFn> setnames;
    Subs<ResetFn> resets;
    Subs<RenderTargetsFn> render_targets;
    Subs<RtvFn> rtvs;
    Subs<UnreadQueueFn> unread_queues;
};

extern Subscribers subscribers;

}
