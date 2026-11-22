---
title: Voice lines
description: Follow the race engineer's radio, the driver's replies and every other voice line with the speech.line events from swofty/systems/speech.hpp.
---

# Voice lines

The core fires an event every time the game posts a voice line: the race engineer on the radio, the driver answering, the commentators. Each line carries its speaker, its category, its subtitle key stem and its id. A second event follows once the audio engine knows how long the line is, and a third when it stops. A radio card addon shows the engineer's words while he talks, a transcript addon writes every radio call to a file, and a league addon counts how often each driver is told to box.

```cpp [native/src/radio.cpp]
#include <swofty/events.hpp>
#include <swofty/lang.hpp>
#include <swofty/systems/speech.hpp>

namespace ev = swofty::events;
namespace lg = swofty::lang;
namespace sp = swofty::speech;

SWOFTY_ON_LOAD
{
    if (!sp::live()) {
        swofty::log("[radio] this core does not fire voice line events");
        return;
    }
    ev::on<sp::events::Line>().after([](const sp::events::Line &l, const ev::Context &) {
        if (!sp::radio(l) || !(sp::engineer(l) || sp::driver(l)))
            return;
        // "lng_subtitle_" + stem in the game's current language; empty for a line without a subtitle
        std::string text = lg::subtitle(sp::stem(l)).value_or("");
        swofty::log("[radio] %s %u: %s", sp::engineer(l) ? "engineer" : "driver", l.id, text.c_str());
    });
    ev::on<sp::events::Timed>().after([](const sp::events::Timed &l, const ev::Context &) {
        swofty::log("[radio] line %u lasts %u ms", l.id, l.duration_ms);
    });
    ev::on<sp::events::Ended>().after([](const sp::events::Ended &l, const ev::Context &) {
        swofty::log("[radio] line %u %s after %u ms", l.id, sp::name(l.reason), l.played_ms);
    });
}

SWOFTY_ADDON()
```

```text [swofty/loader.log]
[speech] voice line events armed: speech.line, speech.line.timed and speech.line.end fire from Game_SpeechSystem_PostLine
[speech] line 41 posted: speaker engineer1, category 2, stem eng_radiocheck_03
[radio] engineer 41: Radio check, how do you hear me?
[radio] line 41 lasts 2100 ms
[radio] line 41 finished after 2934 ms
```

- **Header:** `swofty/systems/speech.hpp`, namespace `swofty::speech`; host test fake `swofty/fake_speech.hpp`
- **Table:** the hooks table, API 9
- **Feature:** `events`, stable
- **Thread:** the three events fire on the flow thread

## The events

```cpp
namespace swofty::speech {

inline constexpr size_t SPEAKER_CHARS = 32;
inline constexpr size_t STEM_CHARS = 96;
inline constexpr uint8_t CATEGORY_RADIO = 2;

enum class End : uint8_t { finished = 0, recycled = 1, lost = 2 };

namespace events {
struct Line {                       // notify, the flow thread
    static constexpr const char *point = "speech.line";
    static constexpr uint32_t version = 1;
    uint32_t size, payload_version;
    uint32_t id;                    // the game's line id, unique while the line lives
    uint32_t duration_ms;           // 0 in speech.line; the real length from speech.line.timed on
    uint32_t played_ms;             // ms since speech.line on the core's clock
    uint32_t speaker_index;         // the speaker's index in the speech system's speaker table
    uint8_t category;               // 2 team radio, 0 commentary
    uint8_t retrigger;              // PostLine's third argument
    End reason;                     // speech.line.end only
    uint8_t reserved;
    char speaker[SPEAKER_CHARS];    // "engineer1", "drivers", "commentator1", ...
    char stem[STEM_CHARS];          // "eng_radiocheck_03"; empty when the line has no subtitle
};                                  // 156 bytes

struct Timed : Line {               // the same layout
    static constexpr const char *point = "speech.line.timed";
};

struct Ended : Line {               // the same layout
    static constexpr const char *point = "speech.line.end";
};
}

bool live();                                   // the core fires the three events in this game
bool radio(const events::Line &l);             // category == CATEGORY_RADIO
bool engineer(const events::Line &l);          // speaker starts with "engineer"
bool driver(const events::Line &l);            // speaker is "drivers"
std::string_view speaker(const events::Line &l);
std::string_view stem(const events::Line &l);
std::string subtitle_key(const events::Line &l); // "lng_subtitle_" + stem, or empty
const char *name(End e);                       // "finished", "recycled", "lost"

}
```

Subscribe with `swofty::events::on<E>().after(...)` from `SWOFTY_ON_LOAD`, as for every notify event (see [Events](./events)).

- **`speech.line`** fires on the next frame after the game posts the line. `duration_ms` is 0: the game asks the audio engine for the line's length when it posts it.
- **`speech.line.timed`** fires once, the first frame the line's length is known. In game the engineer's lines take 1200 to 5500 ms. A line that never gets a length never fires it.
- **`speech.line.end`** fires once per line. `reason` is `finished` when the audio engine ends the line, `recycled` when the game reuses the line for the next one before it is seen ending, and `lost` when the line can no longer be read, the session changes, or the line runs 30 s past its length (60 s when it has none).

A line keeps its `id`, speaker and stem in all three events, so an addon matches them by `id`.

## Speakers and categories

`speaker` is the speech system's speaker name. The race engineer speaks as `engineer1` and a driver's radio reply as `drivers`, both with category 2. The commentators speak as `commentator1` and `commentator2_alt1` with category 0. `speaker_index` is the speaker's slot in the speech system and stays the same for a speaker during a session.

The line's text is the `lng_*` key `"lng_subtitle_" + stem`. `swofty::lang::subtitle(stem)` reads it in the game's current language (see [Strings](./strings#text-and-language)), with the subtitle setting on or off:

```cpp
std::optional<std::string> text = swofty::lang::subtitle(sp::stem(line)); // "Radio check, how do you hear me?"
```

Some lines are one part of a longer message, with `[VALUE_n]` placeholders the next line of the same speaker fills.

## Is it there

`live()` is true when the core fires the events: the `events` feature is on and the game build's speech line layout is proven (the KB build and Steam 1.0.145). On another build, on a core without the speech events, or with `SWOFTY_SPEECH_EVENTS=0` in the game's environment, `live()` is false and nothing fires.

```text
[speech] voice line events off (SWOFTY_SPEECH_EVENTS=0)
[speech] voice line events off on EA App 1.0.145: the speech line layout (...) is proven on the Steam KB build, Steam 1.0.145 (26566308) only
```

The core wraps `Game_SpeechSystem_PostLine` at priority -100, below the addons' wraps and the [characters](./characters) voice mute. A line that a higher wrap does not pass on to the game fires nothing.

## Test it on the host

`swofty/fake_speech.hpp` installs a fake core whose hooks table runs your handlers and whose strings table answers `swofty::lang::text`, `subtitle` and `language`.

```cpp
#include <swofty/fake_speech.hpp>
#include <cassert>

int main()
{
    swofty::fake::Speech fake;
    fake.text("lng_subtitle_eng_radiocheck_03", "Radio check, how do you hear me?");
    fake.language("eng");
    fake.install();

    std::string shown;
    swofty::events::on<swofty::speech::events::Line>().after([&](const swofty::speech::events::Line &l, const swofty::events::Context &) {
        shown = swofty::lang::subtitle(swofty::speech::stem(l)).value_or("");
    });

    fake.post(41, "engineer1", "eng_radiocheck_03"); // speech.line
    assert(shown == "Radio check, how do you hear me?");
    fake.timed(41, 2100);                            // speech.line.timed
    fake.end(41);                                    // speech.line.end, reason finished
}
```

- `post(id, speaker, stem, category = 2, speaker_index = 0)`, `timed(id, duration_ms, played_ms = 50)` and `end(id, reason = finished, played_ms = duration)` fire the events.
- `set_live(false)` makes `live()` false; `set_lang_version(1)` answers as a core whose strings table has no `text` and `language`.
- `handlers(point)` counts the handlers attached to an event.
