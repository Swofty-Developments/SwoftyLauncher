#pragma once

#include <atomic>
#include <chrono>
#include <condition_variable>
#include <cstdint>
#include <cwctype>
#include <mutex>
#include <string>
#include <string_view>

// The D3D12 arm (the ready subsystems from "hud arm" to "rtas guard": the overlay's present hook, postfx, grade, srv
// clamp and the rtas guard, around one probe device) can run on a worker while the stage returns to the game. The
// creation exports the game makes its first device and factory with (D3D12CreateDevice, CreateDXGIFactory,
// CreateDXGIFactory1, CreateDXGIFactory2) are hooked first, and the gate holds the game's calls to them until the
// worker has finished, so the game's first device and factory exist only after every hook is in and the probe
// device is released, as when the arm runs in the stage.
//
// A call goes straight through when it comes from the worker, from a thread that holds the loader lock (the worker
// may need it to load the driver), from the core's own code on another thread, or from code under the Windows
// folder or the game's D3D12 folder (the D3D12 and DXGI runtimes and the driver, whose threads the worker's device
// may wait for). Any other caller is the game or a module it loaded, and waits.

namespace render::d3d12::arm_gate {

inline constexpr const char *SYNC_ENV = "SWOFTY_ARM_SYNC";
inline constexpr std::chrono::milliseconds WAIT_CAP{30000};

struct Inputs {
    std::string_view sync_env; // SWOFTY_ARM_SYNC as read, empty when unset
    std::string_view rival;    // an overlay that patches D3D12 entry points, in the process or running
    bool early = false;        // the core loaded at the early stage and the window stage has not come
    bool loader_lock = false;  // the stage holds the loader lock
    bool hooked = false;       // every creation export the process has took its hook
};

// Where the arm runs, and why it runs in the stage when it does.
struct Choice {
    bool worker = false;
    std::string why;
};

inline Choice choose(const Inputs &in)
{
    if (in.sync_env == "1")
        return {false, std::string(SYNC_ENV) + "=1"};
    if (!in.rival.empty())
        return {false, std::string(in.rival) + " patches D3D12 entry points"};
    if (!in.early)
        return {false, "the core did not load at the early stage, or the window stage has come, so the game may hold "
                       "a DXGI factory or device already"};
    if (in.loader_lock)
        return {false, "the stage holds the loader lock"};
    if (!in.hooked)
        return {false, "a creation export did not hook"};
    return {true, {}};
}

// Why a call may go straight through while the worker runs.
enum class Exempt { none, loader_lock, core, system };

enum class Pass { open, worker, loader_lock, core, system, waited, timed_out };

inline const char *pass_name(Pass p)
{
    switch (p) {
    case Pass::open:
        return "the arm had finished";
    case Pass::worker:
        return "the arm's own call";
    case Pass::loader_lock:
        return "passed: its thread holds the loader lock";
    case Pass::core:
        return "passed: the core's own call";
    case Pass::system:
        return "passed: a Windows, D3D12 runtime or driver module's call";
    case Pass::waited:
        return "waited for the arm";
    case Pass::timed_out:
        return "waited the most it may and went on without the arm";
    }
    return "?";
}

inline bool under(std::wstring_view path, std::wstring_view dir)
{
    if (dir.empty() || path.size() <= dir.size())
        return false;
    for (size_t i = 0; i < dir.size(); i++) {
        wchar_t a = path[i] == L'/' ? L'\\' : path[i], b = dir[i] == L'/' ? L'\\' : dir[i];
        if (std::towlower(a) != std::towlower(b))
            return false;
    }
    return dir.back() == L'\\' || dir.back() == L'/' || path[dir.size()] == L'\\' || path[dir.size()] == L'/';
}

// A caller in a module under the Windows folder or the game's D3D12 folder: the runtimes and the driver.
inline bool system_module(std::wstring_view module_path, std::wstring_view windows_dir, std::wstring_view d3d12_dir)
{
    return under(module_path, windows_dir) || under(module_path, d3d12_dir);
}

// Open until schedule() and again from finish(). In between, a call from the worker or an exempt one passes and any
// other waits for finish(), for at most the cap it is given; one that waits that long goes on (timed_out).
class Gate {
public:
    void schedule(uint32_t worker)
    {
        std::lock_guard<std::mutex> g(lock_);
        worker_ = worker;
        state_.store(RUNNING);
    }

    void finish()
    {
        {
            std::lock_guard<std::mutex> g(lock_);
            state_.store(DONE);
        }
        done_.notify_all();
    }

    bool running() const { return state_.load() == RUNNING; }

    // Read after running() was true: the worker's id is written before the state turns to running.
    bool is_worker(uint32_t thread) const { return state_.load() != IDLE && thread == worker_; }

    Pass wait(uint32_t thread, Exempt exempt, std::chrono::milliseconds cap, double *waited_ms = nullptr)
    {
        if (waited_ms)
            *waited_ms = 0;
        if (state_.load() != RUNNING)
            return Pass::open;
        std::unique_lock<std::mutex> g(lock_);
        if (state_.load() != RUNNING)
            return Pass::open;
        if (thread == worker_)
            return Pass::worker;
        if (exempt == Exempt::loader_lock)
            return Pass::loader_lock;
        if (exempt == Exempt::core)
            return Pass::core;
        if (exempt == Exempt::system)
            return Pass::system;
        auto from = std::chrono::steady_clock::now();
        bool done = done_.wait_for(g, cap, [&] { return state_.load() != RUNNING; });
        if (waited_ms)
            *waited_ms = std::chrono::duration<double, std::milli>(std::chrono::steady_clock::now() - from).count();
        return done ? Pass::waited : Pass::timed_out;
    }

private:
    static constexpr int IDLE = 0, RUNNING = 1, DONE = 2;
    std::mutex lock_;
    std::condition_variable done_;
    std::atomic<int> state_{IDLE};
    uint32_t worker_ = 0;
};

// What starting the arm on a worker does, with the platform's parts given: hook() puts the creation export hooks
// in and says whether every one took, make_thread(id) makes the worker suspended, resume() lets it run, and
// unhook() takes every export hook out again. The worker is scheduled on the gate before it runs, so its own calls
// pass from its first instruction. A choice for the stage leaves no hook in and the gate open: the caller runs the
// block itself, as before the gate.
template <class Platform> Choice begin(Inputs in, Gate &gate, Platform &p)
{
    in.hooked = true;
    Choice c = choose(in);
    if (c.worker) {
        in.hooked = p.hook();
        c = choose(in);
    }
    if (c.worker) {
        uint32_t id = 0;
        if (p.make_thread(id)) {
            gate.schedule(id);
            p.resume();
        } else
            c = {false, "its worker thread did not start"};
    }
    if (!c.worker)
        p.unhook();
    return c;
}

// The worker: the block, with the probe device made and released inside it, then the gate opens and the factory
// export hooks come out (the device export's stays until the game's first call through it).
template <class Platform> void run_worker(Gate &gate, void (*block)(), Platform &p)
{
    block();
    gate.finish();
    p.unhook_factories();
}

}
