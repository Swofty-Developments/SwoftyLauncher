#include <string>

#include "core/diagnostics/crash/crash.hpp"

#include "core/diagnostics/crash/capture.hpp"
#include "core/diagnostics/crash/crash_dir.hpp"
#include "core/diagnostics/crash/crash_log.hpp"
#include "core/diagnostics/crash/dred.hpp"
#include "core/diagnostics/crash/exit_logic.hpp"
#include "core/diagnostics/crash/fatal_paths.hpp"
#include "core/diagnostics/crash/hw_settings_logic.hpp"
#include "core/diagnostics/crash/constants.hpp"
#include "core/diagnostics/crash/process.hpp"
#include "core/diagnostics/crash/report_sections.hpp"
#include "core/diagnostics/crash/report_writer.hpp"
#include "core/diagnostics/crash/symbolizer.hpp"
#include "core/runtime/loader/bindings.hpp"
#include "core/runtime/stage.hpp"
#include "safetyhook.hpp"
#include "swofty_api.h"

// Crash log and crash reports. The crash log writes each fatal-class exception, the top-level filter
// and calls to ExitProcess and TerminateProcess to loader.log. A crash report (swofty\crash-<time>.txt
// and a minidump beside it) is written once per process: for an exception that reaches the top-level
// filter, for the game's own fatal paths (a fatal Ne heap error, Ne_FatalAbort, a heap corruption or
// fail-fast exception, a stack overflow), and for an exit with a non-zero code or one that follows a
// fatal-class exception within EXIT_AFTER_FATAL_MS. An exit with a non-zero code that ends a close the game
// was asked for (WM_CLOSE to its window, or the launcher's stop request; exit_logic.hpp) gets no report
// unless a fatal-class exception came just before. A thread started at attach writes it while the
// reporting thread waits, so a stack overflow or a corrupt stack still gets one. An exit before the
// window stage logs its caller stack whatever its code; a clean one there still gets no report.

namespace crash {

constinit ProcessInfo process{};

namespace {

/* One loader.log line about the player's hardware_settings_config.xml, written at attach before the game's
   InitSequence reads it: its Documents path, whether that is under OneDrive, its size and attributes, and a read
   test that opens without recalling a cloud-only file and does not read one at all. */
void log_hardware_settings()
{
    hw_settings::Probe p;
    wchar_t docs[MAX_PATH] = {};
    using GetFolder = HRESULT(WINAPI *)(HWND, int, HANDLE, DWORD, LPWSTR);
    HMODULE shell = GetModuleHandleW(L"shell32.dll");
    if (!shell)
        shell = LoadLibraryW(L"shell32.dll");
    auto get = shell ? reinterpret_cast<GetFolder>(GetProcAddress(shell, "SHGetFolderPathW")) : nullptr;
    if (get && SUCCEEDED(get(nullptr, 0x0005, nullptr, 0, docs))) {
        std::wstring path = std::wstring(docs) + L"\\My Games\\F1 25\\hardwaresettings\\hardware_settings_config.xml";
        char utf8[1024] = {};
        WideCharToMultiByte(CP_UTF8, 0, path.c_str(), -1, utf8, sizeof utf8 - 1, nullptr, nullptr);
        p.path = utf8;
        p.onedrive = hw_settings::under_onedrive(path);
        WIN32_FILE_ATTRIBUTE_DATA fa{};
        if (GetFileAttributesExW(path.c_str(), GetFileExInfoStandard, &fa)) {
            p.exists = true;
            p.attrs = fa.dwFileAttributes;
            p.size = (uint64_t(fa.nFileSizeHigh) << 32) | fa.nFileSizeLow;
            if (!hw_settings::cloud_only(p.attrs)) {
                p.read_tried = true;
                HANDLE h = CreateFileW(path.c_str(), GENERIC_READ, FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                                       nullptr, OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL | 0x00100000, nullptr);
                char buf[4096];
                DWORD n = 0;
                if (h != INVALID_HANDLE_VALUE && ReadFile(h, buf, sizeof buf, &n, nullptr)) {
                    p.read_ok = true;
                    p.read_bytes = n;
                } else {
                    p.read_error = GetLastError();
                }
                if (h != INVALID_HANDLE_VALUE)
                    CloseHandle(h);
            }
        }
    }
    logger.api->log("%s", hw_settings::line(p).c_str());
}

}

}

using namespace crash;

/* Called by the core once the game's code reads decrypted, which on a packed build is the decrypted stage. */
void crash_game_hooks()
{
    hook_heap_errors();
    hook_fast_fails();
}

namespace runtime {

std::string code_where(uintptr_t a) { return crash::where(a); }

void crash_blame(const char *text)
{
    snprintf(capture.blame_text, sizeof capture.blame_text, "%s", text ? text : "");
    capture.blame_ms = GetTickCount64();
    capture.blame_tid = GetCurrentThreadId();
}

void crash_close_asked(const char *msg)
{
    capture.close_msg = msg;
    capture.close_ms = GetTickCount64();
    cl("[crash] the game's window got %s; an exit within %llu s is that close, with no crash report", msg,
       (unsigned long long)(exit_logic::CLOSE_MS / 1000));
}

}

namespace {

void adopt_binding_names()
{
    std::vector<Symbol> named;
    for (auto &[rva, id] : runtime::loader::bindings().resolved_functions())
        named.push_back({rva, id});
    symbol_table.adopt(std::move(named));
}

}

void crash_attach(SwoftyApi *a, const std::wstring &dir)
{
    logger.api = a;
    process.start_tick = GetTickCount();
    process.window_reached = runtime::load_stage() != "early";
    runtime::on_stage(window_stage);
    process.start_ms = GetTickCount64();
    process.exe = GetModuleHandleW(nullptr);
    GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
                       reinterpret_cast<LPCWSTR>(&crash_attach), &process.self);
    auto base = reinterpret_cast<uintptr_t>(process.exe);
    auto nt = reinterpret_cast<const IMAGE_NT_HEADERS *>(base + reinterpret_cast<const IMAGE_DOS_HEADER *>(base)->e_lfanew);
    process.exe_base = base;
    process.exe_size = nt->OptionalHeader.SizeOfImage;
    process.exe_stamp = nt->FileHeader.TimeDateStamp;
    char v[16] = {0};
    process.ea_build = GetEnvironmentVariableA("SWOFTY_EA_DIAG", v, sizeof v) && v[0] == '1';
    logger.diag_path = process.ea_build ? dir + L"..\\..\\ea-diagnostics.log" : std::wstring();
    wchar_t full[MAX_PATH * 2];
    DWORD len = GetFullPathNameW((dir + L"..\\..\\").c_str(), MAX_PATH * 2, full, nullptr);
    process.swofty_dir = len && len < MAX_PATH * 2 ? full : dir + L"..\\..\\";
    if (!process.swofty_dir.empty() && process.swofty_dir.back() != L'\\')
        process.swofty_dir += L'\\';
    logger.loader_log = a->log;
    a->log = tap;
    symbol_table.load(dir + L"symbols.tsv", nt->FileHeader.TimeDateStamp);
    process.kb_build = symbol_table.matches && nt->FileHeader.TimeDateStamp == KB_STAMP;
    if (!symbol_table.matches)
        runtime::when_code_ready("crash symbol names", adopt_binding_names);
    read_addons(process.swofty_dir + L"settings.json");
    v[0] = 0;
    GetEnvironmentVariableA("SWOFTY_CRASH_DUMP", v, sizeof v);
    report_writer.dump_mode = !_stricmp(v, "full") ? DUMP_FULL : !_stricmp(v, "off") || !strcmp(v, "0") ? DUMP_OFF : DUMP_NORMAL;
    wchar_t sys[MAX_PATH];
    UINT n = GetSystemDirectoryW(sys, MAX_PATH);
    if (n && n < MAX_PATH - 16) {
        wcscpy(sys + n, L"\\dbghelp.dll");
        if (HMODULE dh = LoadLibraryW(sys))
            report_writer.write_dump =
                reinterpret_cast<DumpFn>(reinterpret_cast<void *>(GetProcAddress(dh, "MiniDumpWriteDump")));
    }
    HMODULE ntdll = GetModuleHandleW(L"ntdll.dll"), k32 = GetModuleHandleW(L"kernel32.dll");
    HMODULE kb = GetModuleHandleW(L"kernelbase.dll");
    process.query_thread =
        reinterpret_cast<ThreadInfoFn>(reinterpret_cast<void *>(GetProcAddress(ntdll, "NtQueryInformationThread")));
    process.wine_version =
        reinterpret_cast<const char *(*)()>(reinterpret_cast<void *>(GetProcAddress(ntdll, "wine_get_version")));
    auto describe = reinterpret_cast<HRESULT(WINAPI *)(HANDLE, PCWSTR)>(
        reinterpret_cast<void *>(GetProcAddress(kb ? kb : k32, "SetThreadDescription")));
    process.thread_description =
        reinterpret_cast<DescriptionFn>(reinterpret_cast<void *>(GetProcAddress(kb ? kb : k32, "GetThreadDescription")));
    report_writer.out.cap = TEXT_CAP;
    report_writer.out.b = static_cast<char *>(VirtualAlloc(nullptr, TEXT_CAP, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE));
    report_writer.go_event = CreateEventW(nullptr, FALSE, FALSE, nullptr);
    report_writer.text_event = CreateEventW(nullptr, TRUE, FALSE, nullptr);
    report_writer.done_event = CreateEventW(nullptr, TRUE, FALSE, nullptr);
    HANDLE t =
        report_writer.out.b && report_writer.go_event && report_writer.text_event && report_writer.done_event
            ? CreateThread(nullptr, 1 << 20, writer_main, nullptr, STACK_SIZE_PARAM_IS_A_RESERVATION, &report_writer.writer_tid)
            : nullptr;
    if (t) {
        if (describe)
            describe(t, L"swofty crash report writer");
        CloseHandle(t);
    } else {
        report_writer.writer_tid = 0;
    }
    for (auto &d : {process.swofty_dir, crash_dir::fallback(data_folder())}) {
        if (d.empty())
            continue;
        prune(d, L"crash-*.txt", KEEP_REPORTS);
        prune(d, L"crash-*.dmp", KEEP_DUMPS);
    }
    AddVectoredExceptionHandler(1, veh);
    AddVectoredExceptionHandler(0, last_chance);
    capture.previous = SetUnhandledExceptionFilter(unhandled);
    auto e = safetyhook::InlineHook::create(reinterpret_cast<void *>(GetProcAddress(k32, "ExitProcess")), hooked_exit);
    auto tp = safetyhook::InlineHook::create(reinterpret_cast<void *>(GetProcAddress(k32, "TerminateProcess")), hooked_terminate);
    auto ntt = safetyhook::InlineHook::create(reinterpret_cast<void *>(GetProcAddress(ntdll, "NtTerminateProcess")),
                                              hooked_nt_terminate);
    bool fail_fast_hooked = false;
    if (auto raise = GetProcAddress(kb ? kb : k32, "RaiseFailFastException")) {
        auto ff = safetyhook::InlineHook::create(reinterpret_cast<void *>(raise), hooked_raise_fail_fast);
        if (ff) {
            game_paths.fail_fast_hook = std::move(*ff);
            fail_fast_hooked = true;
        }
    }
    if (e)
        capture.exit_hook = std::move(*e);
    if (tp)
        capture.terminate_hook = std::move(*tp);
    if (ntt)
        capture.nt_terminate_hook = std::move(*ntt);
    cl("[crash] crash log armed: fatal exceptions (first %ld in full, deduplicated by address), the top-level "
       "filter%s, ExitProcess %s, TerminateProcess %s",
       MAX_REPORTS, capture.previous ? " (chained to the game's)" : "", e ? "hooked" : "NOT hooked",
       tp ? "hooked" : "NOT hooked");
    cl("[crash] crash reports: swofty\\crash-<time>.txt (the data folder's state folder when the game folder refuses it) with a "
       "%s minidump%s, written by %s; NtTerminateProcess %s",
       report_writer.dump_mode == DUMP_FULL  ? "full-memory"
       : report_writer.dump_mode == DUMP_OFF ? "no"
                                             : "stacks-only",
       report_writer.dump_mode == DUMP_NORMAL ? " (SWOFTY_CRASH_DUMP=full for all memory, off for none)" : "",
       report_writer.writer_tid ? "a dedicated thread" : "the reporting thread", ntt ? "hooked" : "NOT hooked");
    cl("[crash] RaiseFailFastException %s; the CRT's fast-fail paths are hooked once the game's code is decrypted",
       fail_fast_hooked ? "hooked" : "NOT hooked");
    runtime::dred::arm(process.swofty_dir, tap, process.self);
    log_hardware_settings();
}
