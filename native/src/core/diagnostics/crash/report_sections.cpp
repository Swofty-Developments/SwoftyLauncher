#include "core/diagnostics/crash/report_sections.hpp"

#include <algorithm>
#include <cstdio>
#include <cstring>

#include "core/apply/moments.hpp"
#include "core/diagnostics/crash/blackbox.hpp"
#include "core/diagnostics/crash/capture.hpp"
#include "core/diagnostics/crash/crash_log.hpp"
#include "core/diagnostics/crash/dred.hpp"
#include "core/diagnostics/crash/dred_logic.hpp"
#include "core/diagnostics/crash/constants.hpp"
#include "core/diagnostics/crash/minidump.hpp"
#include "core/diagnostics/crash/process.hpp"
#include "core/diagnostics/crash/report_text_logic.hpp"
#include "core/diagnostics/crash/report_writer.hpp"
#include "core/diagnostics/crash/symbolizer.hpp"
#include "core/diagnostics/perf.hpp"
#include "core/render/d3d12_hooks.hpp"

namespace runtime {
void placed_record_crash_text(char *out, size_t cap);
}

namespace crash {

constinit ReportSections sections{};

void section_registers(const Request &r)
{
    const CONTEXT &c = r.ctx;
    report_writer.out.put("\n== Registers\n");
    report_writer.out.put("  rax=%016llx rbx=%016llx rcx=%016llx rdx=%016llx\n", c.Rax, c.Rbx, c.Rcx, c.Rdx);
    report_writer.out.put("  rsi=%016llx rdi=%016llx rbp=%016llx rsp=%016llx\n", c.Rsi, c.Rdi, c.Rbp, c.Rsp);
    report_writer.out.put("  r8 =%016llx r9 =%016llx r10=%016llx r11=%016llx\n", c.R8, c.R9, c.R10, c.R11);
    report_writer.out.put("  r12=%016llx r13=%016llx r14=%016llx r15=%016llx\n", c.R12, c.R13, c.R14, c.R15);
    report_writer.out.put("  rip=%016llx eflags=%08lx cs=%04x ss=%04x ds=%04x es=%04x fs=%04x gs=%04x mxcsr=%08lx\n", c.Rip,
                          c.EFlags, c.SegCs, c.SegSs, c.SegDs, c.SegEs, c.SegFs, c.SegGs, c.MxCsr);
    const M128A *x = &c.Xmm0;
    for (int i = 0; i < 16; i += 2)
        report_writer.out.put("  xmm%-2d=%016llx%016llx xmm%-2d=%016llx%016llx\n", i, (unsigned long long)x[i].High,
                              (unsigned long long)x[i].Low, i + 1, (unsigned long long)x[i + 1].High,
                              (unsigned long long)x[i + 1].Low);
}

bool call_before(uintptr_t v)
{
    uint8_t b[8];
    Reader rd;
    return rd.read(v - 8, b, 8) && report_text::call_before(b);
}

void section_stack(const CONTEXT &ctx, uintptr_t lo, uintptr_t hi, const char *title)
{
    report_writer.out.put("\n== %s\n", title);
    char w[512];
    int frames = walk(ctx, lo, hi, REPORT_FRAMES, [&](int i, const CONTEXT &c) {
        where_into(w, sizeof w, c.Rip, true);
        report_writer.out.put("  #%-2d %016llx rsp=%016llx  %s\n", i, (unsigned long long)c.Rip, (unsigned long long)c.Rsp, w);
        return true;
    });
    if (frames <= 1)
        report_writer.out.put("  (the unwind stopped at the first frame)\n");
    report_writer.out.put("\n== Return addresses found on the stack (rsp to +0x%zx)\n", STACK_SCAN);
    uintptr_t end = std::min<uintptr_t>(hi, ctx.Rsp + STACK_SCAN);
    int shown = 0;
    Reader rd;
    for (uintptr_t p = ctx.Rsp & ~7ull; p + 8 <= end && shown < SCAN_FRAMES; p += 8) {
        uint64_t v;
        if (!rd.read(p, &v, 8) || !code_at(v) || !call_before(v))
            continue;
        where_into(w, sizeof w, v, true);
        report_writer.out.put("  rsp+0x%-5llx %016llx  %s\n", (unsigned long long)(p - ctx.Rsp), (unsigned long long)v, w);
        shown++;
    }
    if (!shown)
        report_writer.out.put("  none\n");
}

void section_state(const Request &r)
{
    report_writer.out.put("\n== Game state (from the black box)\n");
    blackbox::Copy c;
    for (const char *k : {"flow", "screen", "session", "race", "career", "heap", "exception"}) {
        if (recorder.events.last(k, c))
            report_writer.out.put("  last %-9s %s (%.1f s before this report)\n", k, c.text, double(r.ms - c.ms) / 1000);
        else
            report_writer.out.put("  last %-9s none recorded\n", k);
    }
    report_writer.out.put("  track and car count: core does not read them\n");
    char placed[256];
    runtime::placed_record_crash_text(placed, sizeof placed);
    report_writer.out.put("  %s\n", placed);
}

void section_blackbox(const Request &r)
{
    report_writer.out.put("\n== Black box: last events (%llu recorded, oldest first)\n",
                          (unsigned long long)recorder.events.total());
    if (!recorder.events.each([&](const blackbox::Copy &c) {
            report_writer.out.put("  -%9.3f s [%5lu] %-9s %s\n", double(r.ms - c.ms) / 1000, (unsigned long)c.tid, c.kind,
                                  c.text);
        }))
        report_writer.out.put("  none\n");
    report_writer.out.put("\n== Black box: last game log warnings and errors (%llu recorded, oldest first)\n",
                          (unsigned long long)recorder.warnings.total());
    if (!recorder.warnings.each([&](const blackbox::Copy &c) {
            report_writer.out.put("  -%9.3f s [%5lu] %s\n", double(r.ms - c.ms) / 1000, (unsigned long)c.tid, c.text);
        }))
        report_writer.out.put("  none\n");
}

void section_probes(const Request &r)
{
    report_writer.out.put("\n== Swofty probes (hook and callback activity)\n");
    static char text[48 << 10];
    text[0] = 0;
    perf::crash_text(text, sizeof text, r.probes, r.nprobes);
    report_text::each_line(text, [](const char *line, size_t len) { report_writer.out.put("  %.*s\n", int(len), line); });
}

void summary_line(const Request &r)
{
    bool prior = r.has_prior && !r.has_exception;
    std::string frames;
    char w[512];
    int kept = 0;
    walk(prior ? r.prior.ctx : r.ctx, prior ? r.prior.lo : r.lo, prior ? r.prior.hi : r.hi, 24, [&](int, const CONTEXT &c) {
        const Mod *m = modules.at(c.Rip);
        if (m && m->base == reinterpret_cast<uintptr_t>(process.self))
            return true;
        where_into(w, sizeof w, c.Rip, false);
        frames += (frames.empty() ? "" : " < ") + std::string(w);
        return ++kept < 4;
    });
    cl("[crash] crash report %s: %s; %sstack %s; minidump %s", report_writer.report_where, r.reason,
       prior ? "earlier exception's " : "", frames.c_str(), report_writer.dump_note);
}

void step_header(Request &r)
{
    char now[32];
    local_stamp(now, sizeof now, false);
    if (r.blame[0])
        report_writer.out.put("Swofty crash report: %s\n\n", r.blame);
    else
        report_writer.out.put("Swofty crash report\n\n");
    report_writer.out.put("written      %s local time\n", now);
    report_writer.out.put("reason       %s\n", r.reason);
    report_writer.out.put("thread       %lu%s\n", r.tid, r.kind == K_EXIT ? " (the exiting thread)" : "");
    report_writer.out.put("process      pid %lu, %.1f s after the core loaded\n", GetCurrentProcessId(),
                          double(r.ms - process.start_ms) / 1000);
    wchar_t path[MAX_PATH] = L"?";
    GetModuleFileNameW(nullptr, path, MAX_PATH);
    WIN32_FILE_ATTRIBUTE_DATA fa{};
    unsigned long long size = 0;
    if (GetFileAttributesExW(path, GetFileExInfoStandard, &fa))
        size = ((unsigned long long)fa.nFileSizeHigh << 32) | fa.nFileSizeLow;
    auto nt = reinterpret_cast<const IMAGE_NT_HEADERS *>(process.exe_base +
                                                         reinterpret_cast<const IMAGE_DOS_HEADER *>(process.exe_base)->e_lfanew);
    report_writer.out.put("exe          %ls, %llu bytes, PE timestamp %lu, image base 0x%llx\n", path, size,
                          (unsigned long)nt->FileHeader.TimeDateStamp, (unsigned long long)process.exe_base);
    report_writer.out.put("symbols      %zu KB function name(s)%s\n", symbol_table.symbols.size(),
                          symbol_table.matches ? "" : " (symbols.tsv is for another build or missing)");
    if (size_t bound = symbol_table.bound_count())
        report_writer.out.put("symbols      %zu function name(s) from the binding set\n", bound);
    report_writer.out.put("runtime      %s\n", process.wine_version ? fmt("Wine %s", process.wine_version()).c_str() : "Windows");
    report_writer.out.put("first-chance fatal-class exceptions before this report: %ld\n", capture.fatal_count);
}

void step_prior(Request &r)
{
    if (!r.has_prior)
        return;
    const Snapshot &p = r.prior;
    char w[512];
    where_into(w, sizeof w, p.ctx.Rip, true);
    report_writer.out.put("\n== Earlier fatal-class exception: %s (0x%08lx) on thread %lu, %.1f s before this report, at %s\n",
                          code_name(p.rec.ExceptionCode), p.rec.ExceptionCode, p.tid, double(r.ms - p.ms) / 1000, w);
    for (DWORD i = 0; i < p.rec.NumberParameters && i < 4; i++)
        report_writer.out.put("  parameter %lu: 0x%llx\n", i, (unsigned long long)p.rec.ExceptionInformation[i]);
    static Request pr;
    pr = r;
    pr.ctx = p.ctx;
    pr.lo = p.lo;
    pr.hi = p.hi;
    pr.tid = p.tid;
    section_registers(pr);
    section_stack(p.ctx, p.lo, p.hi, "Stack of the earlier exception");
}

void step_dred(Request &r)
{
    bool removal = false;
    auto look = [&](const CONTEXT &c) {
        const DWORD64 *v = &c.Rax;
        for (int i = 0; i < 16; i++)
            removal = removal || runtime::dred::is_removal_hresult(v[i]);
    };
    look(r.ctx);
    if (r.has_prior)
        look(r.prior.ctx);
    std::string text = runtime::dred::section(removal);
    if (text.empty())
        return;
    report_writer.out.put("%s", text.c_str());
    std::string builds = render::d3d12::rtas_recent_text(runtime::dred::build_stops());
    report_text::indented_lines(
        builds, [](const char *line, size_t len) { report_writer.out.put("%.*s", int(len), line); },
        [](const char *line, size_t len) { logger.api->log("[gfx] rtas recent: %.*s", int(len), line); });
}

void step_close(Request &)
{
    CloseHandle(report_writer.out.file);
    report_writer.out.file = INVALID_HANDLE_VALUE;
}

const Step STEPS[NSTEPS] = {
    {"the header", step_header},
    {"the Exception section", [](Request &r) { section_exception(r); }},
    {"the Registers section", [](Request &r) { section_registers(r); }},
    {"the Stack section",
     [](Request &r) { section_stack(r.ctx, r.lo, r.hi, "Stack (unwound with the modules' exception data)"); }},
    {"the Earlier fatal-class exception section", step_prior},
    {"the Code around rip and Memory sections", [](Request &r) { section_memory(r); }},
    {"the Ne heap section", [](Request &r) { section_heap(r); }},
    {"the Game state section", [](Request &r) { section_state(r); }},
    {"the GPU device removal section", step_dred},
    {"the Black box sections", [](Request &r) { section_blackbox(r); }},
    {"the Swofty probes section", [](Request &r) { section_probes(r); }},
    {"the registry reports", [](Request &) { swofty_core::apply::moments::crash_reports(); }},
    {"the Addons and Loaded modules sections", [](Request &) { section_modules(); }},
    {"the Threads section",
     [](Request &r) {
         if (r.loader_lock)
             report_writer.out.put("\n== Threads\n  not listed: the reporting thread holds the loader lock\n");
         else
             section_threads(r);
     }},
    {"the loader.log section", [](Request &) { section_log(); }},
    {"the Minidump section",
     [](Request &r) {
         SetEvent(report_writer.text_event);
         minidump(r);
         report_writer.out.put("\n== Minidump\n  %s\n", report_writer.dump_note);
     }},
    {"closing the report", step_close},
    {"the summary line", [](Request &r) { summary_line(r); }},
};

void read_addons(const std::wstring &path)
{
    std::string text;
    if (FILE *f = _wfopen(path.c_str(), L"rb")) {
        char b[65536];
        for (size_t n; (n = fread(b, 1, sizeof b, f));)
            text.append(b, n);
        fclose(f);
    }
    sections.addon_text = report_text::addon_text(text, fmt);
}

static_assert(sizeof STEPS / sizeof *STEPS == NSTEPS);

}
