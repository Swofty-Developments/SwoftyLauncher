// Fake game for the career UI test: stand-ins for the character ref builder and list handler, the
// speech line post, the career wizard's start-in-F1 check, tile router, step machine, confirm
// route site, Choose Driver tab builder call and tab state setter, Ne_Make_StrFromCStr, and the
// career chain down to a CareerSetupComponent data group. Each carries a tagged prologue
// (mov rax, "SWFT....") that run_career_ui_test.sh writes into bindings.json. The window trigger
// loads the loader, the core and t_career_ui.dll; t_ui_step(n) drives the API and this file
// checks what the core's hooks did.
#include <windows.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static int fails;
static void expect(int ok, const char *what)
{
    printf("%s %s\n", ok ? "ok  :" : "FAIL:", what);
    fails += !ok;
}

typedef struct NeStr {
    uint32_t hash;
    uint16_t refcount;
    uint8_t len_flags[3];
    char chars[64];
} NeStr;

typedef struct Action {
    int32_t type, pad;
    NeStr *str;
} Action;

uint32_t nestr_impl(NeStr **out, const char *s, uint32_t flags, uintptr_t len)
{
    (void)flags, (void)len;
    if (!s) {
        *out = NULL;
        return 0;
    }
    NeStr *n = calloc(1, sizeof *n);
    strncpy(n->chars, s, sizeof n->chars - 1);
    *out = n;
    return 0;
}

typedef struct Text {
    uint8_t pad[0x10];
    const char *s;
} Text;

typedef struct Node {
    uint8_t stat;
    char name[32];
} Node;

static Text names[3] = {{.s = "elena"}, {.s = "flavio"}, {.s = "max"}};
static uint8_t rows[3][0x38];
static Node nodes[3];
static int node_at;
static struct {
    uint8_t pad[8];
    uint8_t *begin, *end;
} vector;

void *fake_build_ref(void *out, const uint8_t *character, void *node);

void *build_ref_impl(void *out, const uint8_t *character, void *node)
{
    Node *n = node;
    n->stat = character[0];
    const Text *t = *(Text *const *)(character + 0x08);
    snprintf(n->name, sizeof n->name, "%s", t->s);
    return out;
}

int64_t list_loaded_impl(void *ctx, const uint8_t *message)
{
    (void)ctx;
    const uint8_t *v = *(const uint8_t *const *)(message + 0x10);
    const uint8_t *b = *(const uint8_t *const *)(v + 0x08), *e = *(const uint8_t *const *)(v + 0x10);
    node_at = 0;
    for (; b < e; b += 0x38)
        fake_build_ref(NULL, b, &nodes[node_at++]);
    return 0;
}

static char rtpc_name[32];
static float rtpc_value = -1;
static int posted;

static void set_rtpc(void *emitter, const char *name, float value, int a, float interp, int b)
{
    (void)emitter, (void)a, (void)interp, (void)b;
    snprintf(rtpc_name, sizeof rtpc_name, "%s", name);
    rtpc_value = value;
}

void post_line_impl(void *speech, void *line, uint8_t retrigger)
{
    (void)speech, (void)line, (void)retrigger;
    posted++;
}

_Bool starts_impl(void *entrants, void *data, const char *group)
{
    (void)entrants, (void)data, (void)group;
    return 0;
}

uint32_t fake_nestr(NeStr **out, const char *s, uint32_t flags, uintptr_t len);

Action *route_impl(uint8_t *item, Action *out)
{
    (void)item;
    out->type = 1;
    fake_nestr(&out->str, "career_wizard_driver_career_to_f1", 0, 0);
    return out;
}

typedef struct Wizard {
    uint8_t pad[0xe0];
    uint8_t *steps_b, *steps_e, *steps_cap;
    uint8_t pad2[0x08];
    uint8_t *opts_b, *opts_e, *opts_cap;
    uint8_t pad3[0x10];
    const char *group;
    uint8_t steps[16], opts[16];
} Wizard;

void push_impl(Wizard *w)
{
    uint8_t last = w->opts_e[-1];
    *w->steps_e++ = last == 9 || last == 10 ? 5 : last == 0xb ? 14 : 6;
}

static char confirm_seen[64];
void record_confirm(const char *route) { snprintf(confirm_seen, sizeof confirm_seen, "%s", route); }

static int tabs_seen;
static char tabs_names[128];
int64_t build_tabs_impl(void *tabs, void *alloc, void *node, const uint8_t *group)
{
    (void)tabs, (void)alloc, (void)node;
    const uint8_t *b = *(const uint8_t *const *)(group + 0x08), *e = *(const uint8_t *const *)(group + 0x10);
    tabs_seen = 0;
    tabs_names[0] = 0;
    for (; b < e; b += 0x30, tabs_seen++) {
        strcat(tabs_names, (const char *)(*(const uint8_t *const *)(b + 0x08) + 9));
        strcat(tabs_names, " ");
    }
    return 0;
}

static int tab_state_calls;
static char tab_states[16];
void tab_state_impl(void *header, uint64_t tab, uint8_t visible, uint8_t enabled)
{
    (void)header;
    tab_state_calls++;
    if (tab < 4) {
        tab_states[tab * 3] = (char)('0' + visible);
        tab_states[tab * 3 + 1] = (char)('0' + enabled);
        tab_states[tab * 3 + 2] = ' ';
    }
}

void *game_manager_slot;
uint8_t setup_desc[0x20];

_Bool fake_starts(void *, void *, const char *);
Action *fake_route(uint8_t *item, Action *out);
void fake_push(Wizard *w);
void fake_confirm(const char *route, void *vm);
int64_t fake_header_build(void *tabs, void *alloc, void *node, const void *group);
void fake_tab_state(void *header, uint64_t tab, uint8_t visible, uint8_t enabled);
int64_t fake_list_loaded(void *ctx, const void *message);
void fake_post_line(void *speech, void *line, uint8_t retrigger);
void fake_manager_ref(void);
void fake_desc_ref(void);

#define TAG(a) ".byte 0x48,0xB8,0x53,0x57,0x46,0x54," a "\n"
__asm__(".section .xtls,\"xr\"\n"
        ".globl fake_build_ref\nfake_build_ref:\n" TAG(
            "0x43,0x52,0x45,0x46") "jmp build_ref_impl\n"
                                   ".globl fake_list_loaded\nfake_list_loaded:\n" TAG(
                                       "0x43,0x4C,0x53,0x54") "jmp list_loaded_impl\n"
                                                              ".globl fake_post_line\nfake_post_line:\n" TAG(
                                                                  "0x53,0x50,0x43,0x48") "jmp post_line_impl\n"
                                                                                         ".globl fake_starts\nfake_starts:\n" TAG("0x53,0x54,0x46,0x31") "jmp starts_impl\n"
                                                                                                                                                         ".globl fake_route\nfake_route:\n" TAG(
                                                                                                                                                             "0x52,0x4F,0x55,0x54") "jmp route_impl\n"
                                                                                                                                                                                    ".globl fake_push\nfake_push:\n" TAG(
                                                                                                                                                                                        "0x50,0x55,0x53,0x48") "jmp push_impl\n"
                                                                                                                                                                                                               ".globl fake_tab_state\nfake_tab_state:\n" TAG(
                                                                                                                                                                                                                   "0x54,0x53,0x54,0x41") "jmp tab_state_impl\n"
                                                                                                                                                                                                                                          ".globl fake_nestr\nfake_nestr:\n" TAG("0x4E,0x53,0x54,0x52") "jmp nestr_impl\n"
                                                                                                                                                                                                                                                                                                        ".globl fake_confirm\nfake_confirm:\n"
                                                                                                                                                                                                                                                                                                        "push %rsi\nsub $0x20, %rsp\nmov %rdx, %rsi\nmov %rcx, %rdx\n" TAG("0x43,0x4F,0x4E,0x46") "mov %rdx, %rcx\ncall record_confirm\nadd $0x20, %rsp\npop %rsi\nret\n"
                                                                                                                                                                                                                                                                                                                                                                                                  ".globl fake_header_build\nfake_header_build:\n"
                                                                                                                                                                                                                                                                                                                                                                                                  "sub $0x28, %rsp\n" TAG("0x54,0x41,0x42,0x53") "call build_tabs_impl\nadd $0x28, %rsp\nret\n"
                                                                                                                                                                                                                                                                                                                                                                                                                                                 ".globl fake_manager_ref\nfake_manager_ref:\n" TAG("0x47,0x4D,0x47,0x52") "lea game_manager_slot(%rip), %rax\nret\n"
                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           ".globl fake_desc_ref\nfake_desc_ref:\n" TAG(
                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               "0x44,0x45,0x53,0x43") "lea setup_desc(%rip), %rax\nret\n"
                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      ".text\n");

static void *vt_any[0x80];
static void *vt_list[8], *vt_comp[0x60], *vt_emitter[16];
static uint8_t manager[0x100], ctx[0x100], holder[0x300], entity[0x1100], champ[0x300], comp[0x300], source[0x600];
static uint8_t champ_entries[0x10], group_entries[0x20], group_obj[0x30], group_named[0x18];
static const char *group_name = "f1_career_1990";

static void *components_get(void *list, const void *desc, uint32_t parents)
{
    (void)list, (void)parents;
    return desc == setup_desc ? comp : NULL;
}

static uint8_t group_index(void *c)
{
    (void)c;
    return 1;
}

static void career_chain(void)
{
    for (int i = 0; i < 0x80; i++)
        vt_any[i] = (void *)post_line_impl;
    *(void ***)ctx = vt_any, *(void ***)holder = vt_any, *(void ***)manager = vt_any;
    *(void **)(manager + 0xc0) = ctx;
    *(void **)(ctx + 0x88) = holder;
    *(void **)(holder + 0x260) = entity;
    *(void ***)entity = vt_any;
    *(void ***)(entity + 0x1030) = vt_any;
    *(void **)(champ_entries + 8) = champ;
    *(void **)(entity + 0x1040) = champ_entries;
    *(void **)(entity + 0x1048) = champ_entries + 0x10;
    *(void ***)champ = vt_any;
    vt_list[2] = (void *)components_get;
    *(void ***)(champ + 0x208) = vt_list;
    for (int i = 0; i < 0x60; i++)
        vt_comp[i] = (void *)post_line_impl;
    vt_comp[0x268 / 8] = (void *)group_index;
    *(void ***)comp = vt_comp;
    *(void **)(comp + 0x290) = source;
    *(void **)(group_entries + 0x10 + 8) = group_obj;
    *(void **)(source + 0x560) = group_entries;
    *(void **)(source + 0x568) = group_entries + 0x20;
    *(void **)(group_obj + 0x28) = group_named;
    *(const char **)(group_named + 0x10) = group_name;
}

static int ref_is(int i, uint8_t stat, const char *name)
{
    printf("      node %d: stat %u name %s\n", i, nodes[i].stat, nodes[i].name);
    return nodes[i].stat == stat && !strcmp(nodes[i].name, name);
}

static uint8_t speech[0x800], speaker_table[0x200], rec_agent[0x20], rec_engineer[0x20], line[0x100], emitter[0x10];

static void speak(uint64_t speaker)
{
    *(uint64_t *)(line + 0x80) = speaker;
    rtpc_value = -1;
    rtpc_name[0] = 0;
    fake_post_line(speech, line, 0);
}

static void wizard_init(Wizard *w, const char *group)
{
    memset(w, 0, sizeof *w);
    w->steps_b = w->steps_e = w->steps;
    w->steps_cap = w->steps + 16;
    w->opts_b = w->opts_e = w->opts;
    w->opts_cap = w->opts + 16;
    w->group = group;
}

static void add_option(Wizard *w, uint8_t option)
{
    *w->opts_e++ = option;
    fake_push(w);
}

static NeStr *nestr_of(const char *s)
{
    NeStr *n = NULL;
    nestr_impl(&n, s, 0, 0);
    return n;
}

int main(void)
{
    setvbuf(stdout, NULL, _IONBF, 0);
    static const uint8_t stats[3] = {28, 105, 3};
    for (int i = 0; i < 3; i++) {
        rows[i][0] = stats[i];
        *(Text **)(rows[i] + 0x08) = &names[i];
    }
    vector.begin = rows[0], vector.end = rows[0] + sizeof rows;
    uint8_t message[0x18] = {0};
    *(void **)(message + 0x10) = &vector;
    career_chain();
    *(void **)(speech + 0x728) = speaker_table;
    *(uint32_t *)(speech + 0x7cc) = 2;
    *(void **)(speaker_table + 0x60) = rec_agent;
    *(void **)(speaker_table + 0x60 + 0x88) = rec_engineer;
    *(const char **)(rec_agent + 0x18) = "agent";
    *(const char **)(rec_engineer + 0x18) = "engineer";
    for (int i = 0; i < 16; i++)
        vt_emitter[i] = (void *)post_line_impl;
    vt_emitter[0x60 / 8] = (void *)set_rtpc;
    *(void ***)emitter = vt_emitter;
    *(void **)(line + 0x28) = emitter;

    DWORD dummy;
    expect(GetFileVersionInfoSizeW(L"C:\\windows\\system32\\kernel32.dll", &dummy) != 0, "version.dll forwarding works");
    HANDLE early = getenv("T_EARLY") ? CreateMutexW(NULL, FALSE, L"Codemasters F1 25") : NULL;
    expect(!getenv("T_EARLY") || (early && GetModuleHandleW(L"swofty_core.dll")),
           "with the early stage on, CreateMutexW('Codemasters F1 25') loaded the core");
    WNDCLASSEXW wc = {.cbSize = sizeof wc,
                      .lpfnWndProc = DefWindowProcW,
                      .hInstance = GetModuleHandleW(NULL),
                      .lpszClassName = L"EGO_WindowClass"};
    extern void *__imp_RegisterClassExW;
    ATOM(WINAPI * rc)(const WNDCLASSEXW *) = (void *)__imp_RegisterClassExW;
    rc(&wc);
    expect(GetModuleHandleW(L"swofty_core.dll") != NULL, "the window trigger loaded the core");
    if (getenv("T_ONLY")) {
        const char *only = getenv("T_ONLY");
        expect(GetModuleHandleA(only) != NULL, "the window stage loaded the test addon");
        expect(fake_starts(NULL, NULL, "f1_career_2026") == 0, "start in F1 for f1_career_2026: the game's answer");
        printf("%s (%d failure(s))\n", fails ? "SOME FAILED" : "ALL PASSED", fails);
        fflush(stdout);
        ExitProcess(fails ? 1 : 0);
    }
    HMODULE t = GetModuleHandleW(L"t_career_ui.dll");
    int (*step)(int) = t ? (int (*)(int))(void *)GetProcAddress(t, "t_ui_step") : NULL;
    expect(step != NULL, "t_career_ui.dll loaded with t_ui_step");
    if (!step)
        return 1;

    fake_list_loaded(NULL, message);
    expect(node_at == 3 && ref_is(0, 28, "elena"), "the game's first character list build is stock");
    expect(step(0) == 3, "characters and wizard report available, and the registrations at init and at load were ok");
    expect(step(1) == 0, "every registration for f1_career_1990 returns ok");
    expect(step(7) == 33, "an empty group and a step past a byte are invalid");
    expect(step(8) == 3, "strings_json refuses a document that is not an object of language objects");
    expect(step(4) == 0, "no career loaded: the active group is empty");
    fake_list_loaded(NULL, message);
    expect(ref_is(0, 28, "elena"), "no career loaded: the elena ref stays elena");
    speak(0);
    expect(rtpc_value < 0 && posted == 1, "no career loaded: an agent line posts with no volume change");

    game_manager_slot = manager;
    expect(step(2) == 0, "refresh with a 1990 career loaded rebuilt the list");
    expect(step(3) == 0, "career_group() reads f1_career_1990 from the CareerSetupComponent");
    expect(ref_is(0, 28, "flavio") && ref_is(1, 105, "flavio") && ref_is(2, 3, "max"),
           "1990 career: the elena ref shows flavio under elena's stat_id, other rows untouched");
    speak(0);
    expect(!strcmp(rtpc_name, "vol_speech") && rtpc_value == 0.f && posted == 2,
           "1990 career: an agent line sets vol_speech 0 on its emitter and still posts");
    speak(1);
    expect(rtpc_value < 0 && posted == 3, "1990 career: an engineer line is not touched");

    expect(fake_starts(NULL, NULL, "f1_career_1990") == 1, "start in F1: yes for f1_career_1990");
    expect(fake_starts(NULL, NULL, "f1_career_2026") == 0, "start in F1: the game's answer for f1_career_2026");

    static uint8_t item90[0x1c8], item26[0x1c8];
    *(NeStr **)(item90 + 0x180) = nestr_of("f1_career_1990");
    *(NeStr **)(item26 + 0x180) = nestr_of("f1_career_2026");
    Action out = {0};
    fake_route(item26, &out);
    expect(out.type == 1 && !strcmp(out.str->chars, "career_wizard_driver_career_to_f1"), "tile route: 2026 tile stock");
    fake_route(item90, &out);
    printf("      1990 tile route %s\n", out.str ? out.str->chars : "(null)");
    expect(out.type == 1 && !strcmp(out.str->chars, "career_wizard_driver_career_1990"),
           "tile route: the 1990 tile opens career_wizard_driver_career_1990");
    expect(step(5) == 0, "wizard group() names the last tile pressed");

    Wizard w90, w26;
    wizard_init(&w90, "f1_career_1990");
    add_option(&w90, 2);
    add_option(&w90, 10);
    add_option(&w90, 0xb);
    printf("      1990 options %u %u %u steps %u %u %u\n", w90.opts[0], w90.opts[1], w90.opts[2], w90.steps[0], w90.steps[1],
           w90.steps[2]);
    expect(w90.opts[1] == 9, "options: the 1990 season length records Full Season for Custom Season");
    expect(w90.steps_e - w90.steps_b == 3 && w90.steps[2] == 6,
           "steps: the 1990 classic drivers step is replaced by the confirm step");
    wizard_init(&w26, "f1_career_2026");
    add_option(&w26, 2);
    add_option(&w26, 10);
    add_option(&w26, 0xb);
    expect(w26.opts[1] == 10 && w26.steps[2] == 14, "options and steps: the 2026 wizard is stock");

    static uint8_t vm[0x300];
    *(Wizard **)(vm + 0x270) = &w90;
    fake_confirm("driver_career_driver_type_select", vm);
    expect(!strcmp(confirm_seen, "driver_career_driver_select"), "confirm: the 1990 wizard opens Choose Driver");
    *(Wizard **)(vm + 0x270) = &w26;
    fake_confirm("driver_career_driver_type_select", vm);
    expect(!strcmp(confirm_seen, "driver_career_driver_type_select"), "confirm: the 2026 wizard keeps the driver type step");

    static uint8_t entries[4][0x30];
    static const char *tabs[4] = {"header_f1", "header_selected_team_icon", "header_f2", "header_icon"};
    for (int i = 0; i < 4; i++)
        *(NeStr **)(entries[i] + 0x08) = nestr_of(tabs[i]);
    static uint8_t group[0x18];
    *(void **)(group + 0x08) = entries[0];
    *(void **)(group + 0x10) = entries[0] + sizeof entries;
    static uint8_t header[0x300], tab_items[4][0x70];
    *(void **)(header + 0x2b8) = tab_items[0];
    *(void **)(header + 0x2c0) = tab_items[0] + sizeof tab_items;
    fake_route(item90, &out);
    fake_header_build(header + 0x270, NULL, NULL, group);
    printf("      1990 tabs %d: %s\n", tabs_seen, tabs_names);
    expect(tabs_seen == 4, "driver tabs: the 1990 wizard's header keeps all four tabs, so no empty tab box");
    for (int i = 0; i < 4; i++)
        fake_tab_state(header, (uint64_t)i, 1, 1);
    printf("      1990 tab states: %s\n", tab_states);
    expect(!strcmp(tab_states, "11 11 00 00 "), "driver tabs: the 1990 wizard's F2 and Icons tabs are off");
    fake_route(item26, &out);
    fake_header_build(header + 0x270, NULL, NULL, group);
    expect(tabs_seen == 4, "driver tabs: the 2026 wizard's header keeps its four tabs");
    for (int i = 0; i < 4; i++)
        fake_tab_state(header, (uint64_t)i, 1, 1);
    expect(!strcmp(tab_states, "11 11 11 11 "), "driver tabs: the 2026 wizard's tabs stay as the game sets them");
    tab_state_calls = 0;
    fake_tab_state(header, 5, 0, 0);
    fake_tab_state(header, 1, 0, 0);
    expect(tab_state_calls == 1, "tab state: an index past the header's tabs is skipped, one inside is set");

    game_manager_slot = NULL;
    expect(step(2) == 0 && step(4) == 0, "career unloaded: refresh empties the active group");
    expect(ref_is(0, 28, "elena"), "career unloaded: the elena ref is elena again");
    speak(0);
    expect(rtpc_value < 0, "career unloaded: an agent line keeps its volume");
    game_manager_slot = manager;
    expect(step(6) == 0, "clear drops this addon's entries");
    expect(step(2) == 0 && ref_is(0, 28, "elena"), "after clear a 1990 career keeps the elena ref");
    expect(fake_starts(NULL, NULL, "f1_career_1990") == 0, "after clear start in F1 is the game's answer");

    printf("%s (%d failure(s))\n", fails ? "SOME FAILED" : "ALL PASSED", fails);
    fflush(stdout);
    ExitProcess(fails ? 1 : 0);
}
