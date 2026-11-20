#include "core/render/arm_gate_logic.hpp"

#include <atomic>
#include <chrono>
#include <cstdio>
#include <functional>
#include <mutex>
#include <string>
#include <thread>
#include <vector>

namespace ag = render::d3d12::arm_gate;
using namespace std::chrono_literals;

int fails;

void expect(bool ok, const char *what)
{
    std::printf("%s %s\n", ok ? "ok  :" : "FAIL:", what);
    fails += !ok;
}

uint32_t thread_key() { return uint32_t(std::hash<std::thread::id>{}(std::this_thread::get_id()) | 1); }

ag::Inputs ready()
{
    ag::Inputs in;
    in.early = true;
    in.hooked = true;
    return in;
}

// The order things happen in, as one boot would log them.
struct Trace {
    std::mutex lock;
    std::vector<std::string> events;

    void add(const std::string &e)
    {
        std::lock_guard<std::mutex> g(lock);
        events.push_back(e);
    }

    int at(const std::string &e)
    {
        std::lock_guard<std::mutex> g(lock);
        for (size_t i = 0; i < events.size(); i++)
            if (events[i] == e)
                return int(i);
        return -1;
    }
};

// A creation export behind the gate: what the hook's detour does before calling the real function.
struct Export {
    ag::Gate &gate;
    Trace &trace;
    std::string name;

    ag::Pass call(const std::string &who, ag::Exempt exempt = ag::Exempt::none, std::chrono::milliseconds cap = ag::WAIT_CAP)
    {
        ag::Pass p = ag::Pass::open;
        if (gate.running()) {
            uint32_t me = thread_key();
            p = gate.wait(me, gate.is_worker(me) ? ag::Exempt::none : exempt, cap);
        }
        trace.add(who + " " + name);
        return p;
    }
};

struct FakePlatform {
    bool hooks_take = true, thread_starts = true;
    int hooked = 0, unhooked = 0, factories_unhooked = 0, made = 0, resumed = 0;
    std::thread worker;
    std::atomic<bool> go{false};
    std::function<void()> body;
    uint32_t worker_id = 0;

    bool hook()
    {
        hooked++;
        return hooks_take;
    }

    bool make_thread(uint32_t &id)
    {
        if (!thread_starts)
            return false;
        made++;
        std::atomic<uint32_t> got{0};
        worker = std::thread([this, &got] {
            got.store(thread_key());
            while (!go.load())
                std::this_thread::yield();
            body();
        });
        while (!got.load())
            std::this_thread::yield();
        id = worker_id = got.load();
        return true;
    }

    void resume()
    {
        resumed++;
        go.store(true);
    }

    void unhook() { unhooked++; }
    void unhook_factories() { factories_unhooked++; }
};

ag::Gate *the_gate;
Trace *the_trace;
FakePlatform *the_platform;
std::atomic<bool> game_calling{false};

void block()
{
    Export device{*the_gate, *the_trace, "D3D12CreateDevice"};
    Export factory{*the_gate, *the_trace, "CreateDXGIFactory1"};
    the_trace->add("worker starts");
    ag::Pass dev = device.call("worker");
    the_trace->add(dev == ag::Pass::worker ? "probe device made, the worker passed"
                                           : "probe device made, the worker did not pass");
    std::this_thread::sleep_for(60ms);
    ag::Pass fac = factory.call("worker");
    the_trace->add(fac == ag::Pass::worker ? "probe factory made, the worker passed"
                                           : "probe factory made, the worker did not pass");
    std::this_thread::sleep_for(60ms);
    for (int i = 0; i < 2000 && !game_calling.load(); i++)
        std::this_thread::sleep_for(1ms);
    std::this_thread::sleep_for(50ms);
    the_trace->add("hooks in");
    the_trace->add("probe device released");
}

void test_choose()
{
    expect(ag::choose(ready()).worker, "with nothing against it the arm runs on a worker");
    ag::Inputs in = ready();
    in.sync_env = "1";
    ag::Choice c = ag::choose(in);
    expect(!c.worker && c.why == "SWOFTY_ARM_SYNC=1", "SWOFTY_ARM_SYNC=1 runs the arm in the stage and says so");
    in.sync_env = "0";
    expect(ag::choose(in).worker, "only SWOFTY_ARM_SYNC=1 forces the stage");
    in = ready();
    in.rival = "RTSSHooks64.dll";
    c = ag::choose(in);
    expect(!c.worker && c.why.find("RTSSHooks64.dll") == 0, "a rival that patches D3D12 entry points keeps the arm in the stage");
    in = ready();
    in.early = false;
    expect(!ag::choose(in).worker, "past the early stage the game may hold a factory, so the arm stays in the stage");
    in = ready();
    in.loader_lock = true;
    expect(!ag::choose(in).worker, "a stage that holds the loader lock keeps the arm in the stage");
    in = ready();
    in.hooked = false;
    c = ag::choose(in);
    expect(!c.worker && c.why == "a creation export did not hook", "an export that did not hook keeps the arm in the stage");
}

void test_system_module()
{
    expect(ag::system_module(L"C:\\Windows\\System32\\dxgi.dll", L"C:\\Windows", L"D:\\F1 25\\D3D12\\"),
           "dxgi.dll in System32 is a system module");
    expect(ag::system_module(L"c:\\windows\\system32\\DriverStore\\FileRepository\\nv\\nvwgf2umx.dll", L"C:\\Windows", L""),
           "the driver in the driver store counts, whatever the case");
    expect(ag::system_module(L"D:\\F1 25\\D3D12\\D3D12Core.dll", L"C:\\Windows", L"D:\\F1 25\\D3D12\\"),
           "the game's D3D12 runtime counts");
    expect(!ag::system_module(L"D:\\F1 25\\F1_25.exe", L"C:\\Windows", L"D:\\F1 25\\D3D12\\"), "the game does not");
    expect(!ag::system_module(L"D:\\F1 25\\sl.interposer.dll", L"C:\\Windows", L"D:\\F1 25\\D3D12\\"),
           "the Streamline interposer the game loads does not");
    expect(!ag::system_module(L"C:\\WindowsApps\\overlay.dll", L"C:\\Windows", L""),
           "a folder that only starts with the Windows folder's name does not");
    expect(!ag::system_module(L"C:\\Windows\\System32\\dxgi.dll", L"", L""), "with no folders known nothing counts");
    expect(ag::system_module(L"C:/Windows/System32/d3d12.dll", L"C:\\Windows", L""), "forward slashes compare as backslashes");
}

void test_gate_waits()
{
    ag::Gate gate;
    Trace trace;
    FakePlatform platform;
    the_gate = &gate, the_trace = &trace, the_platform = &platform;
    Export factory{gate, trace, "CreateDXGIFactory2"};
    Export device{gate, trace, "D3D12CreateDevice"};

    expect(factory.call("game early") == ag::Pass::open, "before the worker is scheduled every call is open");
    expect(!gate.is_worker(thread_key()), "before it is scheduled no thread is the worker");

    platform.body = [&] { ag::run_worker(gate, block, platform); };
    ag::Choice c = ag::begin(ready(), gate, platform);
    expect(c.worker && platform.hooked == 1 && platform.made == 1 && platform.resumed == 1 && platform.unhooked == 0,
           "the worker starts once the hooks are in, and nothing comes out");

    std::this_thread::sleep_for(10ms);
    expect(gate.running(), "the gate is running while the worker runs");
    ag::Pass game_factory = ag::Pass::open, game_device = ag::Pass::open;
    std::thread game([&] {
        game_calling.store(true);
        game_factory = factory.call("game");
        game_device = device.call("game");
    });
    auto from = std::chrono::steady_clock::now();
    ag::Pass driver = factory.call("driver", ag::Exempt::system);
    ag::Pass core = factory.call("core", ag::Exempt::core);
    ag::Pass locked = device.call("loader lock holder", ag::Exempt::loader_lock);
    double quick = std::chrono::duration<double, std::milli>(std::chrono::steady_clock::now() - from).count();
    expect(driver == ag::Pass::system && core == ag::Pass::core && locked == ag::Pass::loader_lock && quick < 50,
           "the runtimes and driver, the core and a loader lock holder pass straight through while the worker runs");
    game.join();
    platform.worker.join();

    expect(game_factory == ag::Pass::waited && game_device == ag::Pass::open,
           "the game's CreateDXGIFactory made before the worker is done waits for it; its device call then finds it open");
    expect(trace.at("probe device made, the worker passed") >= 0 && trace.at("probe factory made, the worker passed") >= 0,
           "the worker's own calls pass straight through");
    int released = trace.at("probe device released"), hooks = trace.at("hooks in");
    int game_fac = trace.at("game CreateDXGIFactory2"), game_dev = trace.at("game D3D12CreateDevice");
    expect(hooks >= 0 && released > hooks && game_fac > released && game_dev > game_fac,
           "the game's factory and device come after every hook is in and the probe device is released");
    expect(trace.at("driver CreateDXGIFactory2") < released && trace.at("core CreateDXGIFactory2") < released,
           "the exempt calls went before the worker was done");
    expect(!gate.running() && platform.factories_unhooked == 1 && platform.unhooked == 0,
           "once the worker is done the gate is open and only the factory hooks came out");
    expect(factory.call("game later") == ag::Pass::open, "a call after the worker is done is open");
}

void test_game_after_worker()
{
    ag::Gate gate;
    Trace trace;
    FakePlatform platform;
    the_gate = &gate, the_trace = &trace, the_platform = &platform;
    platform.body = [&] { ag::run_worker(gate, block, platform); };
    expect(ag::begin(ready(), gate, platform).worker, "the worker starts");
    platform.worker.join();
    Export device{gate, trace, "D3D12CreateDevice"};
    expect(device.call("game") == ag::Pass::open && trace.at("game D3D12CreateDevice") > trace.at("probe device released"),
           "a game that reaches its device after the worker is done does not wait");
}

void test_arm_inputs()
{
    ag::Gate gate;
    Trace trace;
    FakePlatform platform;
    the_gate = &gate, the_trace = &trace, the_platform = &platform;
    platform.body = [&] { ag::run_worker(gate, block, platform); };
    ag::Inputs in;
    in.early = true;
    ag::Choice c = ag::begin(in, gate, platform);
    expect(c.worker && platform.hooked == 1 && platform.made == 1,
           "inputs that have not hooked yet, as the arm passes them, reach the hook step and start the worker");
    platform.worker.join();
}

void test_cap()
{
    ag::Gate gate;
    gate.schedule(thread_key() + 2);
    double waited = 0;
    auto from = std::chrono::steady_clock::now();
    ag::Pass p = gate.wait(thread_key(), ag::Exempt::none, 80ms, &waited);
    double took = std::chrono::duration<double, std::milli>(std::chrono::steady_clock::now() - from).count();
    expect(p == ag::Pass::timed_out && waited >= 75 && took < 2000, "a wait stops at its cap and goes on without the arm");
    gate.finish();
    expect(gate.wait(thread_key(), ag::Exempt::none, 80ms, &waited) == ag::Pass::open && waited == 0,
           "after finish nothing waits");
}

void test_fallback()
{
    {
        ag::Gate gate;
        FakePlatform platform;
        platform.hooks_take = false;
        ag::Choice c = ag::begin(ready(), gate, platform);
        expect(!c.worker && c.why == "a creation export did not hook" && platform.made == 0 && platform.unhooked == 1,
               "an export hook that fails: no worker, every hook comes out, the stage runs the arm");
        expect(!gate.running() && gate.wait(thread_key(), ag::Exempt::none, 10ms) == ag::Pass::open,
               "after that fallback the gate holds nothing");
    }
    {
        ag::Gate gate;
        FakePlatform platform;
        platform.thread_starts = false;
        ag::Choice c = ag::begin(ready(), gate, platform);
        expect(!c.worker && c.why == "its worker thread did not start" && platform.hooked == 1 && platform.unhooked == 1,
               "a worker that does not start: every hook comes out, the stage runs the arm");
        expect(!gate.running(), "and the gate is not left running");
    }
    {
        ag::Gate gate;
        FakePlatform platform;
        ag::Inputs in = ready();
        in.sync_env = "1";
        ag::Choice c = ag::begin(in, gate, platform);
        expect(!c.worker && platform.hooked == 0 && platform.made == 0, "SWOFTY_ARM_SYNC=1 hooks nothing and starts no worker");
    }
    {
        ag::Gate gate;
        FakePlatform platform;
        ag::Inputs in = ready();
        in.rival = "RTSSHooks64.dll (RivaTuner Statistics Server is running)";
        expect(!ag::begin(in, gate, platform).worker && platform.hooked == 0,
               "a rival is checked before any export hook goes in");
    }
}

int main()
{
    test_choose();
    test_system_module();
    test_gate_waits();
    test_game_after_worker();
    test_arm_inputs();
    test_cap();
    test_fallback();
    if (fails) {
        std::printf("FAILED: %d\n", fails);
        return 1;
    }
    std::printf("all arm gate tests passed\n");
    return 0;
}
