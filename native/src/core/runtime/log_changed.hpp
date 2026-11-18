#pragma once

#include <string>

#include <swofty/mixin.hpp>

namespace runtime {

// A loader.log line written only when it differs from the last one written through the same object. `format`
// takes the line as its one %s.
class LogChanged {
public:
    constexpr explicit LogChanged(const char *format) : format_(format) {}

    void say(const std::string &line)
    {
        if (line == last_)
            return;
        last_ = line;
        swofty::log(format_, line.c_str());
    }

    void forget() { last_.clear(); }

private:
    const char *format_;
    std::string last_;
};

}
