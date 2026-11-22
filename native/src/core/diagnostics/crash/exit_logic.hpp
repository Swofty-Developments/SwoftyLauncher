#pragma once

#include <cstdint>
#include <string>

// Pure logic of the exit hooks' crash report (capture.cpp on_exit). An exit with a non-zero code gets a report unless
// the game was asked to close: its window got WM_CLOSE or WM_ENDSESSION shortly before (the launcher's Stop or its
// restart of the game, the window's close button, a settings restart), or the launcher asked the session to stop
// after the game started. F1 25 answers WM_CLOSE with TerminateProcess(1). An exit soon after a fatal-class
// exception gets a report whatever asked for it. No Windows calls, so the unit test runs on the host.

namespace exit_logic {

// How long after its window was asked to close an exit still counts as that close.
constexpr uint64_t CLOSE_MS = 60000;

constexpr const char *STOP_FILE = "stop_requested";

enum class Verdict { Clean, Crash, Close };

struct Exit {
    uint32_t code;
    bool after_fatal;
    uint64_t now_ms;
    // GetTickCount64 when the game's window was last asked to close; 0 when it never was.
    uint64_t close_ms;
    bool stop_requested;
};

inline bool window_closing(const Exit &e) { return e.close_ms && e.close_ms <= e.now_ms && e.now_ms - e.close_ms < CLOSE_MS; }

inline Verdict verdict(const Exit &e)
{
    if (e.after_fatal)
        return Verdict::Crash;
    if (!e.code)
        return Verdict::Clean;
    return e.stop_requested || window_closing(e) ? Verdict::Close : Verdict::Crash;
}

// The stop request file of the session the launcher names in sessions\latest.txt, or empty when it names none.
inline std::string stop_file(std::string latest)
{
    auto blank = [](char c) { return c == ' ' || c == '\t' || c == '\r' || c == '\n'; };
    while (!latest.empty() && blank(latest.back()))
        latest.pop_back();
    size_t from = 0;
    while (from < latest.size() && blank(latest[from]))
        from++;
    latest.erase(0, from);
    while (!latest.empty() && (latest.back() == '\\' || latest.back() == '/'))
        latest.pop_back();
    return latest.empty() ? latest : latest + "\\" + STOP_FILE;
}

// Whether a stop request written at `written` belongs to the process that started at `started` (both FILETIME
// counts): one written before it is an earlier session's.
inline bool stop_is_ours(uint64_t written, uint64_t started) { return written && written >= started; }

}
