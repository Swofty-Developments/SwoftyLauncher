#pragma once
#include <chrono>

// Durations of the phases of a boot or a load, one loader.log line each, all in the one format
//   [perf] step <milliseconds> ms: <name>
// with the milliseconds to one decimal and the name free text that never holds a newline, so field logs
// match ^\[perf\] step ([0-9]+\.[0-9]) ms: (.+)$ whatever the name. The line format is stable: tools read it.

namespace perf {

inline constexpr const char *STEP_LINE = "[perf] step %.1f ms: %s";

class Stopwatch {
public:
    Stopwatch() : start_(std::chrono::steady_clock::now()), lap_(start_) {}

    double total_ms() const { return ms(start_, std::chrono::steady_clock::now()); }

    double lap_ms()
    {
        auto now = std::chrono::steady_clock::now();
        double out = ms(lap_, now);
        lap_ = now;
        return out;
    }

private:
    using Clock = std::chrono::steady_clock;
    static double ms(Clock::time_point a, Clock::time_point b)
    {
        return std::chrono::duration<double, std::milli>(b - a).count();
    }
    Clock::time_point start_, lap_;
};

}
