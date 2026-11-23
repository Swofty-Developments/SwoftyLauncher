#include "core/hud/toast.hpp"

#include "core/runtime/off_thread.hpp"
#include "core/runtime/runtime.hpp"
#include "core/services/lap_host.hpp"
#include "core/hud/toast_logic.hpp"

#include <windows.h>

#include <swofty/json.hpp>
#include <swofty/mixin.hpp>

#include <algorithm>
#include <ctime>
#include <mutex>
#include <string>
#include <utility>
#include <vector>

namespace hud::toast {
namespace {

constexpr ULONGLONG POLL_MS = 400;
constexpr ULONGLONG POLL_KEEP_MS = 1000;

// The launcher's messages, taken from the mailbox folders every POLL_MS on a thread of its own while a lap is recorded:
// the folder listing, the reads and the deletes never hold up a frame. The game thread hands them to the state machine.
class Mailbox {
public:
    void want() { poll.want(); }

    std::vector<Message> take()
    {
        std::lock_guard<std::mutex> g(lock);
        return std::exchange(taken, {});
    }

private:
    void sweep()
    {
        take_dir(runtime::files_root() + L"tt\\toast");
        if (runtime::files_root() != runtime::swofty_root())
            take_dir(runtime::swofty_root() + L"tt\\toast");
    }

    void take_dir(const std::wstring &dir);

    std::mutex lock;
    std::vector<Message> taken;
    runtime::OffThread poll{[this] { sweep(); }, POLL_MS, POLL_KEEP_MS};
};

void Mailbox::take_dir(const std::wstring &dir)
{
    WIN32_FIND_DATAW fd;
    HANDLE h = FindFirstFileW((dir + L"\\*.json").c_str(), &fd);
    if (h == INVALID_HANDLE_VALUE)
        return;
    std::vector<std::wstring> names;
    do {
        if (!(fd.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY))
            names.emplace_back(fd.cFileName);
    } while (FindNextFileW(h, &fd));
    FindClose(h);
    std::sort(names.begin(), names.end());
    for (const auto &n : names) {
        std::wstring path = dir + L"\\" + n;
        std::string text = runtime::read_file(path, size_t(1) << 16);
        DeleteFileW(path.c_str());
        Message m;
        Status st = parse(text, m);
        if (st != Status::ok) {
            if (!text.empty())
                swofty::log("[toast] a message was refused (status %d)", int(st));
            continue;
        }
        std::lock_guard<std::mutex> g(lock);
        taken.push_back(std::move(m));
    }
}

bool read_gate()
{
    auto doc = swofty::json::parse(runtime::read_file(runtime::swofty_root() + L"tt.json", size_t(1) << 20));
    char v[8] = {};
    bool off = GetEnvironmentVariableA("SWOFTY_TT_TOAST", v, sizeof v) && v[0] == '0';
    bool on = doc.is_object() && doc["tt_boards"].as_bool(false) && !off;
    swofty::log("[toast] lap position toasts %s", on ? "armed (tt.json tt_boards)" : "off");
    return on;
}

class LapToasts {
public:
    void tick();

private:
    bool gate_on();

    Machine machine;
    int gate = -1;
    uint64_t gate_seen = 0;
};

// Not constinit: std::deque's constructor is not constexpr.
LapToasts toasts;

// tt.json's tt_boards, read once on a thread of its own; the toasts stay off until that read is done.
bool LapToasts::gate_on()
{
    if (gate < 0) {
        static auto *file = new runtime::OffThreadValue<bool>(read_gate, 0, 0);
        bool on = false;
        if (!file->take(on, gate_seen))
            return false;
        gate = on;
    }
    return gate > 0;
}

Mailbox &mailbox()
{
    static auto *m = new Mailbox;
    return *m;
}

void LapToasts::tick()
{
    if (!gate_on())
        return;
    if (!runtime::lap_host::recording()) {
        machine.clear();
        return;
    }
    mailbox().want();
    ULONGLONG now = GetTickCount64();
    int64_t wall = int64_t(std::time(nullptr));
    for (const Message &m : mailbox().take()) {
        bool queued = machine.offer(m, wall);
        swofty::log("[toast] %s: %s on %s/%s, P%lld, %s", m.id.c_str(), queued ? "queued" : "dropped (seen, stale or queue full)",
                    m.content.c_str(), m.track.c_str(), (long long)m.position, format_time(m.lap_ms).c_str());
    }
    Snapshot s = machine.step(now, wall);
    if (s.started)
        runtime::hud_toast(title_of(s.message), detail_of(s.message), SHOW_SECONDS, "time trial boards", "fast_lap");
}

}

void tick() { toasts.tick(); }

}
