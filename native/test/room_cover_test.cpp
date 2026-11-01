// Host-native unit test for the room launch cover (native/src/core/room_cover_logic.hpp).
// Built and run by native/test/run_events_test.sh.

#include "room_cover_logic.hpp"

#include <cstdio>

namespace rc = runtime::room_cover;

static int failed = 0;

static void expect(bool ok, const char *what)
{
    std::printf("%s: %s\n", ok ? "ok  " : "FAIL", what);
    if (!ok)
        failed++;
}

int main()
{
    rc::Event ev;
    {
        rc::Cover c;
        expect(!c.armed() && !c.drawing(), "an unarmed cover is idle");
        expect(c.frame(1000, rc::Signal::waiting, &ev) == 0 && ev == rc::Event::none, "no room launch draws nothing");
        expect(c.frame(2000, rc::Signal::shown, &ev) == 0 && ev == rc::Event::none, "and ignores the room signal");
        expect(!c.drawing(), "and never starts");
    }
    {
        rc::Cover c;
        c.arm();
        expect(c.armed() && c.drawing() && !c.up(), "an armed cover is drawing before its first frame");
        expect(c.frame(1000, rc::Signal::waiting, &ev) == 1 && ev == rc::Event::went_up, "the first frame puts it up, opaque");
        expect(c.up(), "it is up");
        expect(c.frame(1100, rc::Signal::waiting, &ev) == 1 && ev == rc::Event::none, "it stays up while the room is not shown");
        expect(c.frame(5000, rc::Signal::waiting, &ev) == 1 && ev == rc::Event::none, "any number of frames later");
        expect(c.frame(6000, rc::Signal::shown, &ev) == 1 && ev == rc::Event::none, "shown starts the settle, still opaque");
        expect(c.frame(6000 + rc::SETTLE_MS - 1, rc::Signal::shown, &ev) == 1 && ev == rc::Event::none, "and holds through it");
        float a = c.frame(6000 + rc::SETTLE_MS, rc::Signal::shown, &ev);
        expect(ev == rc::Event::came_off && c.why() == rc::Why::shown && a == 1, "after the settle it comes off with reason shown");
        float mid = c.frame(6000 + rc::SETTLE_MS + rc::FADE_MS / 2, rc::Signal::shown, &ev);
        expect(mid > 0.4f && mid < 0.6f && ev == rc::Event::none, "it fades out over the fade time, half way at half");
        expect(c.frame(6000 + rc::SETTLE_MS + rc::FADE_MS - 1, rc::Signal::shown, &ev) > 0, "still faintly drawn just before the end");
        expect(c.frame(6000 + rc::SETTLE_MS + rc::FADE_MS, rc::Signal::shown, &ev) == 0 && !c.drawing(), "then it is gone");
        expect(c.frame(99999, rc::Signal::waiting, &ev) == 0 && ev == rc::Event::none, "and stays gone");
        c.arm();
        expect(!c.drawing(), "arming again does not bring it back");
    }
    {
        rc::Cover c;
        c.arm();
        c.frame(0, rc::Signal::waiting, &ev);
        expect(c.frame(500, rc::Signal::failed, &ev) == 0 && ev == rc::Event::came_off && c.why() == rc::Why::failed,
               "a failed set-up takes it off at once, with no fade");
        expect(!c.drawing(), "and it is no longer drawing");
    }
    {
        rc::Cover c;
        c.arm();
        c.frame(0, rc::Signal::waiting, &ev);
        c.frame(100, rc::Signal::shown, &ev);
        c.frame(100 + rc::SETTLE_MS / 2, rc::Signal::waiting, &ev);
        expect(c.frame(100 + rc::SETTLE_MS, rc::Signal::waiting, &ev) == 1 && ev == rc::Event::came_off,
               "once shown was seen the settle is not cancelled by a later waiting");
    }
    {
        rc::Cover c;
        c.arm();
        c.frame(10, rc::Signal::waiting, &ev);
        expect(c.frame(10 + rc::CAP_MS - 1, rc::Signal::waiting, &ev) == 1 && ev == rc::Event::none, "the cap does not trip early");
        float a = c.frame(10 + rc::CAP_MS, rc::Signal::waiting, &ev);
        expect(ev == rc::Event::came_off && c.why() == rc::Why::timeout && a == 1, "60 s after the first frame it comes off, reason timeout");
        expect(c.frame(10 + rc::CAP_MS + rc::FADE_MS, rc::Signal::waiting, &ev) == 0 && !c.drawing(), "after a fade");
    }
    {
        rc::Cover c;
        c.arm();
        c.frame(0, rc::Signal::waiting, &ev);
        c.frame(100, rc::Signal::shown, &ev);
        c.frame(100 + rc::SETTLE_MS, rc::Signal::shown, &ev);
        c.frame(100 + rc::SETTLE_MS + 10, rc::Signal::failed, &ev);
        expect(c.why() == rc::Why::shown, "the first reason wins");
    }
    expect(rc::request_is_room("room", ""), "SWOFTY_DIRECT_START=room is a room launch");
    expect(!rc::request_is_room("session", ""), "another target is not");
    expect(rc::request_is_room("", "{\"target\":\"room\",\"role\":\"host\"}"), "direct_start.json with target room is");
    expect(!rc::request_is_room("", "{\"target\":\"session\"}"), "direct_start.json with another target is not");
    expect(rc::request_is_room("@x.json", "{\"target\":\"room\"}"), "an @file value reads the file's target");
    expect(!rc::request_is_room("", ""), "no request is not");
    expect(!rc::request_is_room("", "not json"), "an unparsable file is not");
    std::printf(failed ? "room cover test: FAILED\n" : "room cover test: ALL PASSED\n");
    return failed ? 1 : 0;
}
