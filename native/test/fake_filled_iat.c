#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <stdint.h>
#include <stdio.h>

/* The EA App build's import table lists only Activation64.dll, and its packer loads the game's real imports,
   version.dll among them, and fills their table at load. This exe imports none of the trigger functions or
   version.dll either: it loads version.dll and fills its own table by name before it runs. */
typedef HANDLE(WINAPI *CreateMutexWFn)(LPSECURITY_ATTRIBUTES, BOOL, LPCWSTR);
typedef ATOM(WINAPI *RegisterClassExWFn)(const WNDCLASSEXW *);
typedef HWND(WINAPI *CreateWindowExWFn)(DWORD, LPCWSTR, LPCWSTR, DWORD, int, int, int, int, HWND, HMENU, HINSTANCE, LPVOID);

static struct {
    CreateMutexWFn create_mutex;
    RegisterClassExWFn register_class;
    CreateWindowExWFn create_window;
} volatile imports;

static int fails;

static void expect(int ok, const char *what)
{
    printf("%s: filled: %s\n", ok ? "ok  " : "FAIL", what);
    fails += !ok;
}

static LRESULT CALLBACK wndproc(HWND h, UINT m, WPARAM w, LPARAM l) { return DefWindowProcW(h, m, w, l); }

int main(void)
{
    setvbuf(stdout, NULL, _IONBF, 0);
    expect(LoadLibraryW(L"version.dll") != NULL, "version.dll loads");
    HMODULE k32 = GetModuleHandleW(L"kernel32.dll"), u32 = LoadLibraryW(L"user32.dll");
    void *real_cm = (void *)GetProcAddress(k32, "CreateMutexW");
    void *real_rc = (void *)GetProcAddress(u32, "RegisterClassExW"), *real_cw = (void *)GetProcAddress(u32, "CreateWindowExW");
    imports.create_mutex = (CreateMutexWFn)real_cm;
    imports.register_class = (RegisterClassExWFn)real_rc;
    imports.create_window = (CreateWindowExWFn)real_cw;
    expect(*(volatile uint8_t *)real_cm == 0xCC, "the loader set a breakpoint on the entry of KERNEL32!CreateMutexW");

    HANDLE other = imports.create_mutex(NULL, FALSE, L"swofty filled-iat test");
    expect(other && GetLastError() != ERROR_ALREADY_EXISTS && !GetModuleHandleW(L"swofty_core.dll"),
           "another mutex is made through the breakpoint and loads nothing");
    HANDLE again = imports.create_mutex(NULL, TRUE, L"swofty filled-iat test");
    expect(again && GetLastError() == ERROR_ALREADY_EXISTS, "a second CreateMutexW of that name reports ERROR_ALREADY_EXISTS");

    HANDLE mx = imports.create_mutex(NULL, FALSE, L"Codemasters F1 25");
    expect(mx && GetModuleHandleW(L"swofty_core.dll"), "CreateMutexW('Codemasters F1 25') loaded the core at the early stage");
    expect(*(volatile uint8_t *)real_cm != 0xCC, "the entry of KERNEL32!CreateMutexW is restored after the early stage");
    expect((void *)imports.register_class != real_rc && (void *)imports.create_window != real_cw,
           "the window triggers are armed in the filled table");

    WNDCLASSEXW wc = {
        .cbSize = sizeof wc, .lpfnWndProc = wndproc, .hInstance = GetModuleHandleW(NULL), .lpszClassName = L"EGO_WindowClass"};
    ATOM cls = imports.register_class(&wc);
    HWND w = imports.create_window(0, L"EGO_WindowClass", L"F1 25", WS_OVERLAPPEDWINDOW, 0, 0, 320, 200, NULL, NULL, wc.hInstance,
                                   NULL);
    expect(cls && w, "RegisterClassExW and CreateWindowExW work through the armed slots");
    expect((void *)imports.register_class == real_rc && (void *)imports.create_window == real_cw,
           "the window triggers are restored after the window stage");
    printf("%s (%d failure(s))\n", fails ? "FAILED" : "ALL PASSED", fails);
    if (w)
        DestroyWindow(w);
    CloseHandle(mx);
    CloseHandle(again);
    CloseHandle(other);
    return fails != 0;
}
