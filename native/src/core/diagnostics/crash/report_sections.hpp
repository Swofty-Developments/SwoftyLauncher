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

}
