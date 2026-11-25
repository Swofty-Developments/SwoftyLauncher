// Windows test. Link with src/core/render/d3d12/subscribers.cpp and d3d12.lib.
#include "core/render/d3d12/subscribers.hpp"
#include <swofty/d3d12.hpp>

#include <wrl/client.h>

#include <cstdio>
#include <cstdlib>
#include <vector>

namespace native = render::d3d12;
namespace sdk = swofty::d3d12;
using Microsoft::WRL::ComPtr;

extern "C" int SwoftyCoreD3d12ObserveListsV1(const sdk::ListObserver *, void *);
extern "C" int SwoftyCoreD3d12RecordComputeV1(void *, sdk::RecordComputeFn, void *);

namespace {

enum Event { reset, barrier, compute, restored, copy, submit, original, submitted };
std::vector<Event> events;
bool device_armed = false, state_armed = false, have_snapshot = true;
int context = 0;
ID3D12GraphicsCommandList *expected_list = nullptr;
ID3D12CommandList *expected_batch[1]{};
D3D12_RESOURCE_BARRIER entries[1]{};
D3D12_TEXTURE_COPY_LOCATION source{}, target{};

void check(bool passed, const char *message)
{
    if (!passed) {
        std::fprintf(stderr, "%s\n", message);
        std::exit(1);
    }
}

void record(void *user, void *list)
{
    check(user == &context && list == expected_list, "recording context was lost");
    events.push_back(compute);
    check(!SwoftyCoreD3d12RecordComputeV1(list, record, user), "nested recording was accepted");
    native::subscribers.barrier_observers.fns[0](expected_list, 1, entries);
    native::subscribers.texture_copies.fns[0](expected_list, &target, 0, 0, 0, &source, nullptr);
}

void on_reset(void *user, void *list)
{
    check(user == &context && list == expected_list, "reset did not forward its arguments");
    check(!SwoftyCoreD3d12RecordComputeV1(list, record, user), "recording from reset was accepted");
    events.push_back(reset);
}

void on_barriers(void *user, void *list, uint32_t count, const void *batch)
{
    check(user == &context && list == expected_list && count == 1 && batch == entries, "barrier arguments did not pass through");
    events.push_back(barrier);
    check(!SwoftyCoreD3d12RecordComputeV1(user, record, user), "recording on the wrong active list was accepted");
    check(bool(SwoftyCoreD3d12RecordComputeV1(list, record, user)) == have_snapshot, "snapshot gate did not apply");
}

void on_copy(void *user, void *list, const void *dst, uint32_t x, uint32_t y, uint32_t z, const void *src, const void *box)
{
    check(user == &context && list == expected_list && dst == &target && src == &source && x == 1 && y == 2 && z == 3 && !box,
          "copy arguments did not pass through");
    events.push_back(copy);
    check(SwoftyCoreD3d12RecordComputeV1(list, record, user) == 1, "recording from copy failed");
}

void on_submit(void *user, const sdk::ListSubmission *submission)
{
    check(user == &context && submission->size == sizeof *submission && submission->count == 1 &&
              submission->lists == expected_batch,
          "submission arguments did not pass through");
    events.push_back(submit);
}

void on_submitted(void *user, void *queue)
{
    check(user == &context && !queue, "submitted arguments did not pass through");
    events.push_back(submitted);
}

}

bool native::arm_device() { return device_armed = true; }
bool native::arm_list_state() { return state_armed = true; }

bool native::list_state(ID3D12GraphicsCommandList *list, ListState &state)
{
    check(list == expected_list, "state read for unexpected list");
    state.heaps = 1;
    return have_snapshot;
}

void native::restore_list_state(ID3D12GraphicsCommandList *list, const ListState &state)
{
    check(list == expected_list && state.heaps == 1, "binding snapshot was not restored");
    events.push_back(restored);
}

int main()
{
    ComPtr<ID3D12Device> device;
    ComPtr<ID3D12CommandAllocator> allocator;
    ComPtr<ID3D12GraphicsCommandList> list;
    check(SUCCEEDED(D3D12CreateDevice(nullptr, D3D_FEATURE_LEVEL_11_0, IID_PPV_ARGS(&device))) &&
              SUCCEEDED(device->CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT, IID_PPV_ARGS(&allocator))) &&
              SUCCEEDED(
                  device->CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, allocator.Get(), nullptr, IID_PPV_ARGS(&list))),
          "could not create a D3D12 test list");
    expected_list = list.Get();
    expected_batch[0] = expected_list;

    const sdk::ListObserver observer{sizeof observer, 0, on_reset, on_barriers, on_copy, on_submit, on_submitted};
    auto invalid = observer;
    invalid.submit = nullptr;
    check(SwoftyCoreD3d12ObserveListsV1(&invalid, &context) == -1, "invalid observer was accepted");
    check(SwoftyCoreD3d12ObserveListsV1(&observer, &context) == 0, "observer registration failed");
    check(SwoftyCoreD3d12ObserveListsV1(&observer, &context) == -2, "second observer was accepted");
    native::list_observer_setup();
    check(device_armed && state_armed, "observer hooks were not armed");
    check(SwoftyCoreD3d12ObserveListsV1(&observer, &context) == 1, "late observer was accepted");
    check(!SwoftyCoreD3d12RecordComputeV1(expected_list, record, &context), "recording outside a callback was accepted");

    native::subscribers.resets.fns[0](expected_list);
    native::subscribers.barrier_observers.fns[0](expected_list, 1, entries);
    native::subscribers.texture_copies.fns[0](expected_list, &target, 1, 2, 3, &source, nullptr);
    have_snapshot = false;
    native::subscribers.barrier_observers.fns[0](expected_list, 1, entries);
    native::ExecPlan plan;
    native::subscribers.execs.fns[0](nullptr, 1, expected_batch, plan);
    native::run_plan(plan, static_cast<ID3D12CommandQueue *>(nullptr), 1, expected_batch,
                     [](auto *, unsigned, auto *) { events.push_back(original); });
    check(events == std::vector<Event>{reset, barrier, compute, restored, copy, compute, restored, barrier, submit, original,
                                       submitted},
          "callback order or suppression was incorrect");
    std::puts("d3d12_list_observer: ok");
}
