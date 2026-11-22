// Host unit test for the speech events (std lib speech): the tracker that turns posted voice lines and their per-frame
// reads into speech.line, speech.line.timed and speech.line.end (core/audio/speech_logic.hpp), the line capture against
// a fake speech system, the SDK helpers in swofty/systems/speech.hpp, and swofty/fake_speech.hpp with swofty/lang.hpp's
// text(), subtitle() and language().
#include "core/audio/speech_logic.hpp"

#include <swofty/fake_speech.hpp>
#include <swofty/lang.hpp>
#include <swofty/systems/speech.hpp>

#include <cstdio>
#include <cstring>
#include <map>
#include <string>
#include <vector>

namespace sl = swofty_core::speech;
namespace sp = swofty::speech;
namespace lg = swofty::lang;

static int fails;

static void expect(bool ok, const std::string &what)
{
    printf("%s: %s\n", ok ? "ok  " : "FAIL", what.c_str());
    fails += !ok;
}

struct Lines {
    std::map<uintptr_t, sl::Read> at;
    sl::Read operator()(uintptr_t line) const
    {
        auto it = at.find(line);
        return it == at.end() ? sl::Read{} : it->second;
    }
    void set(uintptr_t line, uint32_t id, uint32_t duration = 0, uint8_t state = 6) { at[line] = {true, id, duration, state}; }
};

struct Sink {
    std::vector<sl::Moment> got;
    void operator()(const sl::Moment &m) { got.push_back(m); }
    size_t count(sl::Kind k) const
    {
        size_t n = 0;
        for (auto &m : got)
            n += m.kind == k;
        return n;
    }
};

static sl::Posted posted(uintptr_t line, uint32_t id, const char *speaker, const char *stem, double at)
{
    sl::Posted p;
    p.line = line;
    p.id = id;
    p.category = sp::CATEGORY_RADIO;
    p.speaker = speaker;
    p.stem = stem;
    p.at = at;
    return p;
}

static void test_line_timed_end()
{
    sl::Tracker t;
    Lines lines;
    Sink sink;
    auto emit = [&](const sl::Moment &m) { sink(m); };
    lines.set(0x1000, 41);
    t.post(posted(0x1000, 41, "engineer1", "eng_radiocheck_02", 10.0));
    t.tick(10.0, 1, lines, emit);
    expect(sink.got.size() == 1 && sink.got[0].kind == sl::Kind::line, "a posted line fires speech.line on the next tick");
    expect(sink.got[0].payload.id == 41 && !strcmp(sink.got[0].payload.speaker, "engineer1") &&
               !strcmp(sink.got[0].payload.stem, "eng_radiocheck_02") && sink.got[0].payload.duration_ms == 0,
           "speech.line carries the id, speaker and stem, with no duration yet");
    t.tick(10.1, 1, lines, emit);
    expect(sink.got.size() == 1, "nothing fires while the duration is 0 and the line plays");
    lines.set(0x1000, 41, 2100);
    t.tick(10.2, 1, lines, emit);
    expect(sink.count(sl::Kind::timed) == 1 && sink.got.back().payload.duration_ms == 2100 &&
               sink.got.back().payload.played_ms >= 199 && sink.got.back().payload.played_ms <= 201,
           "speech.line.timed fires once with the real duration and the time since speech.line");
    t.tick(10.3, 1, lines, emit);
    expect(sink.count(sl::Kind::timed) == 1, "speech.line.timed does not fire twice");
    lines.set(0x1000, 41, 2100, 8);
    t.tick(12.4, 1, lines, emit);
    expect(sink.count(sl::Kind::ended) == 1 && sink.got.back().payload.reason == sp::End::finished &&
               sink.got.back().payload.duration_ms == 2100,
           "state 8 ends the line as finished with its duration");
    expect(t.tracked() == 0, "an ended line is forgotten");
}

static void test_recycled_and_lost()
{
    sl::Tracker t;
    Lines lines;
    Sink sink;
    auto emit = [&](const sl::Moment &m) { sink(m); };
    lines.set(0x2000, 5);
    t.post(posted(0x2000, 5, "drivers", "ver_r_check_bhr_23_01", 1.0));
    t.tick(1.0, 7, lines, emit);
    lines.set(0x2000, 6);
    t.tick(1.1, 7, lines, emit);
    expect(sink.count(sl::Kind::ended) == 1 && sink.got.back().payload.reason == sp::End::recycled,
           "a line whose id changes ends as recycled");

    lines.set(0x3000, 9);
    t.post(posted(0x3000, 9, "engineer1", "a", 2.0));
    t.tick(2.0, 7, lines, emit);
    lines.at.erase(0x3000);
    t.tick(2.1, 7, lines, emit);
    expect(sink.got.back().kind == sl::Kind::ended && sink.got.back().payload.reason == sp::End::lost,
           "a line that cannot be read ends as lost");

    lines.set(0x4000, 11);
    t.post(posted(0x4000, 11, "engineer1", "b", 3.0));
    t.tick(3.0, 7, lines, emit);
    t.tick(3.1, 8, lines, emit);
    expect(sink.got.back().kind == sl::Kind::ended && sink.got.back().payload.id == 11 &&
               sink.got.back().payload.reason == sp::End::lost,
           "a session change ends every tracked line as lost");

    lines.set(0x5000, 12);
    t.post(posted(0x5000, 12, "engineer1", "c", 4.0));
    t.tick(4.0, 8, lines, emit);
    t.tick(4.0 + sl::UNTIMED_SECONDS + 1, 8, lines, emit);
    expect(sink.got.back().kind == sl::Kind::ended && sink.got.back().payload.id == 12 &&
               sink.got.back().payload.reason == sp::End::lost,
           "a line that never gets a duration ends as lost after its time");
}

static void test_reposts()
{
    sl::Tracker t;
    Lines lines;
    Sink sink;
    auto emit = [&](const sl::Moment &m) { sink(m); };
    lines.set(0x6000, 20);
    t.post(posted(0x6000, 20, "engineer1", "x", 1.0));
    t.post(posted(0x6000, 20, "engineer1", "x", 1.0));
    t.tick(1.0, 1, lines, emit);
    expect(sink.count(sl::Kind::line) == 1, "a second post of the same line and id fires nothing");
    lines.set(0x6000, 21);
    t.post(posted(0x6000, 21, "engineer1", "y", 1.5));
    t.tick(1.5, 1, lines, emit);
    expect(sink.count(sl::Kind::ended) == 1 && sink.got[1].payload.id == 20 && sink.got[1].payload.reason == sp::End::recycled &&
               sink.got[2].kind == sl::Kind::line && sink.got[2].payload.id == 21,
           "a new id posted on the same line ends the old one as recycled, then fires the new one");

    lines.set(0x7000, 30, 900, 9);
    t.post(posted(0x7000, 30, "engineer1", "z", 2.0));
    t.tick(2.0, 1, lines, emit);
    expect(sink.got.size() >= 3 && sink.got[sink.got.size() - 2].kind == sl::Kind::timed &&
               sink.got.back().kind == sl::Kind::ended && sink.got.back().payload.duration_ms == 900,
           "a line that is already timed and ended fires line, timed and end in one tick");

    sl::Tracker cap;
    Sink capped;
    auto emit2 = [&](const sl::Moment &m) { capped(m); };
    for (uint32_t i = 0; i <= sl::MAX_LINES; i++) {
        lines.set(0x10000 + i * 0x200, 100 + i);
        cap.post(posted(0x10000 + i * 0x200, 100 + i, "engineer1", "s", 5.0));
    }
    cap.tick(5.0, 1, lines, emit2);
    expect(cap.tracked() == sl::MAX_LINES && capped.count(sl::Kind::ended) == 1 && capped.got[sl::MAX_LINES].payload.id == 100,
           "past MAX_LINES the oldest tracked line ends as lost");
}

static void test_capture()
{
    std::map<uintptr_t, std::vector<uint8_t>> mem;
    auto put = [&](uintptr_t at, const void *v, size_t n) {
        auto &b = mem[at];
        b.assign(static_cast<const uint8_t *>(v), static_cast<const uint8_t *>(v) + n);
    };
    auto rd = [&](uintptr_t at, auto &v) {
        auto it = mem.find(at);
        if (it == mem.end() || it->second.size() < sizeof v)
            return false;
        std::memcpy(&v, it->second.data(), sizeof v);
        return true;
    };
    auto str = [&](uintptr_t at, std::string &out, size_t) {
        auto it = mem.find(at);
        if (it == mem.end())
            return false;
        out.assign(reinterpret_cast<const char *>(it->second.data()));
        return true;
    };
    const uintptr_t speech = 0x100000, line = 0x200000, table = 0x300000, record = 0x400000, name = 0x500000, stem = 0x600000;
    uint32_t id = 77, count = 3;
    uint64_t index = 1;
    uint8_t category = 2;
    put(line + sl::LINE_ID, &id, 4);
    put(line + sl::LINE_CATEGORY, &category, 1);
    put(line + sl::LINE_SPEAKER, &index, 8);
    put(line + sl::LINE_STEM, &stem, 8);
    put(speech + sl::SPEAKER_COUNT, &count, 4);
    put(speech + sl::SPEAKERS, &table, 8);
    put(table + sl::SPEAKER_RECORDS + 1 * sl::SPEAKER_STRIDE, &record, 8);
    put(record + sl::SPEAKER_NAME, &name, 8);
    put(name, "engineer1", 10);
    put(stem, "eng_radiocheck_02", 18);
    sl::Posted p;
    expect(sl::capture(speech, line, 1, 3.0, rd, str, p) && p.id == 77 && p.category == 2 && p.speaker == "engineer1" &&
               p.speaker_index == 1 && p.stem == "eng_radiocheck_02" && p.retrigger == 1 && p.at == 3.0,
           "capture reads the id, category, speaker record name and subtitle stem");
    index = 3;
    put(line + sl::LINE_SPEAKER, &index, 8);
    sl::Posted q;
    expect(sl::capture(speech, line, 0, 3.0, rd, str, q) && q.speaker.empty() && q.stem == "eng_radiocheck_02",
           "a speaker index at or past the count leaves the speaker empty");
    sl::Posted r;
    expect(!sl::capture(speech, 0x999, 0, 3.0, rd, str, r), "a line whose id cannot be read is not captured");
}

static void test_sdk_helpers()
{
    sp::events::Line l{};
    std::snprintf(l.speaker, sizeof l.speaker, "%s", "engineer1");
    std::snprintf(l.stem, sizeof l.stem, "%s", "eng_radiocheck_02");
    l.category = 2;
    expect(sp::radio(l) && sp::engineer(l) && !sp::driver(l), "an engineer1 radio line is radio and engineer");
    expect(sp::subtitle_key(l) == "lng_subtitle_eng_radiocheck_02", "subtitle_key prefixes lng_subtitle_");
    std::snprintf(l.speaker, sizeof l.speaker, "%s", "drivers");
    l.stem[0] = 0;
    expect(sp::driver(l) && !sp::engineer(l) && sp::subtitle_key(l).empty(), "a drivers line without a stem has no key");
    expect(sizeof(sp::events::Timed) == sizeof(sp::events::Line) && sizeof(sp::events::Ended) == sizeof(sp::events::Line),
           "the three payloads share one layout");
    expect(sizeof(sp::events::Line) == 156 && offsetof(sp::events::Line, speaker) == 28 && offsetof(sp::events::Line, stem) == 60,
           "the payload layout is 156 bytes with speaker at 28 and stem at 60");
    expect(!strcmp(sp::name(sp::End::recycled), "recycled"), "End names");
}

static void test_fake_and_lang()
{
    expect(!lg::text("lng_x") && !lg::language() && !sp::live(), "without a core text, language and live answer nothing");
    swofty::fake::Speech fake;
    fake.text("lng_subtitle_eng_radiocheck_02", "Radio check.");
    fake.text("lng_long", std::string(600, 'a'));
    fake.language("fre");
    fake.install();
    expect(sp::live(), "the fake reports the speech events live");
    expect(lg::version() == 2, "lang::version reports a version 2 table");
    expect(lg::text("lng_subtitle_eng_radiocheck_02") == std::optional<std::string>("Radio check."), "lang::text reads the key");
    expect(lg::subtitle("eng_radiocheck_02") == std::optional<std::string>("Radio check."), "lang::subtitle builds the key");
    expect(lg::text("lng_long") && lg::text("lng_long")->size() == 600, "a text past the first buffer is read whole");
    expect(!lg::text("lng_missing"), "a missing key gives nothing");
    expect(lg::language() == std::optional<std::string>("fre"), "lang::language reads the current code");

    std::vector<std::string> seen;
    namespace ev = swofty::events;
    ev::on<sp::events::Line>().after([&](const sp::events::Line &l, const ev::Context &) {
        seen.push_back("line " + std::string(sp::speaker(l)) + " " + lg::subtitle(sp::stem(l)).value_or("?"));
    });
    ev::on<sp::events::Timed>().after(
        [&](const sp::events::Timed &l, const ev::Context &) { seen.push_back("timed " + std::to_string(l.duration_ms)); });
    ev::on<sp::events::Ended>().after(
        [&](const sp::events::Ended &l, const ev::Context &) { seen.push_back(std::string("end ") + sp::name(l.reason)); });
    expect(fake.handlers(sp::events::Line::point) == 1, "the subscription reached the fake's hooks table");
    fake.post(7, "engineer1", "eng_radiocheck_02");
    fake.timed(7, 2100);
    fake.end(7);
    expect(seen.size() == 3 && seen[0] == "line engineer1 Radio check." && seen[1] == "timed 2100" && seen[2] == "end finished",
           "post, timed and end reach the handlers in order");

    fake.set_lang_version(1);
    expect(lg::version() == 1 && !lg::text("lng_subtitle_eng_radiocheck_02") && !lg::language(),
           "a SwoftyLang version 1 core answers unavailable");
    fake.set_lang_version(2);
    fake.set_live(false);
    expect(!sp::live(), "a core without the speech layout reports the events unavailable");
    fake.uninstall();
    expect(!lg::text("lng_subtitle_eng_radiocheck_02"), "the fake restores the previous api");
}

int main()
{
    test_line_timed_end();
    test_recycled_and_lost();
    test_reposts();
    test_capture();
    test_sdk_helpers();
    test_fake_and_lang();
    printf("%s (%d failure(s))\n", fails ? "FAILED" : "ALL PASSED", fails);
    return fails != 0;
}
