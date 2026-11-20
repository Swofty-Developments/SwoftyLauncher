#include "core/hud/toast.hpp"

#include "core/runtime/runtime.hpp"
#include "core/services/lap_host.hpp"
#include "core/hud/toast_logic.hpp"

#include <windows.h>

#include <swofty/json.hpp>
#include <swofty/mixin.hpp>

#include <algorithm>
#include <ctime>
#include <string>
#include <vector>

namespace hud::toast {
namespace {

constexpr ULONGLONG POLL_MS = 400;

class LapToasts {
public:
    void tick();

private:
    bool gate_on();
    void take(const std::wstring &dir);

    Machine machine;
    ULONGLONG last_poll = 0;
    int gate = -1;
};

// Not constinit: std::deque's constructor is not constexpr.
LapToasts toasts;

bool LapToasts::gate_on()
{
    if (gate < 0) {
        auto doc = swofty::json::parse(runtime::read_file(runtime::swofty_root() + L"tt.json", size_t(1) << 20));
        char v[8] = {};
        bool off = GetEnvironmentVariableA("SWOFTY_TT_TOAST", v, sizeof v) && v[0] == '0';
        gate = doc.is_object() && doc["tt_boards"].as_bool(false) && !off;
        swofty::log("[toast] lap position toasts %s", gate ? "armed (tt.json tt_boards)" : "off");
    }
    return gate > 0;
}

void LapToasts::take(const std::wstring &dir)
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
    int64_t now = int64_t(std::time(nullptr));
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
        bool taken = machine.offer(m, now);
        swofty::log("[toast] %s: %s on %s/%s, P%lld, %s", m.id.c_str(), taken ? "queued" : "dropped (seen, stale or queue full)",
                    m.content.c_str(), m.track.c_str(), (long long)m.position, format_time(m.lap_ms).c_str());
    }
}

void LapToasts::tick()
{
    if (!gate_on())
        return;
    if (!runtime::lap_host::recording()) {
        machine.clear();
        return;
    }
    ULONGLONG now = GetTickCount64();
    if (now - last_poll >= POLL_MS) {
        last_poll = now;
        take(runtime::files_root() + L"tt\\toast");
        if (runtime::files_root() != runtime::swofty_root())
            take(runtime::swofty_root() + L"tt\\toast");
    }
    Snapshot s = machine.step(now, int64_t(std::time(nullptr)));
    if (s.started)
        runtime::hud_toast(title_of(s.message), detail_of(s.message), SHOW_SECONDS, "time trial boards", "fast_lap");
}

}

void tick() { toasts.tick(); }

}
