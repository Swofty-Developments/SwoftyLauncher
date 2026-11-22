#pragma once
#include <swofty/systems/speech.hpp>

#include <cstdint>
#include <cstdio>
#include <cstring>
#include <mutex>
#include <string>
#include <string_view>
#include <vector>

// Voice line tracking for the speech events (std lib speech, swofty/systems/speech.hpp). The PostLine wrap hands each
// posted line to post() from the thread that posted it; the flow tick drains them and reads every tracked line once a
// frame through a reader: the line's id (+0x8c), duration (+0x88, 0 at PostLine, set by the Wwise duration callback a
// few frames later) and state (u32 +0x144: 6 playing, 8 Wwise ended, 9 and 10 the speech system's ends;
// re/db/structs/SpeechLine.h). A line fires speech.line when drained, speech.line.timed the first frame its duration is
// non-zero, and speech.line.end when its state reaches 8 to 10 (finished), its id changes (recycled), or it cannot be
// read, its session changes or it runs past its time (lost). A line is matched by its address and id, never kept past
// its end. Pure: speech.cpp feeds it and fires what it emits.

namespace swofty_core::speech {

namespace sp = swofty::speech;

inline constexpr uintptr_t SPEAKERS = 0x728, SPEAKER_RECORDS = 0x60, SPEAKER_STRIDE = 0x88, SPEAKER_NAME = 0x18,
                           SPEAKER_COUNT = 0x7cc;
inline constexpr uintptr_t LINE_SPEAKER = 0x80, LINE_DURATION = 0x88, LINE_ID = 0x8c, LINE_STEM = 0x138, LINE_CATEGORY = 0x140,
                           LINE_STATE = 0x144;
inline constexpr uint32_t STATE_ENDED_FIRST = 8, STATE_ENDED_LAST = 10;
inline constexpr size_t MAX_LINES = 32;
inline constexpr double UNTIMED_SECONDS = 60.0, PAST_END_SECONDS = 30.0;

inline bool fires(std::string_view point)
{
    return point == sp::events::Line::point || point == sp::events::Timed::point || point == sp::events::Ended::point;
}

struct Posted {
    uintptr_t line = 0;
    uint32_t id = 0;
    uint32_t speaker_index = 0;
    uint8_t category = 0;
    uint8_t retrigger = 0;
    std::string speaker, stem;
    double at = 0;
};

struct Read {
    bool ok = false;
    uint32_t id = 0;
    uint32_t duration_ms = 0;
    uint32_t state = 0;
};

enum class Kind : uint8_t { line, timed, ended };

struct Moment {
    Kind kind;
    sp::events::Line payload;
};

// Reads one posted line through guarded readers: rd(address, value&) -> bool and str(address, std::string&, max) ->
// bool. False when the line's id cannot be read.
template <class Rd, class Str>
bool capture(uintptr_t speech, uintptr_t line, uint8_t retrigger, double now, Rd &&rd, Str &&str, Posted &out)
{
    uint64_t index = ~0ull;
    uint32_t count = 0;
    uintptr_t table = 0, record = 0, name = 0, stem = 0;
    if (!line || !rd(line + LINE_ID, out.id))
        return false;
    out.line = line;
    out.retrigger = retrigger;
    out.at = now;
    rd(line + LINE_CATEGORY, out.category);
    if (speech && rd(line + LINE_SPEAKER, index) && rd(speech + SPEAKER_COUNT, count) && index < count &&
        rd(speech + SPEAKERS, table) && table && rd(table + SPEAKER_RECORDS + index * SPEAKER_STRIDE, record) && record &&
        rd(record + SPEAKER_NAME, name) && name) {
        out.speaker_index = uint32_t(index);
        str(name, out.speaker, sp::SPEAKER_CHARS - 1);
    }
    if (rd(line + LINE_STEM, stem) && stem)
        str(stem, out.stem, sp::STEM_CHARS - 1);
    return true;
}

inline void copy_text(char *to, size_t cap, std::string_view from)
{
    size_t n = from.size() < cap - 1 ? from.size() : cap - 1;
    std::memcpy(to, from.data(), n);
    std::memset(to + n, 0, cap - n);
}

class Tracker {
public:
    void post(Posted p)
    {
        std::lock_guard<std::mutex> g(lock_);
        if (pending_.size() < MAX_LINES * 2)
            pending_.push_back(std::move(p));
        else
            dropped_++;
    }

    size_t dropped() const { return dropped_; }
    size_t tracked() const { return lines_.size(); }

    // Once a frame on the flow thread. serial is the session's serial: a change ends every tracked line as lost.
    template <class Reader, class Emit> void tick(double now, uint64_t serial, Reader &&read, Emit &&emit)
    {
        std::vector<Posted> fresh;
        {
            std::lock_guard<std::mutex> g(lock_);
            fresh.swap(pending_);
        }
        if (serial != serial_) {
            serial_ = serial;
            end_all(now, sp::End::lost, emit);
        }
        for (Posted &p : fresh)
            take(std::move(p), now, emit);
        for (size_t i = 0; i < lines_.size();) {
            Entry &e = lines_[i];
            Read r = read(e.posted.line);
            if (!r.ok) {
                finish(i, now, sp::End::lost, emit);
                continue;
            }
            if (r.id != e.posted.id) {
                finish(i, now, sp::End::recycled, emit);
                continue;
            }
            if (!e.timed && r.duration_ms) {
                e.timed = true;
                e.duration_ms = r.duration_ms;
                emit(Moment{Kind::timed, payload(e, now)});
            }
            if (r.state >= STATE_ENDED_FIRST && r.state <= STATE_ENDED_LAST) {
                finish(i, now, sp::End::finished, emit);
                continue;
            }
            double age = now - e.posted.at;
            if ((!e.timed && age > UNTIMED_SECONDS) || (e.timed && age > e.duration_ms / 1000.0 + PAST_END_SECONDS)) {
                finish(i, now, sp::End::lost, emit);
                continue;
            }
            i++;
        }
    }

    template <class Emit> void end_all(double now, sp::End why, Emit &&emit)
    {
        while (!lines_.empty())
            finish(0, now, why, emit);
    }

private:
    struct Entry {
        Posted posted;
        bool timed = false;
        uint32_t duration_ms = 0;
    };

    template <class Emit> void take(Posted p, double now, Emit &emit)
    {
        for (size_t i = 0; i < lines_.size(); i++) {
            if (lines_[i].posted.line != p.line)
                continue;
            if (lines_[i].posted.id == p.id)
                return;
            finish(i, now, sp::End::recycled, emit);
            break;
        }
        if (lines_.size() >= MAX_LINES)
            finish(0, now, sp::End::lost, emit);
        lines_.push_back(Entry{std::move(p)});
        emit(Moment{Kind::line, payload(lines_.back(), now)});
    }

    template <class Emit> void finish(size_t i, double now, sp::End why, Emit &emit)
    {
        sp::events::Line p = payload(lines_[i], now);
        p.reason = why;
        lines_.erase(lines_.begin() + ptrdiff_t(i));
        emit(Moment{Kind::ended, p});
    }

    static sp::events::Line payload(const Entry &e, double now)
    {
        sp::events::Line p{};
        p.size = sizeof p;
        p.payload_version = sp::events::Line::version;
        p.id = e.posted.id;
        p.duration_ms = e.duration_ms;
        double ms = (now - e.posted.at) * 1000.0;
        p.played_ms = ms > 0 ? uint32_t(ms) : 0;
        p.speaker_index = e.posted.speaker_index;
        p.category = e.posted.category;
        p.retrigger = e.posted.retrigger;
        p.reason = sp::End::finished;
        copy_text(p.speaker, sizeof p.speaker, e.posted.speaker);
        copy_text(p.stem, sizeof p.stem, e.posted.stem);
        return p;
    }

    std::mutex lock_;
    std::vector<Posted> pending_;
    std::vector<Entry> lines_;
    uint64_t serial_ = 0;
    size_t dropped_ = 0;
};

}
