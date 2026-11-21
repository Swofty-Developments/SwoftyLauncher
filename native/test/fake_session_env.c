#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <stdio.h>

/* Imports version.dll, so the loader attaches before main, then writes seen.txt beside itself: for each variable
   its arguments name (ASCII names), NAME=value as UTF-8, or NAME unset. The session environment test reads it. */
int main(int argc, char **argv)
{
    GetFileVersionInfoSizeW(L"F1_25.exe", NULL);
    wchar_t path[MAX_PATH];
    DWORD n = GetModuleFileNameW(NULL, path, MAX_PATH);
    while (n && path[n - 1] != L'\\')
        n--;
    wcscpy(path + n, L"seen.txt");
    FILE *out = _wfopen(path, L"wb");
    if (!out)
        return 2;
    static wchar_t name[512], value[32768];
    static char value8[3 * 32768];
    for (int i = 1; i < argc; i++) {
        MultiByteToWideChar(CP_ACP, 0, argv[i], -1, name, 512);
        SetLastError(0);
        DWORD got = GetEnvironmentVariableW(name, value, 32768);
        if (!got && GetLastError() == ERROR_ENVVAR_NOT_FOUND) {
            fprintf(out, "%s unset\n", argv[i]);
            continue;
        }
        WideCharToMultiByte(CP_UTF8, 0, value, -1, value8, sizeof value8, NULL, NULL);
        fprintf(out, "%s=%s\n", argv[i], value8);
    }
    fclose(out);
    return 0;
}
