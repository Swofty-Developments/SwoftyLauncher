#pragma once

#include <cctype>
#include <cstddef>
#include <cstdint>
#include <cstring>
#include <string>

#include "core/runtime/json.hpp"

// The parts of the crash and load-stall reports' text that read no process state, which test/report_text_test.cpp
// pins: the Addons list, the report's lines, the ray tracing build lines, the loader.log tail, the pointers in a heap
// error message and the call before a return address.
namespace crash::report_text {

// Calls line(start, len) for each line of text, without its '\n'.
template <class Line> void each_line(char *text, Line line)
{
    for (char *at = text; *at;) {
        char *nl = strchr(at, '\n');
        size_t len = nl ? size_t(nl - at) : strlen(at);
        line(at, len);
        at += len + (nl ? 1 : 0);
    }
}

// Calls put(start, len) for each line of text with its '\n', and log(start, len) for the same line without its
// leading spaces and '\n' when anything is left.
template <class Put, class Log> void indented_lines(const std::string &text, Put put, Log log)
{
    for (size_t at = 0; at < text.size();) {
        size_t end = text.find('\n', at);
        end = end == std::string::npos ? text.size() : end + 1;
        put(text.data() + at, end - at);
        size_t from = text.find_first_not_of(' ', at), to = text[end - 1] == '\n' ? end - 1 : end;
        if (from < to)
            log(text.data() + from, to - from);
        at = end;
    }
}

// The Addons section's lines from settings.json's text; fmt is printf-like and returns a std::string.
template <class Fmt> std::string addon_text(const std::string &text, Fmt fmt)
{
    json::Value doc;
    if (text.empty() || !json::parse(text, doc))
        return text.empty() ? "  no settings.json\n" : "  settings.json is not valid JSON\n";
    const json::Value &list = doc.field("addons", "mods");
    std::string out = fmt("  profile '%s', %zu addon(s)\n", doc["profile"].str("?").c_str(), list.a.size());
    for (auto &a : list.a) {
        std::string dlls;
        for (auto &d : a["dlls"].a)
            dlls += (dlls.empty() ? "" : ", ") + d.str();
        out += fmt("  %-36s %-10s %-3s %s\n", a["id"].str("?").c_str(), a["version"].str("?").c_str(),
                   a["enabled"].type == json::Value::Bool && !a["enabled"].b ? "off" : "on",
                   dlls.empty() ? "(no dll)" : dlls.c_str());
    }
    return out;
}

// The last max_lines lines of the got bytes in tail (which holds got + 1 bytes), with every '\r' dropped in place.
inline char *log_tail(char *tail, size_t got, int max_lines)
{
    tail[got] = 0;
    int lines = 0;
    char *start = tail + got;
    if (start > tail && start[-1] == '\n')
        start--;
    while (start > tail && lines < max_lines)
        if (*--start == '\n')
            lines++;
    if (*start == '\n')
        start++;
    char *w = start;
    for (char *p = start; *p; p++)
        if (*p != '\r')
            *w++ = *p;
    *w = 0;
    return start;
}

// Up to max values of 8 to 16 hex digits (with or without 0x) in msg, appended to found from found[n].
inline void scan_message_pointers(const char *msg, uintptr_t *found, int &n, int max)
{
    for (const char *p = msg; *p && n < max; p++) {
        const char *s = p;
        if (s[0] == '0' && (s[1] == 'x' || s[1] == 'X'))
            s += 2;
        int digits = 0;
        uint64_t v = 0;
        while (digits < 17 && isxdigit((unsigned char)s[digits])) {
            char c = s[digits];
            v = v * 16 + uint64_t(c <= '9' ? c - '0' : (c | 0x20) - 'a' + 10);
            digits++;
        }
        if (digits >= 8 && digits <= 16) {
            found[n++] = uintptr_t(v);
            p = s + digits - 1;
        }
    }
}

// Whether the 8 bytes before a return address end in a call: E8 rel32, or FF /2 of 2, 3, 4, 6 or 7 bytes.
inline bool call_before(const uint8_t b[8])
{
    if (b[3] == 0xE8)
        return true;
    for (int len : {2, 3, 4, 6, 7})
        if (b[8 - len] == 0xFF && ((b[8 - len + 1] >> 3) & 7) == 2)
            return true;
    return false;
}

}
