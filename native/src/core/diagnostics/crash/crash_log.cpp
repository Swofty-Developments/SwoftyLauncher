#include "core/diagnostics/crash/crash_log.hpp"

#include <cstdarg>
#include <cstdio>
#include <cstring>

#include "core/diagnostics/crash/process.hpp"
#include "loader/data_folder.h"

namespace runtime {
void load_stall_game_line(const char *line);
}

namespace crash {

constinit CrashLog logger{};
constinit Recorder recorder{};

std::wstring data_folder()
{
    wchar_t buf[MAX_PATH * 2];
    DWORD n = swofty_data_folder(buf, MAX_PATH * 2);
    return std::wstring(buf, n);
}

std::string fmt(const char *f, ...)
{
    char b[512];
    va_list ap;
    va_start(ap, f);
    vsnprintf(b, sizeof b, f, ap);
    va_end(ap);
    return b;
}

void diag_line(const std::string &s)
{
    if (!process.ea_build || logger.diag_path.empty())
        return;
    HANDLE f = CreateFileW(logger.diag_path.c_str(), FILE_APPEND_DATA, FILE_SHARE_READ | FILE_SHARE_WRITE, nullptr, OPEN_ALWAYS,
                           FILE_ATTRIBUTE_NORMAL, nullptr);
    if (f == INVALID_HANDLE_VALUE)
        return;
    SYSTEMTIME t;
    GetLocalTime(&t);
    char line[1200];
    int n = snprintf(line, sizeof line, "%04u-%02u-%02u %02u:%02u:%02u.%03u [%5lu] %s\r\n", t.wYear, t.wMonth, t.wDay, t.wHour,
                     t.wMinute, t.wSecond, t.wMilliseconds, GetCurrentThreadId(), s.c_str());
    if (n > 0) {
        DWORD w;
        WriteFile(f, line, (DWORD)(n > (int)sizeof line ? (int)sizeof line : n), &w, nullptr);
    }
    CloseHandle(f);
}

/* Like api->log, but on a non-matching (EA/other) build also mirrors the line into the dedicated
   swofty\ea-diagnostics.log, so the friend's single file carries the symbolised crash report. */
void cl(const char *f, ...)
{
    char b[1024];
    va_list ap;
    va_start(ap, f);
    vsnprintf(b, sizeof b, f, ap);
    va_end(ap);
    logger.api->log("%s", b);
    diag_line(b);
}

/* Every api->log call from core and the addons passes through here on its way to the loader, so the
   black box sees the game log's warnings and the telemetry's session lines without another hook. */
void tap(const char *f, ...)
{
    char b[2048];
    va_list ap;
    va_start(ap, f);
    vsnprintf(b, sizeof b, f, ap);
    va_end(ap);
    if (!strncmp(b, "[game] ", 7)) {
        if (strstr(b, " (warn): ") || strstr(b, " (error): "))
            recorder.warnings.put(GetTickCount64(), GetCurrentThreadId(), "game", b + 7);
        runtime::load_stall_game_line(b);
    } else if (!strncmp(b, "[telemetry] session ", 20))
        recorder.events.put(GetTickCount64(), GetCurrentThreadId(), "session", b + 20);
    logger.loader_log("%s", b);
}

}

namespace blackbox {

void note(const char *kind, const char *a, const char *b)
{
    crash::recorder.events.put(GetTickCount64(), GetCurrentThreadId(), kind, a, b);
}

}
