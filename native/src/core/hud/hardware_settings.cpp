#include "core/hud/hardware_settings.hpp"

#include "core/runtime/runtime.hpp"

#include <windows.h>

namespace hud::hardware_settings {

std::wstring path()
{
    using GetFolder = HRESULT(WINAPI *)(HWND, int, HANDLE, DWORD, LPWSTR);
    HMODULE shell = GetModuleHandleW(L"shell32.dll");
    if (!shell)
        shell = LoadLibraryW(L"shell32.dll");
    auto get = shell ? reinterpret_cast<GetFolder>(GetProcAddress(shell, "SHGetFolderPathW")) : nullptr;
    wchar_t docs[MAX_PATH] = {};
    if (!get || FAILED(get(nullptr, 0x0005, nullptr, 0, docs)))
        return {};
    return std::wstring(docs) + L"\\My Games\\F1 25\\hardwaresettings\\hardware_settings_config.xml";
}

hud::arm_policy::FgSetting frame_gen(const std::wstring &path)
{
    return hud::arm_policy::fg_setting(runtime::read_file(path, 1 << 20));
}

}
