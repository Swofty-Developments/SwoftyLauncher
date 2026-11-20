#pragma once

#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <windows.h>

#include <string>

#include "core/diagnostics/crash/crash_heap.hpp"
#include "core/diagnostics/crash/request.hpp"

namespace crash {

struct Step {
    const char *name;
    void (*run)(Request &);
};

constexpr LONG NSTEPS = 18, CLOSE_STEP = NSTEPS - 2, MAX_WRITER_FAULTS = 8;

/* The report is written one step at a time, so a fault in one step costs that step only: the writer
   notes it and goes on with the next (writer_resume). */
extern const Step STEPS[NSTEPS];

// What the report sections keep between steps: the Ne heaps found around the fault and the addon list read at attach.
struct ReportSections {
    neheap::Heap heaps[8];
    int nheaps;
    std::string addon_text;
};

extern ReportSections sections;

void read_addons(const std::wstring &path);

// The sections the steps write from report_memory.cpp: the exception, the memory at the registers and the Ne heap.
void section_exception(const Request &r);
void section_memory(const Request &r);
void section_heap(const Request &r);

// The sections the steps write from report_process.cpp: the addons and modules, the threads and loader.log's end.
void section_modules();
void section_threads(const Request &r);
void section_log();

}
