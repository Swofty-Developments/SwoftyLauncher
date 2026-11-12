#pragma once

#include <cstddef>
#include <cstdint>
#include <cstdio>
#include <string>
#include <unordered_set>

namespace runtime::track_list_guard {

constexpr uintptr_t ENTRY = 0x28;
constexpr uintptr_t CALENDAR_BEGIN = 0x2b0;
constexpr uintptr_t CALENDAR_END = 0x2b8;

struct Load {
    uintptr_t component = 0;
    std::unordered_set<uintptr_t> rows;
    size_t repeats = 0;
};

inline size_t entries(uintptr_t begin, uintptr_t end)
{
    return end > begin ? (end - begin) / ENTRY : 0;
}

inline bool take(Load &load, uintptr_t component, size_t calendar, uintptr_t row)
{
    if (calendar == 0 || component != load.component) {
        load.component = component;
        load.rows.clear();
        load.repeats = 0;
    }
    if (load.rows.insert(row).second)
        return true;
    ++load.repeats;
    return false;
}

inline bool last_entry(uint64_t index, uint64_t stored)
{
    return index + 1 >= stored;
}

inline size_t finish(Load &load, uintptr_t component)
{
    size_t repeats = load.component == component ? load.repeats : 0;
    load.component = 0;
    load.rows.clear();
    load.repeats = 0;
    return repeats;
}

inline std::string load_line(uintptr_t component, size_t stored, size_t loaded, size_t repeats, bool guarded)
{
    char buf[400];
    if (!guarded)
        snprintf(buf, sizeof buf, "[saves] custom_track_list loaded (schedule component 0x%llx): %zu stored entr%s -> %zu calendar event%s",
                 (unsigned long long)component, stored, stored == 1 ? "y" : "ies", loaded, loaded == 1 ? "" : "s");
    else if (!repeats)
        snprintf(buf, sizeof buf, "[saves] custom_track_list loaded (schedule component 0x%llx): %zu stored entr%s -> %zu calendar event%s, no repeats",
                 (unsigned long long)component, stored, stored == 1 ? "y" : "ies", loaded, loaded == 1 ? "" : "s");
    else
        snprintf(buf, sizeof buf,
                 "[saves] custom_track_list loaded (schedule component 0x%llx): %zu stored entr%s -> %zu calendar event%s; %zu repeat "
                 "match%s of an event already in the calendar skipped (the game alone would load %zu)",
                 (unsigned long long)component, stored, stored == 1 ? "y" : "ies", loaded, loaded == 1 ? "" : "s", repeats,
                 repeats == 1 ? "" : "es", loaded + repeats);
    return buf;
}

inline std::string save_line(uintptr_t component, size_t count)
{
    char buf[200];
    snprintf(buf, sizeof buf, "[saves] custom_track_list saved (schedule component 0x%llx): %zu calendar event%s",
             (unsigned long long)component, count, count == 1 ? "" : "s");
    return buf;
}

struct SaveLog {
    uintptr_t component[8] = {};
    size_t count[8] = {};
    size_t next = 0;

    bool changed(uintptr_t comp, size_t n)
    {
        for (size_t i = 0; i < 8; i++)
            if (component[i] == comp) {
                if (count[i] == n)
                    return false;
                count[i] = n;
                return true;
            }
        component[next] = comp;
        count[next] = n;
        next = (next + 1) % 8;
        return true;
    }
};

inline bool switch_off(const char *value)
{
    if (!value)
        return false;
    return (value[0] == '0' && value[1] == 0) || (value[0] == 'o' && value[1] == 'f' && value[2] == 'f' && value[3] == 0);
}

}
