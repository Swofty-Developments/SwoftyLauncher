#include "player_name_logic.hpp"

#include <cstdio>
#include <cstring>

namespace pn = runtime::player_name;

static int failed = 0;
static void expect(bool ok, const char *what)
{
    std::printf("%s: %s\n", ok ? "ok  " : "FAIL", what);
    if (!ok)
        failed++;
}

int main()
{
    std::string out;
    expect(pn::sanitize("Swofty", out) && out == "Swofty", "a plain name");
    expect(pn::sanitize("  Swofty   Racing \n", out) && out == "Swofty Racing", "whitespace is trimmed and collapsed");
    expect(pn::sanitize("Jos\xc3\xa9 \xc3\x96zil", out) && out == "Jos\xc3\xa9 \xc3\x96zil", "Latin-1 letters are kept");
    expect(pn::sanitize("A very long discord display name indeed", out) && out == "A very long discord disp", "cut at 24 characters");
    expect(pn::sanitize("\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9\xc3\xa9",
                       out) &&
                   out.size() == 48,
               "the cut never splits a multibyte character");
    expect(!pn::sanitize("", out) && !pn::sanitize("   ", out), "empty names are not used");
    expect(!pn::sanitize("Swofty \xf0\x9f\x8f\x8e", out) && out.empty(), "an emoji falls back to the stock name");
    expect(!pn::sanitize("\xe6\x97\xa5\xe6\x9c\xac", out), "characters outside the font fall back");
    expect(!pn::sanitize("bad\xff", out) && !pn::sanitize("trunc\xc3", out), "invalid UTF-8 falls back");
    expect(!pn::sanitize("a\x01" "b", out), "control characters fall back");
    expect(!pn::sanitize("5 \xc3\x97 3", out), "the multiplication sign is not a letter the font is assumed to draw");
    {
        std::string out;
        expect(pn::display_source("Swofty", "Discord", out) && out == "Swofty", "the environment text wins over the account name");
        expect(pn::display_source(nullptr, "Discord", out) && out == "Discord", "no environment text uses the account name");
        expect(pn::display_source("", "Discord", out) && out == "Discord", "an empty environment text uses the account name");
        expect(!pn::display_source("0", "Discord", out), "0 turns the feature off");
        expect(!pn::display_source(nullptr, "", out), "nothing to show");

        runtime::presence_id::Id me;
        me.type = 1;
        me.value[0] = 76561198803768583ull;
        uint8_t id[0x50] = {};
        id[0] = 1;
        uint64_t mask = 0x1234567890abcdefull, v = me.value[0] ^ mask;
        std::memcpy(id + 8, &v, 8);
        std::memcpy(id + 0x10, &mask, 8);
        expect(pn::is_local_id(id, me), "the presence's own id is local");
        uint64_t other = me.value[0] + 1 ^ mask;
        std::memcpy(id + 8, &other, 8);
        expect(!pn::is_local_id(id, me), "another user's id is not local");
        std::memcpy(id + 8, &v, 8);
        id[0x39] = 1;
        expect(!pn::is_local_id(id, me), "a different flag is another id");
        id[0x39] = 0;
        id[0] = 3;
        expect(!pn::is_local_id(id, me), "another type is another id");
        id[0] = 1;
        id[0x4c] = 1;
        expect(!pn::is_local_id(id, me), "only kind 0 keys are compared");
        id[0x4c] = 0;
        runtime::presence_id::Id none;
        expect(!pn::is_local_id(id, none), "an unknown presence matches nothing");
        runtime::presence_id::Id zero = me;
        zero.value[0] = 0;
        uint8_t blank[0x50] = {};
        blank[0] = 1;
        expect(!pn::is_local_id(blank, zero), "a zero id is never local");
        expect(pn::root_allowed(pn::djb2("network_drivers")) && pn::root_allowed(pn::djb2("players")) && !pn::root_allowed(pn::djb2("hud")),
               "only the driver roots are rewritten");
        expect(pn::leaf_name(0xd23bb5d8) && !strcmp(pn::leaf_name(0xd23bb5d8), "full_name"), "leaf hash to name");
        expect(pn::leaf_name(pn::djb2("gamertag")) && !pn::leaf_name(pn::djb2("team")), "only name leaves");
    }
    if (failed)
        return 1;
    std::printf("all passed\n");
}
