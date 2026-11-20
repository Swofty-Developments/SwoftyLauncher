#include "core/career/characters_state.hpp"

#include "core/career/career_ui.hpp"
#include "core/guards/skips.hpp"

#include <atomic>
#include <cstring>
#include <mutex>
#include <string>

// Character refs (Game_EntrantsUi_BuildCharacterRef, Game_EntrantsUi_OnCharacterListLoaded). The UI
// keeps one VM node per entrants.characters row under character_list. The GameCharacterListLoaded
// handler clears that node and rebuilds it from the game's character vector (message +0x10; begin
// +0x08, end +0x10, 0x38-byte rows: stat_id byte +0x00, name string object +0x08 with its text at
// +0x10), calling the ref builder once per row. The builder wrap hands a swapped character a
// stand-in record: the replacement row with the character's stat_id, so names, job and the
// message_<name> textures follow the replacement while lookups by id still find the ref. The
// handler wrap keeps the vector; a career load or unload that changes the active group, or
// refresh(), runs the handler again with it so the list is rebuilt for the new group.
//
// Voice (Game_SpeechSystem_PostLine). The speaker name of a line is the speech system's speaker
// record name (*(speech +0x728) +0x60 + line[+0x80] * 0x88, name at +0x18, below the count at
// speech +0x7cc). A muted speaker's line gets its emitter's (line +0x28) vol_speech RTPC set to 0
// through the emitter's setter (vtable +0x60), then posts as usual, so subtitles and the
// end-of-line callbacks still run.

namespace career::characters {

namespace {

namespace mem = swofty::mem;

constexpr size_t ROW = 0x38, MAX_ROWS = 4096, RTPC_SLOT = 0x60;
constexpr uintptr_t SPEAKERS = 0x728, SPEAKER_RECORDS = 0x60, SPEAKER_STRIDE = 0x88, SPEAKER_NAME = 0x18, SPEAKER_COUNT = 0x7cc,
                    LINE_SPEAKER = 0x80, LINE_EMITTER = 0x28;
constexpr char SPEECH_VOLUME[] = "vol_speech";

using SetRtpc = void(void *emitter, const char *name, float value, int a, float interpolation, int b);

std::atomic<const void *> vector_seen{nullptr};
std::atomic<uint32_t> stand_ins, muted_lines;
constinit swofty_core::guards::LogFirst<4> missing_lines;
constinit swofty_core::guards::LogFirst<8> build_lines, shared_lines;

std::string name_of(const uint8_t *row)
{
    uintptr_t s = 0;
    std::string out;
    if (row && mem::read(reinterpret_cast<uintptr_t>(row) + 0x08, s) && s)
        if (uintptr_t text = 0; mem::read(s + 0x10, text))
            mem::c_string(text, out, 128);
    return out;
}

size_t rows(const void *vector, const uint8_t **begin)
{
    uintptr_t b = 0, e = 0;
    if (!vector || !mem::read(reinterpret_cast<uintptr_t>(vector) + 0x08, b) ||
        !mem::read(reinterpret_cast<uintptr_t>(vector) + 0x10, e) || !b || e <= b || (e - b) % ROW || (e - b) / ROW > MAX_ROWS)
        return 0;
    if (begin)
        *begin = reinterpret_cast<const uint8_t *>(b);
    return (e - b) / ROW;
}

const uint8_t *find_row(const std::string &name)
{
    const uint8_t *b = nullptr;
    size_t n = rows(vector_seen.load(), &b);
    for (size_t i = 0; i < n; i++)
        if (name_of(b + i * ROW) == name)
            return b + i * ROW;
    return nullptr;
}

const char *speaker_of(const void *speech, const void *line)
{
    uint64_t index = 0;
    uint32_t count = 0;
    uintptr_t table = 0, record = 0, name = 0;
    auto s = reinterpret_cast<uintptr_t>(speech), l = reinterpret_cast<uintptr_t>(line);
    if (!mem::read(l + LINE_SPEAKER, index) || !mem::read(s + SPEAKER_COUNT, count) || index >= count ||
        !mem::read(s + SPEAKERS, table) || !table || !mem::read(table + SPEAKER_RECORDS + index * SPEAKER_STRIDE, record) ||
        !record || !mem::read(record + SPEAKER_NAME, name) || !mem::readable(name, 1))
        return nullptr;
    return reinterpret_cast<const char *>(name);
}

}

void *build_ref(void *out, const uint8_t *character, void *node)
{
    if (any_swap.load() && character) {
        std::string want = replacement_for(name_of(character));
        if (!want.empty()) {
            if (const uint8_t *rep = find_row(want)) {
                uint8_t stand_in[ROW];
                memcpy(stand_in, rep, ROW);
                stand_in[0] = character[0];
                stand_ins++;
                return orig_build(out, stand_in, node);
            }
            if (missing_lines.note())
                swofty::log("[characters] no '%s' row in the character list; '%s' keeps its own ref", want.c_str(),
                            name_of(character).c_str());
        }
    }
    return orig_build(out, character, node);
}

int64_t list_loaded(void *ctx, const void *message)
{
    uintptr_t vector = 0;
    if (message && mem::read(reinterpret_cast<uintptr_t>(message) + 0x10, vector) && vector)
        vector_seen = reinterpret_cast<const void *>(vector);
    if (build_lines.note()) {
        std::string group;
        {
            std::lock_guard<std::mutex> g(lock);
            group = active;
        }
        swofty::log("[characters] character list built from %p (%zu rows, group '%s')", (void *)vector,
                    rows(reinterpret_cast<const void *>(vector), nullptr), group.c_str());
    }
    return orig_loaded(ctx, message);
}

void post_line(void *speech, void *line, uint8_t retrigger)
{
    if (any_mute.load() && speech && line) {
        const char *speaker = speaker_of(speech, line);
        uintptr_t emitter = 0, vt = 0, fn = 0;
        mem::read(reinterpret_cast<uintptr_t>(line) + LINE_EMITTER, emitter);
        if (emitter && speaker_muted(speaker) && mem::read(emitter, vt) && mem::in_image(vt) && mem::read(vt + RTPC_SLOT, fn) &&
            mem::in_image(fn)) {
            reinterpret_cast<SetRtpc *>(fn)(reinterpret_cast<void *>(emitter), SPEECH_VOLUME, 0.f, 0, 0.f, 0);
            bool fresh;
            {
                std::lock_guard<std::mutex> g(lock);
                fresh = muted_emitters.insert(reinterpret_cast<void *>(emitter)).second;
            }
            uint32_t n = ++muted_lines;
            if (fresh || n <= 8)
                swofty::log("[characters] %s line muted (emitter %p, %u so far)", speaker, (void *)emitter, n);
        } else if (emitter) {
            bool shared;
            {
                std::lock_guard<std::mutex> g(lock);
                shared = muted_emitters.count(reinterpret_cast<void *>(emitter)) != 0;
            }
            if (shared && shared_lines.note())
                swofty::log("[characters] emitter %p, muted earlier, also speaks %s; that line is silent too", (void *)emitter,
                            speaker ? speaker : "(unknown)");
        }
    }
    orig_post(speech, line, retrigger);
}

bool rebuild(const char *why)
{
    const void *vector = vector_seen.load();
    if (!ref.live || !rows(vector, nullptr)) {
        swofty::log("[characters] %s; the character list is not rebuilt (%s)", why,
                    ref.live ? "the game has not built it yet" : "the ref hooks are not installed");
        return false;
    }
    uint8_t message[0x18] = {};
    memcpy(message + 0x10, &vector, sizeof vector);
    auto handler = reinterpret_cast<ListLoaded *>(swofty::api->resolve("Game_EntrantsUi_OnCharacterListLoaded"));
    if (!handler)
        return false;
    uint32_t before = stand_ins.load();
    handler(nullptr, message);
    swofty::log("[characters] %s; character list rebuilt (%u stand-in ref(s))", why, stand_ins.load() - before);
    return true;
}

}
