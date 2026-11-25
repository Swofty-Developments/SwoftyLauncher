#include "core/render/d3d12/subscribers.hpp"

#include <swofty/d3d12.hpp>

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

namespace {

constinit SRWLOCK observer_lock = SRWLOCK_INIT;
constinit swofty::d3d12::ListObserver observer{};
constinit void *observer_user = nullptr;
constinit bool ready = false;
constinit thread_local ID3D12GraphicsCommandList *active_list = nullptr;
constinit thread_local bool recording = false;

struct CallbackList {
    ID3D12GraphicsCommandList *previous;
    explicit CallbackList(ID3D12GraphicsCommandList *list) : previous(active_list) { active_list = list; }
    ~CallbackList() { active_list = previous; }
};

void observer_reset(ID3D12GraphicsCommandList *list)
{
    if (!recording)
        observer.reset(observer_user, list);
}

void observer_barriers(ID3D12GraphicsCommandList *list, UINT count, const D3D12_RESOURCE_BARRIER *entries)
{
    if (recording)
        return;
    CallbackList scope(list);
    observer.barriers(observer_user, list, count, entries);
}

void observer_copy(ID3D12GraphicsCommandList *list, const D3D12_TEXTURE_COPY_LOCATION *dst, UINT x, UINT y, UINT z,
                   const D3D12_TEXTURE_COPY_LOCATION *src, const D3D12_BOX *box)
{
    if (recording)
        return;
    CallbackList scope(list);
    observer.texture_copy(observer_user, list, dst, x, y, z, src, box);
}

void observer_submit(ID3D12CommandQueue *queue, UINT count, ID3D12CommandList *const *lists, ExecPlan &plan)
{
    if (recording)
        return;
    const swofty::d3d12::ListSubmission submission{sizeof submission, count, queue, lists};
    observer.submit(observer_user, &submission);
    if (count && observer.submitted)
        plan.at_end([](ID3D12CommandQueue *q) { observer.submitted(observer_user, q); });
}

}

void list_observer_setup()
{
    AcquireSRWLockExclusive(&observer_lock);
    ready = true;
    ReleaseSRWLockExclusive(&observer_lock);

    if (observer.reset)
        on_reset(observer_reset);
    if (observer.barriers)
        on_barrier_observe(observer_barriers);
    if (observer.texture_copy)
        on_texture_copy(observer_copy);
    if (observer.submit)
        on_exec(observer_submit);
    if (observer.reset || observer.barriers || observer.texture_copy)
        arm_list_state();
    if (observer.barriers || observer.texture_copy || observer.submit)
        arm_device();
}

extern "C" __declspec(dllexport) int SwoftyCoreD3d12ObserveListsV1(const swofty::d3d12::ListObserver *callbacks, void *user)
{
    if (!callbacks || callbacks->size != sizeof observer || callbacks->reserved ||
        !(callbacks->reset || callbacks->barriers || callbacks->texture_copy || callbacks->submit) ||
        (callbacks->submitted && !callbacks->submit))
        return -1;
    AcquireSRWLockExclusive(&observer_lock);
    const int result = ready ? 1 : observer.size ? -2 : 0;
    if (!result) {
        observer = *callbacks;
        observer_user = user;
    }
    ReleaseSRWLockExclusive(&observer_lock);
    return result;
}

extern "C" __declspec(dllexport) int SwoftyCoreD3d12RecordComputeV1(void *list, swofty::d3d12::RecordComputeFn fn, void *user)
{
    if (!list || list != active_list || !fn || recording || active_list->GetType() != D3D12_COMMAND_LIST_TYPE_DIRECT)
        return 0;
    ListState state;
    if (!list_state(active_list, state) || !state.heaps)
        return 0;
    recording = true;
    fn(user, list);
    restore_list_state(active_list, state);
    recording = false;
    return 1;
}

}
