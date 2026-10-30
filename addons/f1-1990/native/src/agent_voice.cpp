// SPDX-License-Identifier: AGPL-3.0-or-later
//
// F1 1990 - the agent is Flavio, and silent, in 1990 careers only. Its own DLL, so a binding it
// can't resolve drops only this hook.
//
// Game_SpeechSystem_PostLine posts each speech line as play_speech_<speaker>_<type> on the line's
// emitter, with the speech system's callback object, whose end-of-event and duration handlers move
// the line's state; a line that is never posted never finishes, so the line is still posted. For
// the speakers agent and agent_cutscene, while the loaded career's data group is f1_career_1990,
// the emitter's vol_speech RTPC (the speech volume the audio options set) is set to 0 first through
// the emitter's own RTPC setter (vtable +0x60, the call the post makes for random_number_generator),
// so the line plays silently with its subtitles and callbacks. The 1990 state is worked out on the
// core's career.loaded event and cleared on career.unloaded. Option agent_voice_mute turns it off.
//
// The stock strings that name the agent are rewritten for Flavio per language in
// data/flavio-strings.json. With a core that has runtime string overrides (API v8), each text is
// set for its own language when a 1990 career loads and cleared again when it unloads, so every
// other career and mode keeps Elena's strings. The file is read on the first 1990 career load.

#include <swofty/mixin.hpp>

SWOFTY_SETTING(bool, agent_voice_mute, true);

#ifdef SWOFTY_API_CHARACTERS
// With a core that has the characters library the core owns the speech hook and the per-career
// strings: the agent's speakers are muted and data/flavio-strings.json is handed to the core for
// the 1990 data group, which sets the strings on a 1990 career load and clears them on unload.
#include <string>

#include <swofty/characters.hpp>
#include <swofty/storage.hpp>

#include "career_group.hpp"

SWOFTY_ON_LOAD
{
    namespace ch = swofty::characters;
    const char *group = f1990::kCareerGroup1990;
    if (agent_voice_mute) {
        ch::Result a = ch::mute(group, "agent"), b = ch::mute(group, "agent_cutscene");
        swofty::log("[f1-1990] voice: agent muted in %s careers through the core's characters library (%s, %s)", group,
                    ch::name(a), ch::name(b));
    } else {
        swofty::log("[f1-1990] voice: agent mute in 1990 careers off (agent_voice_mute)");
    }
    std::string json;
    if (auto r = swofty::storage::read("data/flavio-strings.json", json); r != swofty::storage::Result::ok)
        swofty::log("[f1-1990] strings: read data/flavio-strings.json: %s; the 1990 agent keeps Elena's strings",
                    swofty::storage::name(r));
    else
        swofty::log("[f1-1990] strings: agent strings for %s careers handed to the core (%s)", group,
                    ch::name(ch::strings_json(group, json)));
}
#else
#include <atomic>
#include <cstring>
#include <mutex>
#include <set>

#include <swofty/career.hpp>
#include <swofty/events.hpp>
#include <swofty/game.hpp>
#include "career_1990.hpp"
#include "speech_line.hpp"

#if SWOFTY_API_VERSION >= 8
#include <swofty/lang.hpp>
#include <swofty/storage.hpp>

#include "json_strings.hpp"
#endif

namespace {

using f1990::at;

constexpr size_t kRtpcSlot = 0x60 / 8;
constexpr char kSpeechVolume[] = "vol_speech";

std::atomic<int> g_1990{-1};
std::atomic<uint32_t> g_muted_lines, g_logged;
std::mutex g_lock;
std::set<void *> g_muted_emitters;

#if SWOFTY_API_VERSION >= 8
constexpr char kStringsFile[] = "data/flavio-strings.json";

std::vector<f1990::AgentStrings> g_strings;
bool g_strings_read, g_strings_set, g_strings_off_logged;

bool read_strings()
{
    if (g_strings_read)
        return !g_strings.empty();
    g_strings_read = true;
    std::string json;
    size_t error_at = 0;
    if (auto r = swofty::storage::read(kStringsFile, json); r != swofty::storage::Result::ok)
        swofty::log("[f1-1990] strings: read %s: %s; the 1990 agent keeps Elena's strings", kStringsFile,
                    swofty::storage::name(r));
    else if (!f1990::parse_agent_strings(json, g_strings, &error_at))
        swofty::log("[f1-1990] strings: %s is not an object of language objects of strings (byte %zu); the 1990 "
                    "agent keeps Elena's strings",
                    kStringsFile, error_at);
    return !g_strings.empty();
}

void agent_strings(bool on)
{
    namespace lg = swofty::lang;
    if (on == g_strings_set)
        return;
    if (!lg::available()) {
        if (!g_strings_off_logged)
            swofty::log("[f1-1990] strings: the core has no runtime string overrides; the 1990 agent keeps Elena's "
                        "strings");
        g_strings_off_logged = true;
        return;
    }
    if (on && !read_strings())
        return;
    size_t calls = 0, failed = 0;
    for (auto &lang : g_strings)
        for (auto &[key, text] : lang.texts) {
            lg::Result r = on ? lg::set(key, text, lang.code) : lg::clear(key, lang.code);
            calls++;
            failed += r != lg::Result::ok;
        }
    g_strings_set = on;
    swofty::log("[f1-1990] strings: %s %zu agent string(s) in %zu language(s) (%zu failed)", on ? "set" : "cleared",
                calls, g_strings.size(), failed);
}
#else
void agent_strings(bool)
{
}
#endif

void on_career(const SwoftyEvent *ev, void *)
{
    if (!std::strcmp(ev->name, SWOFTY_EVENT_CAREER_UNLOADED)) {
        if (g_1990.exchange(0) == 1)
            swofty::log("[f1-1990] voice: 1990 career unloaded; the agent speaks again");
        agent_strings(false);
        std::lock_guard<std::mutex> g(g_lock);
        g_muted_emitters.clear();
        return;
    }
    void *career = swofty::career::entity();
    bool yes = career && f1990::is_1990_career(career);
    g_1990 = yes ? 1 : 0;
    agent_strings(yes);
    swofty::log("[f1-1990] voice: career loaded (%s); agent voice %s", yes ? "data group f1_career_1990" : "not a 1990 career",
                yes && agent_voice_mute ? "muted" : "stock");
}

void subscribe(const char *event)
{
    uint32_t token = 0;
    swofty::events::Result r = swofty::events::subscribe(event, on_career, nullptr, &token);
    if (r != swofty::events::Result::ok)
        swofty::log("[f1-1990] voice: subscribe %s: %s; the agent keeps her voice", event, swofty::events::name(r));
}

void mute(void *emitter, const char *speaker)
{
    auto **vt = *static_cast<void ***>(emitter);
    if (!vt || !vt[kRtpcSlot])
        return;
    reinterpret_cast<void (*)(void *, const char *, float, int, float, int)>(vt[kRtpcSlot])(emitter, kSpeechVolume, 0.f, 0,
                                                                                           0.f, 0);
    bool fresh;
    {
        std::lock_guard<std::mutex> g(g_lock);
        fresh = g_muted_emitters.insert(emitter).second;
    }
    if (fresh || g_logged++ < 8)
        swofty::log("[f1-1990] voice: %s line muted (emitter %p, %u muted so far)", speaker, emitter, ++g_muted_lines);
}

}

SWOFTY_WRAP(Game_SpeechSystem_PostLine, void, void *speech, void *line, uint8_t retrigger)
{
    if (speech && line && g_1990.load() == 1 && agent_voice_mute) {
        const char *speaker = f1990::speech_speaker(speech, line);
        void *emitter = at<void *>(line, 0x28);
        if (emitter && f1990::is_agent_speaker(speaker))
            mute(emitter, speaker);
        else if (emitter) {
            bool shared;
            {
                std::lock_guard<std::mutex> g(g_lock);
                shared = g_muted_emitters.count(emitter) != 0;
            }
            if (shared && g_logged++ < 16)
                swofty::log("[f1-1990] voice: emitter %p muted for the agent also speaks %s; that line is silent too",
                            emitter, speaker ? speaker : "(unknown)");
        }
    }
    orig(speech, line, retrigger);
}

SWOFTY_ON_LOAD
{
    swofty::log("[f1-1990] voice: agent mute in 1990 careers %s", agent_voice_mute ? "on" : "off (agent_voice_mute)");
#if SWOFTY_API_VERSION < 8
    swofty::log("[f1-1990] strings: built against SDK API v%d, without runtime string overrides; the 1990 agent keeps "
                "Elena's strings",
                SWOFTY_API_VERSION);
#endif
    subscribe(SWOFTY_EVENT_CAREER_LOADED);
    subscribe(SWOFTY_EVENT_CAREER_UNLOADED);
}
#endif

SWOFTY_ADDON()
