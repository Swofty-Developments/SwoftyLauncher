#pragma once

namespace runtime {

// Writes load-stall report index of at most max (stall-<time>.txt in the crash folder): why, the loading state and
// every game thread's stack. False while a crash report or another stall report is being written, or without memory.
bool stall_report(int index, int max, const char *why, const char *state);

}
