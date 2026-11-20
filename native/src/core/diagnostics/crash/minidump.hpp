#pragma once

#include "core/diagnostics/crash/request.hpp"

namespace crash {

// Writes the minidump beside the report and leaves a one-line note on it in report_writer.dump_note.
void minidump(Request &r);

}
