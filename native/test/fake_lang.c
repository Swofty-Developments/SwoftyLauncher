// Fake game for the runtime string override test: a language set whose virtual find carries
// Loc_LangSet_Find's prologue, a locale module init with Loc_LocaleModule_Init's prologue, the
// g_Loc_LanguageInfo reference and one language listener. The window trigger loads the loader,
// the core and t_lang.dll; t_lang_step(n) then drives the API and this file checks the lookups.
#include <windows.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

typedef struct Set {
    void **vt;
    const char *keys[8];
    const char *vals[8];
    int n;
} Set;

static int fails;
static void expect(int ok, const char *what)
{
    printf("%s %s\n", ok ? "ok  :" : "FAIL:", what);
    fails += !ok;
}

const char *find_impl(Set *s, const char *key, int mode)
{
    (void)mode;
    for (int i = 0; i < s->n; i++)
        if (!strcmp(s->keys[i], key))
            return s->vals[i];
    return NULL;
}

static uint8_t module[0x200];
uint8_t lang_info[0x13 * 0x100];
static int listener_calls;
static uint32_t listener_lang = 0xffffffff;

int32_t module_init_impl(uint8_t *m)
{
    *(uint32_t *)(m + 0x160) = 0;
    return 0;
}

static void listener(void *ctx, uint32_t lang, uint32_t loaded)
{
    (void)ctx;
    listener_calls += loaded == 1;
    listener_lang = lang;
}

const char *fake_set_find(void *set, const char *key, int mode);
int32_t fake_module_init(void *m);
void fake_lang_info_ref(void);

__asm__(".section .xtls,\"xr\"\n"
        ".globl fake_set_find\nfake_set_find:\n"
        ".byte 0x41,0x57,0x41,0x56,0x41,0x54,0x56,0x57,0x53,0x48,0x83,0xEC,0x28,0x44,0x89,0xC7,0x49,0x89,0xD6,0x48,0x89,0xCB\n"
        "mov %rbx, %rcx\nmov %r14, %rdx\nmov %edi, %r8d\ncall find_impl\nadd $0x28, %rsp\n"
        "pop %rbx\npop %rdi\npop %rsi\npop %r12\npop %r14\npop %r15\nret\n"
        ".globl fake_module_init\nfake_module_init:\n"
        ".byte 0x41,0x57,0x41,0x56,0x41,0x55,0x41,0x54,0x56,0x57,0x53,0x48,0x81,0xEC,0xA0,0x03,0x00,0x00\n"
        "call module_init_impl\nadd $0x3a0, %rsp\n"
        "pop %rbx\npop %rdi\npop %rsi\npop %r12\npop %r13\npop %r14\npop %r15\nret\n"
        ".globl fake_lang_info_ref\nfake_lang_info_ref:\n"
        "lea lang_info(%rip), %rcx\n.byte 0x48,0x8B,0x5C,0x08,0x60\nret\n"
        ".text\n");

typedef const char *(*Find)(void *, const char *, int);
static void *vt_set[8];
static Set sets[2];

static const char *look(int set, const char *key)
{
    const char *v = ((Find)vt_set[1])(&sets[set], key, 0);
    return v ? v : "(none)";
}

static int is(int set, const char *key, const char *want)
{
    const char *v = look(set, key);
    printf("      set %d %s = '%s'\n", set, key, v);
    return !strcmp(v, want);
}

int main(void)
{
    setvbuf(stdout, NULL, _IONBF, 0);
    static const char *codes[2] = {"eng", "fre"};
    for (int i = 0; i < 2; i++)
        *(const char **)(lang_info + i * 0x100 + 0x60) = codes[i];
    for (int i = 0; i < 8; i++)
        vt_set[i] = NULL;
    vt_set[1] = (void *)fake_set_find;
    for (int i = 0; i < 2; i++) {
        sets[i].vt = vt_set;
        sets[i].keys[0] = "lng_mode_career", sets[i].vals[0] = i ? "Carriere" : "Career";
        sets[i].keys[1] = "lng_press_start", sets[i].vals[1] = i ? "APPUYEZ" : "PRESS ANY BUTTON";
        sets[i].keys[2] = "lng_other", sets[i].vals[2] = i ? "Autre" : "Other";
        sets[i].n = 3;
        *(Set **)(module + 0x28 + i * 0x10) = &sets[i];
    }
    static uint8_t listeners[0x20];
    static void *entries[2];
    entries[0] = (void *)listener, entries[1] = NULL;
    *(void **)(listeners + 0x08) = entries;
    *(void **)(listeners + 0x10) = entries + 2;
    *(void **)(module + 0x170) = listeners;

    DWORD dummy;
    expect(GetFileVersionInfoSizeW(L"C:\\windows\\system32\\kernel32.dll", &dummy) != 0, "version.dll forwarding works");
    uint8_t before = *(volatile uint8_t *)fake_set_find;
    WNDCLASSEXW wc = {.cbSize = sizeof wc,
                      .lpfnWndProc = DefWindowProcW,
                      .hInstance = GetModuleHandleW(NULL),
                      .lpszClassName = L"EGO_WindowClass"};
    extern void *__imp_RegisterClassExW;
    ATOM(WINAPI * rc)(const WNDCLASSEXW *) = (void *)__imp_RegisterClassExW;
    rc(&wc);
    expect(GetModuleHandleW(L"swofty_core.dll") != NULL, "the window trigger loaded the core");
    expect(fake_module_init(module) == 0, "locale module init ran through the core's hook");
    HMODULE t = GetModuleHandleW(L"t_lang.dll");
    int (*step)(int) = t ? (int (*)(int))(void *)GetProcAddress(t, "t_lang_step") : NULL;
    expect(step != NULL, "t_lang.dll loaded and exports t_lang_step");
    if (!step) {
        printf("FAILED (%d failure(s))\n", fails + 1);
        ExitProcess(1);
    }
    expect(before == 0x41 && *(volatile uint8_t *)fake_set_find == 0x41, "no override set: the lookup is not hooked");
    expect(is(0, "lng_mode_career", "Career"), "stock lookup before any override");
    expect(step(0) == 1, "the addon sees the override API available (API v8)");

    expect(step(1) == 0, "override_set for '*', eng and fre returned ok");
    expect(*(volatile uint8_t *)fake_set_find != 0x41, "the first override hooked the lookup");
    expect(is(0, "lng_mode_career", "1990 CAREER"), "'*' override wins in the current language's set");
    expect(is(1, "lng_mode_career", "1990 CAREER"), "'*' override wins in every set");
    expect(is(0, "lng_press_start", "ENG ONLY"), "eng override wins while eng is the current language");
    expect(is(0, "lng_other", "Other"), "fre override is not used while eng is current");
    expect(is(0, "lng_new_key", "BRAND NEW"), "an override can add a key the tables lack");
    expect(is(0, "lng_unknown", "(none)"), "other keys still miss");

    *(uint32_t *)(module + 0x160) = 1;
    expect(is(0, "lng_other", "FRE ONLY"), "fre override wins once fre is the current language");
    expect(is(0, "lng_press_start", "PRESS ANY BUTTON"), "eng-only override is not used in fre");
    *(uint32_t *)(module + 0x160) = 0;

    expect(step(2) == 0, "override_clear of '*' for lng_mode_career returned ok");
    expect(is(0, "lng_mode_career", "Career"), "a cleared key falls back to the table");
    expect(is(0, "lng_press_start", "ENG ONLY"), "other overrides stay");

    expect(step(3) == 0, "refresh returned ok");
    expect(listener_calls == 1 && listener_lang == 0, "refresh sent the current language to the listener once");

    expect(step(4) == 0, "override_clear_all returned ok");
    expect(is(0, "lng_press_start", "PRESS ANY BUTTON") && is(0, "lng_new_key", "(none)"),
           "after clear_all every key is stock again");
    expect(step(5) == 3, "override_set without a key is invalid");

    expect(step(6) == 0, "override_set of a text returned ok");
    const char *first = look(0, "lng_mode_career");
    expect(step(7) == 0, "clearing and setting the same text again returned ok");
    expect(look(0, "lng_mode_career") == first && !strcmp(first, "SAME TEXT"), "the same text is stored once");

    printf("%s (%d failure(s))\n", fails ? "FAILED" : "ALL PASSED", fails);
    ExitProcess(fails != 0);
}
