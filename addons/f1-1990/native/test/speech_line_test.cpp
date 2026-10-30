// SPDX-License-Identifier: AGPL-3.0-or-later
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <vector>

#include "speech_line.hpp"

static int failures;

static void check(const char *name, bool ok)
{
    std::printf("%s %s\n", ok ? "ok  " : "FAIL", name);
    failures += !ok;
}

template <class T> static void put(void *p, size_t off, T v)
{
    std::memcpy(static_cast<uint8_t *>(p) + off, &v, sizeof v);
}

int main()
{
    const char *names[] = {"engineer", "agent", "agent_cutscene", "teammate"};
    std::vector<uint8_t> records[4], table(0x60 + 4 * 0x88), speech(0x800), line(0x160);
    for (int i = 0; i < 4; i++) {
        records[i].resize(0x20);
        put(records[i].data(), 0x18, names[i]);
        put(table.data(), 0x60 + i * 0x88, records[i].data());
    }
    put(speech.data(), 0x728, table.data());
    put(speech.data(), 0x7cc, uint32_t(4));
    for (uint64_t i = 0; i < 4; i++) {
        put(line.data(), 0x80, i);
        const char *s = f1990::speech_speaker(speech.data(), line.data());
        check(names[i], s && !std::strcmp(s, names[i]));
    }
    put(line.data(), 0x80, uint64_t(4));
    check("index past the speaker count has no speaker", f1990::speech_speaker(speech.data(), line.data()) == nullptr);
    check("agent is the agent", f1990::is_agent_speaker("agent"));
    check("agent_cutscene is the agent", f1990::is_agent_speaker("agent_cutscene"));
    check("engineer is not", !f1990::is_agent_speaker("engineer"));
    check("agent_x is not", !f1990::is_agent_speaker("agent_x"));
    check("no speaker is not", !f1990::is_agent_speaker(nullptr));
    if (failures)
        std::printf("%d failure(s)\n", failures);
    return failures ? EXIT_FAILURE : EXIT_SUCCESS;
}
