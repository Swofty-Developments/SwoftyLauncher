#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <cstdint>
#include <cstdio>
#include <cstring>

// Stands in for an injected hook that rewrites ntdll's export table, loaded from swofty\mods\core
// before swofty_core. It points the NtTerminateProcess export at a syscall stub nothing calls that
// lies on the page holding NtProtectVirtualMemory, and NtContinue too where the layout allows. That
// is the layout of Proton's and Windows' ntdll, where the stubs are ordered by syscall number and
// NtTerminateProcess (0x2c), NtContinue (0x43) and NtProtectVirtualMemory (0x50) share a page. Wine
// 11 orders them the same way from another offset and splits them; Wine 9 orders them by name. The
// core then hooks a function on that page, as it does in game.

namespace {

uintptr_t page(uintptr_t a) { return a & ~uintptr_t(0xfff); }

void say(const char *s)
{
    HANDLE out = GetStdHandle(STD_OUTPUT_HANDLE);
    DWORD w;
    if (out && out != INVALID_HANDLE_VALUE)
        WriteFile(out, s, DWORD(strlen(s)), &w, nullptr);
}

void retarget()
{
    char line[320];
    HMODULE ntdll = GetModuleHandleW(L"ntdll.dll");
    auto base = reinterpret_cast<uint8_t *>(ntdll);
    auto at = [&](const char *name) { return reinterpret_cast<uintptr_t>(GetProcAddress(ntdll, name)); };
    uintptr_t cont = at("NtContinue"), protect = at("NtProtectVirtualMemory");
    const char *unused[] = {"NtDisplayString",  "NtCreatePagingFile",    "NtLoadDriver",         "NtUnloadDriver",
                            "NtShutdownSystem", "NtInitiatePowerAction", "NtSystemDebugControl", "NtSetSystemTime",
                            "NtCreateToken",    "NtPrivilegeCheck"};
    const char *decoy = nullptr;
    uintptr_t to = 0;
    for (int pass = 0; pass < 2 && !decoy && protect; pass++)
        for (const char *name : unused) {
            uintptr_t a = at(name);
            if (a && page(a) == page(protect) && (pass || page(a) == page(cont))) {
                decoy = name;
                to = a;
                break;
            }
        }
    if (!decoy) {
        snprintf(line, sizeof line,
                 "[ntpage] no unused syscall stub shares the page of NtProtectVirtualMemory (ntdll+0x%llx); the "
                 "export is left alone\n",
                 (unsigned long long)(protect - uintptr_t(base)));
        say(line);
        return;
    }
    auto nt = reinterpret_cast<IMAGE_NT_HEADERS *>(base + reinterpret_cast<IMAGE_DOS_HEADER *>(base)->e_lfanew);
    auto exp = reinterpret_cast<IMAGE_EXPORT_DIRECTORY *>(
        base + nt->OptionalHeader.DataDirectory[IMAGE_DIRECTORY_ENTRY_EXPORT].VirtualAddress);
    auto names = reinterpret_cast<DWORD *>(base + exp->AddressOfNames);
    auto ords = reinterpret_cast<WORD *>(base + exp->AddressOfNameOrdinals);
    auto funcs = reinterpret_cast<DWORD *>(base + exp->AddressOfFunctions);
    DWORD *slot = nullptr;
    for (DWORD i = 0; i < exp->NumberOfNames && !slot; i++)
        if (!strcmp(reinterpret_cast<const char *>(base + names[i]), "NtTerminateProcess"))
            slot = &funcs[ords[i]];
    DWORD old;
    if (!slot || !VirtualProtect(slot, sizeof *slot, PAGE_READWRITE, &old)) {
        say("[ntpage] the NtTerminateProcess export slot cannot be written; the export is left alone\n");
        return;
    }
    *slot = DWORD(to - uintptr_t(base));
    VirtualProtect(slot, sizeof *slot, old, &old);
    snprintf(line, sizeof line,
             "[ntpage] ntdll!NtTerminateProcess now exports %s at ntdll+0x%llx, on the page of "
             "NtProtectVirtualMemory (+0x%llx)%s NtContinue (+0x%llx)\n",
             decoy, (unsigned long long)(to - uintptr_t(base)), (unsigned long long)(protect - uintptr_t(base)),
             page(to) == page(cont) ? " and" : "; on another page:", (unsigned long long)(cont - uintptr_t(base)));
    say(line);
}

}

BOOL WINAPI DllMain(HINSTANCE, DWORD reason, LPVOID)
{
    if (reason == DLL_PROCESS_ATTACH)
        retarget();
    return TRUE;
}
