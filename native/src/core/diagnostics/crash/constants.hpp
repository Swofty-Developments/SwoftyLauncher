#pragma once

#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <windows.h>

#include <cstddef>
#include <cstdint>

namespace crash {

// Report limits and the game functions the crash code hooks, with the KB and EA builds' addresses for them.
constexpr LONG MAX_REPORTS = 16;
constexpr int MAX_FRAMES = 32;
constexpr LONG MAX_SEEN = 256;
constexpr int REPORT_FRAMES = 64;
constexpr int THREAD_FRAMES = 8;
constexpr int SCAN_FRAMES = 48;
constexpr size_t STACK_SCAN = 0x8000;
constexpr size_t TEXT_CAP = size_t(4) << 20;
constexpr int LOG_LINES = 200;
constexpr size_t LOG_TAIL = 256 << 10;
constexpr ULONGLONG PRIOR_MS = 60000;
constexpr ULONGLONG EXIT_AFTER_FATAL_MS = 10000;
constexpr unsigned EXIT_WRITER_WAIT_MS = 2000;
constexpr uint64_t DUMP_STACKS = uint64_t(48) << 20;
constexpr int KEEP_REPORTS = 8, KEEP_DUMPS = 3;
constexpr DWORD TEXT_WAIT_MS = 20000, DUMP_WAIT_MS = 60000, FULL_DUMP_WAIT_MS = 600000;
constexpr DWORD CODE_EXIT = 0xE5F10001, CODE_GAME_FATAL = 0xE5F10002;

constexpr uint32_t KB_STAMP = 1789118616;
constexpr uint32_t RVA_REPORT_MEMORY_ERROR = 0x395f6e0, RVA_HEAP_ERROR = 0x39657d0;
constexpr uint32_t EA_STAMP = 1789128892, EA_RVA_REPORT_MEMORY_ERROR = 0x394c770;
constexpr const char *PATTERN_REPORT_MEMORY_ERROR =
    "41 57 41 56 41 55 41 54 56 57 55 53 48 81 EC B8 02 00 00 C5 78 29 8C 24 A0 02 00 00 C5 79 29 84 24 90 02 00 00 "
    "C5 F9 29 BC 24 80 02 00 00";
constexpr uint8_t HEAP_ERROR_PREFIX[] = {0x55, 0x41, 0x57, 0x41, 0x56, 0x41, 0x55, 0x41, 0x54, 0x56,
                                         0x57, 0x53, 0x48, 0x83, 0xEC, 0x18, 0x48, 0x8D, 0x6C, 0x24,
                                         0x10, 0x89, 0xD6, 0x48, 0x89, 0xCF, 0x48, 0x8B, 0x5D, 0x78};
constexpr uint32_t RVA_REPORT_GS_FAILURE = 0x4d9245c, RVA_FAST_FAIL_TERMINATE = 0x5349d58,
                   RVA_INVALID_PARAMETER_ABORT = 0x5351784;
constexpr const char *PATTERN_REPORT_GS_FAILURE = "48 89 4C 24 08 48 83 EC 38 B9 17 00 00 00";
constexpr const char *PATTERN_FAST_FAIL_TERMINATE = "48 83 EC 28 B9 17 00 00 00 FF 15 ?? ?? ?? ?? 85 C0 74 07 B9";
constexpr const char *PATTERN_INVALID_PARAMETER_ABORT = "48 83 EC 28 E8 ?? ?? ?? ?? 48 85 C0 74 0A B9 16";
constexpr uint8_t FATAL_ABORT[] = {0xC7, 0x04, 0x25, 0x01, 0x00, 0x00, 0x00, 0xAD, 0xDE, 0xAD, 0xDE};
// A crash report written on the thread a hook-point guard blamed within this long leads with that blame.
constexpr ULONGLONG BLAME_MS = 10000;

}
