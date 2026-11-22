#include "core/audio/speech_logic.hpp"
#include "core/hooks/core_events.hpp"
#include "core/runtime/build.hpp"
#include "core/runtime/build_proof.hpp"
#include "core/runtime/loader/core_sets.hpp"
#include "core/runtime/loader/introspection.hpp"
#include "core/runtime/loader/mixin_gate.hpp"
#include "core/runtime/runtime.hpp"
#include "core/runtime/stage.hpp"

#include <windows.h>

#include <swofty/game/audio.hpp>
#include <swofty/mem.hpp>
#include <swofty/mixin.hpp>

#include <atomic>
#include <cstddef>
#include <cstdint>
#include <cstring>
#include <string>
#include <vector>

// Speech events (core/audio/speech_logic.hpp, docs/reference/std/speech.md): a wrap of Game_SpeechSystem_PostLine at
// priority -100, below the addons' wraps and the characters voice mute, so a line an addon's wrap does not pass on
// fires nothing. The wrap reads the line after the game has posted it and queues it; the flow tick fires speech.line, speech.line.timed and
// speech.line.end. On with the feature events, on a build where the speech line layout is proven;
// SWOFTY_SPEECH_EVENTS=0 turns it off.

namespace {

namespace game = swofty::game;
static_assert(offsetof(game::SpeechSystem, speakers) == swofty_core::speech::SPEAKERS);
static_assert(offsetof(game::SpeechSystem, speaker_count) == swofty_core::speech::SPEAKER_COUNT);
static_assert(offsetof(game::SpeechSpeaker, name) == swofty_core::speech::SPEAKER_NAME);
static_assert(offsetof(game::SpeechLine, speaker) == swofty_core::speech::LINE_SPEAKER);
static_assert(offsetof(game::SpeechLine, duration_ms) == swofty_core::speech::LINE_DURATION);
static_assert(offsetof(game::SpeechLine, id) == swofty_core::speech::LINE_ID);
static_assert(offsetof(game::SpeechLine, subtitle_stem) == swofty_core::speech::LINE_STEM);
static_assert(offsetof(game::SpeechLine, category) == swofty_core::speech::LINE_CATEGORY);
static_assert(offsetof(game::SpeechLine, state) == swofty_core::speech::LINE_STATE);

namespace bp = runtime::build_proof;
namespace sl = swofty_core::speech;
namespace mem = swofty::mem;

constexpr const char *MIXIN = "swofty_core.dll (speech events)";
constexpr const char *POST_FN = "Game_SpeechSystem_PostLine";
constexpr const char *const BINDINGS[] = {POST_FN};
constexpr bp::Proof LAYOUT{"the speech line layout (line +0x80 speaker index, +0x88 duration, +0x8c id, +0x138 subtitle stem, "
                           "+0x140 category, +0x144 state; the speech system's speaker table +0x728 and count +0x7cc)",
                           bp::bit(bp::Build::kb) | bp::bit(bp::Build::steam_1_0_145)};
constexpr bp::Gate GATE{"[speech] voice line events", &LAYOUT, BINDINGS};
constexpr int32_t PRIORITY = -100;

using PostLine = void(void *speech, void *line, uint8_t retrigger);

PostLine *post_orig = nullptr;
runtime::loader::MixinGate gate;
std::atomic<bool> wanted{false}, armed{false};
sl::Tracker tracker;
std::atomic<uint32_t> posted{0};

double now() { return double(GetTickCount64()) / 1000.0; }

bool switched_on()
{
    char v[8] = {};
    DWORD n = GetEnvironmentVariableA("SWOFTY_SPEECH_EVENTS", v, sizeof v);
    return !(n && n < sizeof v && !strcmp(v, "0"));
}

bool rd_str(uintptr_t at, std::string &out, size_t max) { return mem::c_string(at, out, max); }

void post_entry(void *speech, void *line, uint8_t retrigger)
{
    post_orig(speech, line, retrigger);
    if (line) {
        sl::Posted p;
        auto rd = [](uintptr_t at, auto &v) { return mem::read(at, v); };
        if (sl::capture(reinterpret_cast<uintptr_t>(speech), reinterpret_cast<uintptr_t>(line), retrigger, now(), rd, rd_str,
                        p)) {
            uint32_t n = ++posted;
            if (n <= 4)
                swofty::log("[speech] line %u posted: speaker %s, category %u, stem %s", p.id,
                            p.speaker.empty() ? "?" : p.speaker.c_str(), unsigned(p.category),
                            p.stem.empty() ? "(none)" : p.stem.c_str());
            tracker.post(std::move(p));
        }
    }
}

sl::Read read_line(uintptr_t line)
{
    sl::Read r;
    r.ok = mem::read(line + sl::LINE_ID, r.id) && mem::read(line + sl::LINE_DURATION, r.duration_ms) &&
           mem::read(line + sl::LINE_STATE, r.state);
    return r;
}

void emit(const sl::Moment &m)
{
    namespace se = swofty::speech::events;
    switch (m.kind) {
    case sl::Kind::line: {
        se::Line p = m.payload;
        swofty_core::hooks::events::speech_line(p);
        break;
    }
    case sl::Kind::timed: {
        se::Timed p;
        static_cast<se::Line &>(p) = m.payload;
        swofty_core::hooks::events::speech_line_timed(p);
        break;
    }
    case sl::Kind::ended: {
        se::Ended p;
        static_cast<se::Line &>(p) = m.payload;
        swofty_core::hooks::events::speech_line_end(p);
        break;
    }
    }
}

std::vector<runtime::CoreMixins> speech_mixins()
{
    std::vector<runtime::CoreMixins> out;
    if (gate.offered() || !runtime::code_ready())
        return out;
    gate.offer();
    if (!runtime::enabled("events", nullptr))
        return out;
    if (!switched_on()) {
        swofty::log("[speech] voice line events off (SWOFTY_SPEECH_EVENTS=0)");
        return out;
    }
    if (!runtime::holds(GATE))
        return out;
    wanted = true;
    out.push_back({MIXIN, {{POST_FN, SWOFTY_MIXIN_WRAP, PRIORITY, (void *)&post_entry, (void **)&post_orig}}});
    return out;
}

void speech_commit()
{
    if (!wanted || !gate.settle())
        return;
    armed = runtime::core_mixins_active(MIXIN);
    swofty::log("[speech] voice line events %s",
                runtime::loader::install_state(armed.load(), "armed: speech.line, speech.line.timed and speech.line.end "
                                                             "fire from Game_SpeechSystem_PostLine"));
}

runtime::loader::FunctionSet speech_core_set{runtime::loader::CoreSetId::speech, speech_mixins, speech_commit};

}

bool runtime::speech_live() { return armed.load() || (wanted.load() && !gate.settled()); }

void runtime::speech_tick()
{
    if (!armed.load())
        return;
    tracker.tick(now(), runtime::session_serial(), read_line, emit);
}
