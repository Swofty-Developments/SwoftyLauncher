#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <cstdarg>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <string>
#include <vector>
#include "core/runtime/json.hpp"
#include "../src/loader/pattern_scan.h"
#include "swofty_api.h"

static uint8_t *img;
static FILE *logf;
static CRITICAL_SECTION log_cs;
static std::string log_path;
static int fails;

static void expect(bool ok, const char *what)
{
    printf("%s: %s\n", ok ? "ok  " : "FAIL", what);
    fails += !ok;
}

static void hlog(const char *fmt, ...)
{
    EnterCriticalSection(&log_cs);
    va_list ap;
    va_start(ap, fmt);
    vfprintf(logf, fmt, ap);
    va_end(ap);
    fputc('\n', logf);
    fflush(logf);
    LeaveCriticalSection(&log_cs);
}

static uintptr_t hbase() { return reinterpret_cast<uintptr_t>(img); }

static IMAGE_NT_HEADERS64 *nt()
{
    return reinterpret_cast<IMAGE_NT_HEADERS64 *>(img + reinterpret_cast<IMAGE_DOS_HEADER *>(img)->e_lfanew);
}

static int parse(const char *p, int *out, int max)
{
    int n = 0;
    while (*p) {
        while (*p == ' ')
            p++;
        if (!*p)
            break;
        if (n == max)
            return -1;
        if (*p == '?') {
            out[n++] = -1;
            while (*p == '?')
                p++;
            continue;
        }
        char *end;
        long v = strtol(p, &end, 16);
        if (end == p || end - p > 2)
            return -1;
        out[n++] = int(v);
        p = end;
    }
    return n;
}

/* The loader's scan contract: exactly one hit in the named section (every executable one when null). */
static int hscan(const char *section, const char *pattern, uintptr_t *out)
{
    int pat[256], n = parse(pattern, pat, 256), a1, a2, hits = 0, searched = 0;
    uintptr_t first = 0;
    if (out)
        *out = 0;
    if (n <= 0 || !pick_anchors(pat, n, &a1, &a2))
        return 0;
    auto s = IMAGE_FIRST_SECTION(nt());
    for (int i = 0; i < nt()->FileHeader.NumberOfSections; i++, s++) {
        char name[9] = {};
        memcpy(name, s->Name, 8);
        if (section ? strcmp(name, section) != 0 : !(s->Characteristics & IMAGE_SCN_MEM_EXECUTE))
            continue;
        searched++;
        uint32_t size = s->Misc.VirtualSize ? s->Misc.VirtualSize : s->SizeOfRawData;
        uintptr_t f = 0;
        int h = scan_range(img + s->VirtualAddress, img + s->VirtualAddress + size, pat, n, a1, a2, &f);
        if (h && !hits)
            first = f;
        hits += h;
    }
    hlog(searched ? "scan %s [%s] -> %d hit(s)" : "scan %s [%s] -> no such section", section ? section : "<exec>", pattern, hits);
    if (hits == 1 && out)
        *out = first;
    return hits;
}

static SwoftyApi api = {1, hlog, hbase, hscan};
static HMODULE core;
static SwoftyCoreAttachFn attach;
static SwoftyCoreInitFn core_init;
static SwoftyCoreCommitFn commit;
static SwoftyCoreStageFn stage;

static bool load_core(const std::string &root)
{
    std::string dll = root + "\\swofty\\addons\\core\\swofty_core.dll";
    core = LoadLibraryA(dll.c_str());
    if (!core) {
        printf("FAIL: LoadLibrary %s (error %lu)\n", dll.c_str(), GetLastError());
        return false;
    }
    attach = reinterpret_cast<SwoftyCoreAttachFn>(reinterpret_cast<void *>(GetProcAddress(core, "SwoftyCoreAttach")));
    core_init = reinterpret_cast<SwoftyCoreInitFn>(reinterpret_cast<void *>(GetProcAddress(core, "SwoftyCoreInit")));
    commit = reinterpret_cast<SwoftyCoreCommitFn>(reinterpret_cast<void *>(GetProcAddress(core, "SwoftyCoreCommit")));
    stage = reinterpret_cast<SwoftyCoreStageFn>(reinterpret_cast<void *>(GetProcAddress(core, "SwoftyCoreStage")));
    return attach && core_init && commit;
}

static std::string slurp(const std::string &path)
{
    std::string out;
    if (FILE *f = fopen(path.c_str(), "rb")) {
        char buf[65536];
        for (size_t n; (n = fread(buf, 1, sizeof buf, f));)
            out.append(buf, n);
        fclose(f);
    }
    return out;
}

static bool map_exe(const char *path)
{
    HANDLE f = CreateFileA(path, GENERIC_READ, FILE_SHARE_READ, nullptr, OPEN_EXISTING, 0, nullptr);
    if (f == INVALID_HANDLE_VALUE)
        return false;
    uint8_t head[0x1000];
    DWORD got = 0;
    ReadFile(f, head, sizeof head, &got, nullptr);
    auto h = reinterpret_cast<IMAGE_NT_HEADERS64 *>(head + reinterpret_cast<IMAGE_DOS_HEADER *>(head)->e_lfanew);
    img = static_cast<uint8_t *>(VirtualAlloc(nullptr, h->OptionalHeader.SizeOfImage, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE));
    if (!img)
        return false;
    memcpy(img, head, std::min<DWORD>(got, h->OptionalHeader.SizeOfHeaders));
    auto s = IMAGE_FIRST_SECTION(nt());
    for (int i = 0; i < nt()->FileHeader.NumberOfSections; i++, s++) {
        DWORD want = s->Misc.VirtualSize ? std::min(s->SizeOfRawData, s->Misc.VirtualSize) : s->SizeOfRawData;
        if (!s->PointerToRawData || !want)
            continue;
        LARGE_INTEGER at;
        at.QuadPart = s->PointerToRawData;
        SetFilePointerEx(f, at, nullptr, FILE_BEGIN);
        for (DWORD done = 0; done < want; done += got)
            if (!ReadFile(f, img + s->VirtualAddress + done, want - done, &got, nullptr) || !got)
                break;
    }
    CloseHandle(f);
    return true;
}

/* Resolves every binding of the set through the core over the on-disk exe mapped as an image and writes
   id<TAB>rva, so two cores can be compared binding by binding. */
static int steam_mode(const std::string &root, const char *exe, const char *out)
{
    if (!map_exe(exe)) {
        printf("FAIL: map %s\n", exe);
        return 1;
    }
    if (!load_core(root) || !attach(&api)) {
        printf("FAIL: attach the core\n");
        return 1;
    }
    json::Value doc;
    json::parse(slurp(root + "\\swofty\\addons\\core\\bindings.json"), doc);
    FILE *o = fopen(out, "wb");
    size_t resolved = 0;
    for (auto &b : doc["bindings"].a) {
        uintptr_t a = api.resolve(b["id"].str().c_str());
        resolved += a != 0;
        if (a)
            fprintf(o, "%s\t0x%llx\n", b["id"].str().c_str(), (unsigned long long)(a - hbase()));
        else
            fprintf(o, "%s\t-\n", b["id"].str().c_str());
    }
    fclose(o);
    printf("      resolved %zu of %zu binding(s)\n", resolved, doc["bindings"].a.size());
    return 0;
}

constexpr uint32_t IMG = 0x18000, CODE = 0x1000, CODE_SIZE = 0x10000, XCODE = 0x11000, XCODE_SIZE = 0x4000, DATA = 0x15000,
                   DATA_SIZE = 0x2000;
constexpr uint32_t F_HINT = 0x3000, F_SCAN = 0x6000, F_SITE = 0x9000, F_ENC = 0x12000, D_MARK = 0x15100;

static void fill(uint32_t at, uint32_t n, uint32_t seed)
{
    static const uint8_t alphabet[] = {0x48, 0x89, 0x8B, 0x8D, 0x83, 0xC3, 0xCC, 0x90, 0xE8, 0xE9, 0x0F, 0x85,
                                       0x84, 0x74, 0x75, 0xEB, 0x41, 0x4C, 0x49, 0x45, 0x44, 0x24, 0x20, 0x28,
                                       0x30, 0x38, 0x40, 0x50, 0x55, 0x53, 0x57, 0x56, 0x5B, 0x5D, 0x5F, 0x5E,
                                       0xC0, 0xC1, 0xC7, 0xD0, 0xD8, 0xF8, 0x01, 0x00, 0x08, 0x10, 0x18, 0xFF};
    for (uint32_t i = 0; i < n; i++) {
        seed ^= seed << 13, seed ^= seed >> 17, seed ^= seed << 5;
        img[at + i] = alphabet[seed % sizeof alphabet];
    }
}

static void crypt(uint32_t at, uint32_t n, uint32_t seed)
{
    for (uint32_t i = 0; i < n; i++) {
        seed ^= seed << 13, seed ^= seed >> 17, seed ^= seed << 5;
        img[at + i] ^= uint8_t(seed >> 8);
    }
}

static void put_fn(uint32_t at, const char *tag, uint8_t ret)
{
    uint8_t code[16] = {0x48, 0xB8, 0, 0, 0, 0, 0, 0, 0, 0, 0xB8, ret, 0, 0, 0, 0xC3};
    memcpy(code + 2, tag, 8);
    memcpy(img + at, code, sizeof code);
}

static std::string hex(uint32_t at, int n)
{
    std::string s;
    char b[4];
    for (int i = 0; i < n; i++)
        snprintf(b, sizeof b, "%s%02X", i ? " " : "", img[at + i]), s += b;
    return s;
}

static void section(IMAGE_SECTION_HEADER *s, const char *name, uint32_t va, uint32_t size, uint32_t ch)
{
    memcpy(s->Name, name, strlen(name));
    s->VirtualAddress = va, s->Misc.VirtualSize = size, s->SizeOfRawData = size, s->PointerToRawData = va;
    s->Characteristics = ch;
}

static void build_image()
{
    img = static_cast<uint8_t *>(VirtualAlloc(nullptr, IMG, MEM_RESERVE | MEM_COMMIT, PAGE_EXECUTE_READWRITE));
    auto dos = reinterpret_cast<IMAGE_DOS_HEADER *>(img);
    dos->e_magic = IMAGE_DOS_SIGNATURE, dos->e_lfanew = 0x80;
    auto h = nt();
    h->Signature = IMAGE_NT_SIGNATURE;
    h->FileHeader.Machine = IMAGE_FILE_MACHINE_AMD64, h->FileHeader.NumberOfSections = 3;
    h->FileHeader.TimeDateStamp = 0x5A5A0001, h->FileHeader.SizeOfOptionalHeader = sizeof(IMAGE_OPTIONAL_HEADER64);
    h->OptionalHeader.Magic = IMAGE_NT_OPTIONAL_HDR64_MAGIC, h->OptionalHeader.SizeOfImage = IMG;
    h->OptionalHeader.SizeOfHeaders = 0x1000, h->OptionalHeader.ImageBase = hbase();
    h->OptionalHeader.NumberOfRvaAndSizes = IMAGE_NUMBEROF_DIRECTORY_ENTRIES;
    auto s = IMAGE_FIRST_SECTION(h);
    section(s, ".rdata", CODE, CODE_SIZE, IMAGE_SCN_CNT_CODE | IMAGE_SCN_MEM_EXECUTE | IMAGE_SCN_MEM_READ);
    section(s + 1, ".xcode", XCODE, XCODE_SIZE, IMAGE_SCN_CNT_CODE | IMAGE_SCN_MEM_EXECUTE | IMAGE_SCN_MEM_READ);
    section(s + 2, ".data", DATA, DATA_SIZE, IMAGE_SCN_CNT_INITIALIZED_DATA | IMAGE_SCN_MEM_READ | IMAGE_SCN_MEM_WRITE);
    fill(CODE, CODE_SIZE, 0x1234567);
    fill(XCODE, XCODE_SIZE, 0x7654321);
    put_fn(F_HINT, "SWFTPKH1", 7);
    put_fn(F_SCAN, "SWFTPKS2", 9);
    put_fn(F_SITE, "SWFTPKS3", 11);
    put_fn(F_ENC, "SWFTPKX4", 4);
    memcpy(img + D_MARK, "SWFTDATA\x01\x02\x03\x04", 12);
}

static void write_set(const std::string &root, const char *packed)
{
    std::string fn = [&] {
        std::string t;
        for (uint32_t at : {F_HINT, F_SCAN, F_SITE, F_ENC})
            t += hex(at, 16) + "|";
        return t;
    }();
    auto pat = [&](int k) { return fn.substr(size_t(k) * 48, 47); };
    char doc[8192];
    snprintf(doc, sizeof doc,
             "{\"schema\": 1, \"build\": {\"exe\": \"F1_25.exe\", \"sha256\": \"packed-test\", \"pe_timestamp\": %u, "
             "\"image_base\": \"0x140000000\", \"distribution\": \"EA App\"%s}, \"bindings\": [\n"
             "{\"id\": \"pk:Hint_Func\", \"kind\": \"func\", \"status\": \"stable\", \"section\": \".rdata\", "
             "\"pattern\": \"%s\", \"anchor\": 0, \"resolve\": {\"kind\": \"addr\"}, \"match\": \"0x%x\"},\n"
             "{\"id\": \"pk:Scan_Func\", \"kind\": \"func\", \"status\": \"stable\", \"section\": \".rdata\", "
             "\"pattern\": \"%s\", \"anchor\": 0, \"resolve\": {\"kind\": \"addr\"}},\n"
             "{\"id\": \"pk:Site_Entry\", \"kind\": \"site\", \"status\": \"stable\", \"section\": \".rdata\", "
             "\"pattern\": \"%s\", \"anchor\": 0, \"resolve\": {\"kind\": \"addr\"}},\n"
             "{\"id\": \"pk:Enc_Func\", \"kind\": \"func\", \"status\": \"stable\", \"section\": \".xcode\", "
             "\"pattern\": \"%s\", \"anchor\": 0, \"resolve\": {\"kind\": \"addr\"}, \"match\": \"0x%x\"},\n"
             "{\"id\": \"pk:Wrong_Pattern\", \"kind\": \"func\", \"status\": \"stable\", \"section\": \".rdata\", "
             "\"pattern\": \"DE AD BE EF 0F 0B DE AD BE EF\", \"anchor\": 0, \"resolve\": {\"kind\": \"addr\"}},\n"
             "{\"id\": \"pk:Legacy\", \"kind\": \"func\", \"status\": \"ea-captured\", \"section\": \".rdata\", "
             "\"pattern\": \"\", \"anchor\": 0, \"resolve\": {\"kind\": \"addr\"}},\n"
             "{\"id\": \"pk:Unresolvable\", \"kind\": \"func\", \"status\": \"unresolvable\", \"reason\": \"no "
             "\\\"unique\\\" pattern\", \"section\": \".rdata\", \"pattern\": \"%s\", \"anchor\": 0, \"resolve\": "
             "{\"kind\": \"addr\"}},\n"
             "{\"id\": \"pk:Missing_Section\", \"kind\": \"site\", \"status\": \"stable\", \"section\": \".data1\", "
             "\"pattern\": \"74 D2 0F\", \"anchor\": 0, \"resolve\": {\"kind\": \"addr\"}},\n"
             "{\"id\": \"pk:Plain_Data\", \"kind\": \"data\", \"status\": \"stable\", \"section\": \".data\", "
             "\"pattern\": \"53 57 46 54 44 41 54 41\", \"anchor\": 0, \"resolve\": {\"kind\": \"addr\"}}\n]}\n",
             nt()->FileHeader.TimeDateStamp, packed, pat(0).c_str(), F_HINT, pat(1).c_str(), pat(2).substr(0, 29).c_str(),
             pat(3).c_str(), F_ENC, pat(0).c_str());
    FILE *f = fopen((root + "\\swofty\\addons\\core\\bindings.json").c_str(), "wb");
    fputs(doc, f);
    fclose(f);
}

static int (*orig_hint)();
static int wrap_hint() { return orig_hint() + 100; }
static int (*orig_scan)();
static int wrap_scan() { return orig_scan() + 200; }
static int (*orig_enc)();
static int wrap_enc() { return orig_enc() + 300; }
static volatile LONG site_hits, on_load_ran, init_b_calls;
static void site_handler(SwoftyContext *) { InterlockedIncrement(&site_hits); }
static void on_load_a() { on_load_ran = 1; }

static int init_a(const SwoftyApi *a)
{
    SwoftyMixin m[] = {{"pk:Hint_Func", SWOFTY_MIXIN_WRAP, 0, (void *)&wrap_hint, (void **)&orig_hint},
                       {"pk:Site_Entry", SWOFTY_MIXIN_SITE, 0, (void *)&site_handler, nullptr},
                       {nullptr, SWOFTY_MIXIN_ON_LOAD, 0, (void *)&on_load_a, nullptr}};
    return a->register_mixins(m, 3);
}

static int init_b(const SwoftyApi *a)
{
    InterlockedIncrement(&init_b_calls);
    if (!a->resolve("pk:Scan_Func"))
        return 0;
    SwoftyMixin m[] = {{"pk:Scan_Func", SWOFTY_MIXIN_WRAP, 0, (void *)&wrap_scan, (void **)&orig_scan}};
    return a->register_mixins(m, 1);
}

static int init_c(const SwoftyApi *a)
{
    SwoftyMixin m[] = {{"pk:Enc_Func", SWOFTY_MIXIN_WRAP, 0, (void *)&wrap_enc, (void **)&orig_enc}};
    return a->register_mixins(m, 1);
}

static int init_d(const SwoftyApi *a) { return a->resolve("pk:Site_Entry") != 0; }

static int call(uint32_t at) { return reinterpret_cast<int (*)()>(img + at)(); }

static bool logged(const char *needle)
{
    EnterCriticalSection(&log_cs);
    bool hit = slurp(log_path).find(needle) != std::string::npos;
    LeaveCriticalSection(&log_cs);
    return hit;
}

static bool wait_logged(const char *needle, DWORD ms)
{
    for (DWORD t0 = GetTickCount(); GetTickCount() - t0 < ms; Sleep(100))
        if (logged(needle))
            return true;
    return logged(needle);
}

/* A synthetic packed exe: .rdata is code that reads encrypted until "the game" decrypts it before the window
   stage, .xcode stays encrypted, .data is plain. plain mode is the same image never encrypted. */
static int image_mode(const std::string &root, const char *mode)
{
    bool plain = !strcmp(mode, "plain"), measured = !strcmp(mode, "measured");
    build_image();
    write_set(root, plain ? ", \"packed\": false" : measured ? "" : ", \"packed\": true");
    if (!plain) {
        crypt(CODE, CODE_SIZE, 0xC0DE);
        crypt(XCODE, XCODE_SIZE, 0xBEEF);
    }
    std::string enc_before(reinterpret_cast<char *>(img + F_ENC), 32), hint_before(reinterpret_cast<char *>(img + F_HINT), 16);
    SetEnvironmentVariableA("SWOFTY_LOAD_STAGE", "early");
    if (!load_core(root) || !attach(&api)) {
        printf("FAIL: attach the core\n");
        return 1;
    }
    auto self = reinterpret_cast<SwoftyAddonInitFn>(reinterpret_cast<void *>(GetProcAddress(core, "SwoftyAddonInit")));
    if (self)
        core_init(L"swofty_core.dll", self);
    expect(core_init(L"t_pk_a.dll", init_a) == 1, "init: an addon that only registers mixins initialises");
    int rb = core_init(L"t_pk_b.dll", init_b);
    expect(core_init(L"t_pk_c.dll", init_c) == 1, "init: the addon on the never-decrypted section initialises");
    commit();
    if (plain) {
        expect(rb == 1 && init_b_calls == 1, "plain: the addon that resolves in its init succeeds on the first call");
        expect(call(F_HINT) == 107 && call(F_SCAN) == 209 && call(F_ENC) == 304, "plain: every wrap installs at the commit");
        call(F_SITE);
        expect(site_hits == 1 && on_load_ran == 1, "plain: the site runs and the on_load handler ran at the commit");
        expect(!logged("result=deferred"), "plain: nothing is deferred on a build that is not packed");
        expect(logged("[bind] id=pk:Hint_Func build=ea pass=init result=resolved method=hint matches=1"),
               "plain: the hint is verified at init");
        printf("%s (%d failure(s))\n", fails ? "FAILED" : "ALL PASSED", fails);
        return fails != 0;
    }
    expect(rb == 1 && init_b_calls == 1, "init: the addon whose init needs a deferred binding is kept for a second call");
    expect(!memcmp(img + F_HINT, hint_before.data(), 16), "init: nothing is written into the encrypted code");
    expect(on_load_ran == 0, "init: a waiting addon's on_load handler does not run");
    expect(logged("[bind] id=pk:Hint_Func build=ea pass=init result=deferred method=- matches=- rva=- match=- verified=- "
                  "reason=\"code section .rdata reads encrypted"),
           "init: the code-section binding is deferred");
    expect(logged("[bind] id=pk:Plain_Data build=ea pass=init result=resolved method=scan matches=1"),
           "init: the plain-section binding resolves at init");
    expect(logged("[bind] summary build=ea pass=init resolved=1 deferred=6 unresolved=2 total=9"),
           "init: summary counts 1 resolved, 6 deferred, 2 unresolved");
    expect(logged(measured ? "(measured at init)" : "(the set says so)"),
           "init: the packed state comes from the expected source");
    crypt(CODE, CODE_SIZE, 0xC0DE);
    expect(call(F_HINT) == 7, "decrypted: the target runs unhooked before the window stage");
    expect(core_init(L"t_pk_d.dll", init_d) == 1 && commit() >= 0,
           "decrypted: an addon loaded with the window resolves a deferred binding on demand");
    expect(stage && stage(SWOFTY_STAGE_WINDOW) >= 1, "window: the core runs its stage handlers");
    expect(init_b_calls == 2, "window: the waiting addon's init runs again");
    expect(call(F_HINT) == 107, "window: the wrap on the hinted binding installed");
    expect(call(F_SCAN) == 209, "window: the wrap the second init registered installed");
    call(F_SITE);
    expect(site_hits == 1 && on_load_ran == 1, "window: the site runs and the late addon's on_load handler ran");
    expect(logged("[bind] id=pk:Hint_Func build=ea pass=window result=resolved method=hint matches=1 rva=0x3000 "
                  "match=0x3000 verified=yes reason=\"\" site_bytes=\"\""),
           "window: the hint line");
    expect(logged("[bind] id=pk:Scan_Func build=ea pass=window result=resolved method=scan matches=1 rva=0x6000 "
                  "match=0x6000 verified=yes"),
           "window: the scan line");
    expect(logged("[bind] id=pk:Wrong_Pattern build=ea pass=window result=unresolved method=- matches=0 rva=- match=- "
                  "verified=- reason=\"signature did not match exactly once\" site=- site_bytes=\"\""),
           "window: a pattern that misses decrypted code is unresolved, not retried");
    expect(logged("[bind] id=pk:Enc_Func build=ea pass=window result=deferred"), "window: the still-encrypted binding waits");
    expect(logged("[bind] summary build=ea pass=window resolved=4 deferred=1 unresolved=4 total=9"),
           "window: summary counts 4 resolved, 1 deferred, 4 unresolved");
    expect(wait_logged("[bind] summary build=ea pass=stage:8", 20000), "retry: the last retry reports");
    expect(logged("[bind] id=pk:Enc_Func build=ea pass=stage:8 result=unresolved method=- matches=- rva=- match=- "
                  "verified=- reason=\"code section .xcode reads encrypted"),
           "retry: the still-encrypted binding fails closed at the last retry");
    expect(logged(("site=0x12000 site_bytes=\"" + hex(F_ENC, 32) + "\"").c_str()),
           "retry: the fail-closed line carries 32 bytes at its hint");
    expect(logged("addon t_pk_c.dll dropped"), "retry: the addon on the never-decrypted code is dropped");
    expect(!memcmp(img + F_ENC, enc_before.data(), 32), "retry: no hook was ever written into the encrypted code");
    expect(logged("[bind] summary build=ea pass=stage:8 resolved=4 deferred=0 unresolved=5 total=9"),
           "retry: the final summary has nothing deferred");
    printf("%s (%d failure(s))\n", fails ? "FAILED" : "ALL PASSED", fails);
    return fails != 0;
}

int main(int argc, char **argv)
{
    setvbuf(stdout, nullptr, _IONBF, 0);
    InitializeCriticalSection(&log_cs);
    if (argc < 3) {
        printf("usage: bind_harness steam|packed|measured|plain <root> [exe out]\n");
        return 2;
    }
    std::string root = argv[2];
    log_path = root + "\\bind.log";
    logf = fopen(log_path.c_str(), "wb");
    int r = !strcmp(argv[1], "steam") && argc > 4 ? steam_mode(root, argv[3], argv[4]) : image_mode(root, argv[1]);
    fflush(stdout);
    TerminateProcess(GetCurrentProcess(), r);
    return r;
}
