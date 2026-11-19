#include <windows.h>

#include <safetyhook.hpp>
#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include <string>
#include <vector>

#include "core/runtime/hook_errors.hpp"
#include "core/runtime/json.hpp"
#include "core/frontend/ui/screen_loader.hpp"
#include "core/frontend/ui_vm.hpp"
#include "core/runtime/loader/core_rank.hpp"

namespace frontend::ui {

ScreenLoader loader;

namespace {

std::wstring self_dir()
{
    HMODULE m = nullptr;
    GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
                       reinterpret_cast<LPCWSTR>(&self_dir), &m);
    wchar_t path[MAX_PATH] = {};
    GetModuleFileNameW(m, path, MAX_PATH);
    std::wstring dir(path);
    return dir.substr(0, dir.find_last_of(L"\\/") + 1);
}

std::string read_file(const std::wstring &path)
{
    HANDLE f = CreateFileW(path.c_str(), GENERIC_READ, FILE_SHARE_READ, nullptr, OPEN_EXISTING, 0, nullptr);
    if (f == INVALID_HANDLE_VALUE)
        return {};
    std::string out;
    LARGE_INTEGER size{};
    if (GetFileSizeEx(f, &size) && size.QuadPart > 0 && size.QuadPart < (64 << 20)) {
        out.resize(size_t(size.QuadPart));
        DWORD got = 0;
        if (!ReadFile(f, out.data(), DWORD(out.size()), &got, nullptr) || got != out.size())
            out.clear();
    }
    CloseHandle(f);
    return out;
}

std::wstring wide(const std::string &s)
{
    std::wstring w(s.size(), L'\0');
    int n = MultiByteToWideChar(CP_UTF8, 0, s.data(), int(s.size()), w.data(), int(w.size()));
    w.resize(n > 0 ? size_t(n) : 0);
    return w;
}

bool env_on(const char *name)
{
    char v[16] = {};
    DWORD n = GetEnvironmentVariableA(name, v, sizeof v);
    return n && n < sizeof v && v[0] && v[0] != '0';
}

}

const Target T_HANDLER{"Ui_Uidf_Handler", "41 57 41 56 41 55 41 54 56 57 55 53 48 83 EC 48 4C 89 CB 49 89 D6", 0x449e140};
const Target T_PARSER{"Ui_Uidf_LoadScreens",
                      "41 57 41 56 41 55 41 54 56 57 55 53 48 81 EC 48 01 00 00 C5 F8 29 B4 24 30 01 00 00 4C 89 CF 49 89 CD",
                      0x41d7a90};
const Target T_PKGOPEN{"Ui_Pkg_Open", "56 57 53 48 83 EC 40 48 85 C9 0F 84 ?? ?? ?? ?? 48 89 CF 8B 01", 0x3927510};
const Target T_RESOLVE{
    "Vm_RouteCmd_ResolveTargetScreen",
    "41 57 41 56 41 55 41 54 56 57 55 53 48 81 EC 48 01 00 00 C5 F8 29 B4 24 30 01 00 00 49 89 D4 48 89 4C 24 68", 0x4187620};
const Target T_OPENEXEC{"Vm_RouteCmd_OpenScreen", "41 57 41 56 41 55 41 54 56 57 55 53 48 83 EC 58 4D 89 C7 48 89 D6", 0x3889310};
const Target T_SITE_READ{"site_Ui_Job_NodeRead", "E8 ?? ?? ?? ?? 89 C5 41 89 85 8C 00 00 00", 0x41dda07, 5};
const Target T_SITE_INIT{"site_Ui_Job_NodeInit", "4C 89 EA FF 50 48 89 C5 85 C0 0F 85", 0x41dda71, 6};
const Target T_SITE_FACTORY{"site_Ui_Job_NodeNoObject", "BD 05 00 00 00 41 89 AD 8C 00 00 00", 0x41ddc1c, 5};
const Target T_SITE_SCREEN{"site_Ui_Job_ScreenDone", "8B 43 50 FF C0 89 43 50 83 FD 55 0F 84", 0x41de0d7, 8};
const Target T_OPEN{"Vm_Open_Screen", "41 57 41 56 41 55 41 54 56 57 55 53 48 83 EC 48 48 89 D6 48 89 CF 80 3D ?? ?? ?? ?? 01",
                    0x4182660};
constexpr uintptr_t SCREEN_SYS_LOAD = 0x10c;

const char *chars(const void *s) { return s ? static_cast<const char *>(s) + 9 : ""; }

uintptr_t ScreenLoader::find(const Target &t)
{
    uintptr_t at = 0;
    int hits = swofty::api->scan(".xtls", t.pattern, &at);
    if (hits != 1 || !at) {
        swofty::log("[ui] %s: signature matched %d time(s); the ui prototype stays off", t.what, hits);
        return 0;
    }
    if (at + t.anchor - base != t.rva)
        swofty::log("[ui] %s: matched at base+0x%llx, KB build has base+0x%llx", t.what,
                    (unsigned long long)(at + t.anchor - base), (unsigned long long)t.rva);
    return at;
}

bool readable(const void *p, size_t n) { return swofty::mem::readable(p, n); }

void ScreenLoader::load_config()
{
    diagnostics = env_on("SWOFTY_UI_DIAG");
    perf = env_on("SWOFTY_PERF");
    std::wstring root = self_dir() + L"..\\..\\";
    json::Value doc;
    std::wstring src = L"settings.json";
    std::string text = read_file(root + src);
    if (text.empty() || !json::parse(text, doc) || doc["ui"].type != json::Value::Obj) {
        doc = json::Value{};
        src = L"ui.json";
        text = read_file(root + src);
        if (text.empty() || !json::parse(text, doc) || doc["ui"].type != json::Value::Obj) {
            swofty::log("[ui] no \"ui\" block in settings.json or ui.json; no addon screens declared (route guard and "
                        "build diagnostics stay on)");
            return;
        }
    }
    const json::Value &ui = doc["ui"];
    diagnostics = diagnostics || (ui["diagnostics"].type == json::Value::Bool && ui["diagnostics"].b);
    strict = ui["strict"].type == json::Value::Bool && ui["strict"].b;
    do_register = !(ui["register"].type == json::Value::Bool && !ui["register"].b);
    recover = !(ui["recover"].type == json::Value::Bool && !ui["recover"].b);
    for (auto &j : ui["screens"].a) {
        Screen s;
        s.name = j["name"].str();
        if (s.name.empty())
            continue;
        s.flow = j["flow"].str("flow_frontend");
        s.anchor = j["anchor"].str(s.flow == "flow_options" ? "settings_container" : "fe_home");
        s.file = j["file"].str(("ui/" + s.name + ".ui").c_str());
        s.view_model = j["view_model"].type == json::Value::Bool && j["view_model"].b;
        s.hash = djb2(s.name.c_str());
        s.data = read_file(root + wide(s.file));
        if (s.data.size() < 16 || s.data.compare(0, 4, "!pkg"))
            swofty::log("[ui] screen %s: %s is missing or not a !pkg file (%zu bytes)", s.name.c_str(), s.file.c_str(),
                        s.data.size());
        screens.push_back(std::move(s));
    }
    swofty::log("[ui] %ls: %zu addon screen(s), register %s, strict %s, recover %s", src.c_str(), screens.size(),
                do_register ? "on" : "off", strict ? "on" : "off", recover ? "on" : "off");
    for (auto &s : screens)
        swofty::log("[ui]   %s (%s, anchor %s, djb2 0x%08x, %zu bytes from %s%s)", s.name.c_str(), s.flow.c_str(),
                    s.anchor.c_str(), s.hash, s.data.size(), s.file.c_str(), s.view_model ? ", view model" : "");
}

template <class F> bool ScreenLoader::install(safetyhook::InlineHook &h, uintptr_t at, F *fn, const char *what)
{
    auto r = safetyhook::InlineHook::create(reinterpret_cast<void *>(at), reinterpret_cast<void *>(fn));
    if (!r) {
        swofty::log("[ui] %s: hook at 0x%llx failed", what, (unsigned long long)at);
        return false;
    }
    h = std::move(*r);
    swofty::log("[ui] %s: hooked at base+0x%llx", what, (unsigned long long)(at - base));
    return true;
}

void ScreenLoader::mid(safetyhook::MidHook &h, const Target &t, void (*fn)(safetyhook::Context &), const char *what)
{
    uintptr_t at = find(t);
    if (!at)
        return;
    auto r = safetyhook::MidHook::create(reinterpret_cast<void *>(at + t.anchor), fn);
    if (!r) {
        swofty::log("[ui] %s: mid hook at base+0x%llx failed: %s", what, (unsigned long long)(at + t.anchor - base),
                    runtime::hook_error(r.error()).c_str());
        return;
    }
    h = std::move(*r);
    swofty::log("[ui] %s: mid hook at base+0x%llx", what, (unsigned long long)(at + t.anchor - base));
}

void ScreenLoader::load()
{
    base = swofty::api->module_base();
    load_config();
    bool vms = false;
    for (auto &s : screens)
        vms |= s.view_model;
    if (vms)
        ui_vm_init(base, self_dir() + L"..\\..\\");
    a_handler = find(T_HANDLER);
    a_parser = find(T_PARSER);
    a_pkgopen = find(T_PKGOPEN);
    a_resolve = find(T_RESOLVE);
    a_openexec = find(T_OPENEXEC);
    a_open = find(T_OPEN);
    if (a_resolve) {
        auto *insn = reinterpret_cast<const uint8_t *>(a_resolve + SCREEN_SYS_LOAD);
        if (insn[0] == 0x48 && insn[1] == 0x8b && insn[2] == 0x0d)
            screen_sys =
                reinterpret_cast<void **>(a_resolve + SCREEN_SYS_LOAD + 7 + *reinterpret_cast<const int32_t *>(insn + 3));
        swofty::log("[ui] screen system global %p", static_cast<void *>(screen_sys));
    }
    if (a_resolve && screen_sys)
        install(h_resolve, a_resolve, resolve_hook, "route guard (resolve-target step)");
    if (a_openexec && screen_sys)
        install(h_openexec, a_openexec, openexec_hook, "route guard (open-screen step)");
    if (a_open)
        install(h_open, a_open, open_hook, "screen open");
    if (diagnostics) {
        mid(m_read, T_SITE_READ, read_site, "screen build diagnostics (node read)");
        mid(m_init, T_SITE_INIT, init_site, "screen build diagnostics (node init)");
        mid(m_factory, T_SITE_FACTORY, factory_site, "screen build diagnostics (node class)");
    } else {
        swofty::log("[ui] per-node screen build diagnostics off (set SWOFTY_UI_DIAG=1 or \"diagnostics\": true in the "
                    "\"ui\" block to turn them on); the screen result hook stays on");
    }
    mid(m_screen, T_SITE_SCREEN, screen_site, "screen build diagnostics (screen result)");
    if (screens.empty() || !do_register)
        return;
    if (a_handler && a_parser && a_pkgopen && screen_sys && h_open)
        install(h_handler, a_handler, handler_hook, "screen loader (UIDF handler)");
    else
        swofty::log("[ui] addon screen registration off: a loader target did not resolve");
}

}

namespace runtime {
void when_code_ready(const char *what, void (*fn)());
}

namespace frontend::ui {

namespace {

void ui_load() { loader.load(); }

}

}

SWOFTY_CORE_RANK(ui)

SWOFTY_ON_LOAD { runtime::when_code_ready("ui: screen hooks", frontend::ui::ui_load); }
