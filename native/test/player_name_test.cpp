#include "player_name_logic.hpp"

#include <cstdio>

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
    if (failed)
        return 1;
    std::printf("all passed\n");
}
