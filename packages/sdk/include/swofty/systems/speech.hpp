// SPDX-License-Identifier: MIT
#pragma once
#include <cstddef>
#include <cstdint>
#include <string>
#include <string_view>
#include <swofty/events.hpp>

// Voice lines: the core wraps Game_SpeechSystem_PostLine, the function the game's speech system posts every voice line
// through (the race engineer's radio, the driver's replies, ...), and fires three notify events per line on the flow
// thread: speech.line when the game posts it, speech.line.timed once the audio engine reports its real length, and
// speech.line.end when it stops. The three payloads share one layout. A line's text is the lng_* key
// "lng_subtitle_" + stem (swofty::lang::subtitle in swofty/lang.hpp). Everything needs the feature events on a build
// where the speech line layout is proven (the KB build and Steam 1.0.145); elsewhere live() is false and nothing fires.

namespace swofty::speech {

inline constexpr size_t SPEAKER_CHARS = 32;
inline constexpr size_t STEM_CHARS = 96;

// The line's category byte (the game's play_speech_<speaker>_<type> event picks its type by it). 2 is team radio.
inline constexpr uint8_t CATEGORY_RADIO = 2;

// Why a line ended: finished (the audio engine ended it), recycled (the game reused the line for another one before
// it was seen ending) or lost (the line could no longer be read, the session changed, or it ran past its time).
enum class End : uint8_t { finished = 0, recycled = 1, lost = 2 };

inline const char *name(End e)
{
    switch (e) {
    case End::finished:
        return "finished";
    case End::recycled:
        return "recycled";
    default:
        return "lost";
    }
}

namespace events {

// id is the game's line id, unique while the line lives. speaker is the speech system's speaker name: "engineer1"
// for the race engineer, "drivers" for a driver's reply; speaker_index its index in the speaker table. stem is the
// subtitle key stem ("eng_radiocheck_02"), empty when the line has none. duration_ms is 0 in speech.line, the real
// length in speech.line.timed, and in speech.line.end the length if the audio engine reported one, else 0. played_ms
// is the time since speech.line on the core's clock (0 in speech.line). reason is set in speech.line.end only.
struct Line {
    static constexpr const char *point = "speech.line";
    static constexpr uint32_t version = 1;
    uint32_t size, payload_version;
    uint32_t id;
    uint32_t duration_ms;
    uint32_t played_ms;
    uint32_t speaker_index;
    uint8_t category;
    uint8_t retrigger;
    End reason;
    uint8_t reserved;
    char speaker[SPEAKER_CHARS];
    char stem[STEM_CHARS];
};

struct Timed : Line {
    static constexpr const char *point = "speech.line.timed";
};

struct Ended : Line {
    static constexpr const char *point = "speech.line.end";
};

}

inline bool radio(const events::Line &l) { return l.category == CATEGORY_RADIO; }

inline std::string_view speaker(const events::Line &l)
{
    size_t n = 0;
    while (n < SPEAKER_CHARS && l.speaker[n])
        n++;
    return std::string_view(l.speaker, n);
}

inline std::string_view stem(const events::Line &l)
{
    size_t n = 0;
    while (n < STEM_CHARS && l.stem[n])
        n++;
    return std::string_view(l.stem, n);
}

// The race engineer speaks as "engineer1" (and any other "engineer..." speaker).
inline bool engineer(const events::Line &l) { return speaker(l).starts_with("engineer"); }

// A driver's radio reply.
inline bool driver(const events::Line &l) { return speaker(l) == "drivers"; }

// "lng_subtitle_" + stem, or empty when the line has no stem.
inline std::string subtitle_key(const events::Line &l)
{
    std::string_view s = stem(l);
    return s.empty() ? std::string() : std::string("lng_subtitle_").append(s);
}

// True when the core fires the speech events in this game: the events feature is on, and the build's speech line
// layout is proven.
inline bool live()
{
    auto t = swofty::detail::hooks_table();
    uint32_t v = 0;
    return t && t->live && t->live(events::Line::point, &v) == SWOFTY_ASSET_OK && v >= events::Line::version;
}

}
