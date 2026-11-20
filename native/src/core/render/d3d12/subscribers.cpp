#include "core/render/d3d12/subscribers.hpp"

namespace render::d3d12 {

constinit Subscribers subscribers;

void on_present(PresentFn fn) { subscribers.presents.add(fn); }

void on_presented(PresentedFn fn) { subscribers.presenteds.add(fn); }

void on_resize(ResizeFn fn) { subscribers.resizes.add(fn); }

void on_exec(ExecFn fn) { subscribers.execs.add(fn); }

void on_barrier(BarrierFn fn) { subscribers.barriers.add(fn); }

void on_barrier_observe(BarrierObserverFn fn) { subscribers.barrier_observers.add(fn); }

void on_setname(SetNameFn fn) { subscribers.setnames.add(fn); }

void on_texture_copy(TextureCopyFn fn) { subscribers.texture_copies.add(fn); }

void on_render_targets(RenderTargetsFn fn) { subscribers.render_targets.add(fn); }

void on_create_rtv(RtvFn fn) { subscribers.rtvs.add(fn); }

void on_reset(ResetFn fn) { subscribers.resets.add(fn); }

void on_unread_queue(UnreadQueueFn fn) { subscribers.unread_queues.add(fn); }

}
