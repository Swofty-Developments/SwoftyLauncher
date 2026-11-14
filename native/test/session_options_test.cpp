#include "session_options_logic.hpp"

#include <cstdio>
#include <cstdlib>

namespace so = runtime::session_options;

#define CHECK(c)                                                                  \
    do {                                                                          \
        if (!(c)) {                                                               \
            std::fprintf(stderr, "%s:%d: CHECK failed: %s\n", __FILE__, __LINE__, #c); \
            std::exit(1);                                                         \
        }                                                                         \
    } while (0)

static std::string file(const char *expires, const char *env)
{
    return std::string("{\"version\": 1, \"id\": \"ab12\", \"expires\": ") + expires + ", \"env\": " + env + "}";
}

int main()
{
    const int64_t now = 1794658984;
    so::Options o;

    std::string armed = file("1794659584", "{\"SWOFTY_NETPLAY\": \"armed;room\", \"SWOFTY_NET_TRACE\": \"1\", "
                                           "\"SWOFTY_GAME_LOG_CHANNELS\": \"default,NETWORK*:info,SSL*:info\"}");
    CHECK(so::parse(armed, now, o) == so::Verdict::ok);
    CHECK(o.id == "ab12" && o.expires == 1794659584);
    CHECK(o.env.size() == 3);
    CHECK(o.env[0].first == "SWOFTY_NETPLAY" && o.env[0].second == "armed;room");
    CHECK(o.env[1].first == "SWOFTY_NET_TRACE" && o.env[1].second == "1");
    CHECK(o.env[2].first == "SWOFTY_GAME_LOG_CHANNELS");

    // At the expiry second the file still counts; one second later it does not, and nothing is taken from it.
    CHECK(so::parse(armed, 1794659584, o) == so::Verdict::ok);
    CHECK(so::parse(armed, 1794659585, o) == so::Verdict::expired && o.env.empty());
    CHECK(so::parse(armed, now + 86400, o) == so::Verdict::expired && o.env.empty());

    // A file with no expiry, or one further ahead than any launcher writes, arms nothing.
    CHECK(so::parse("{\"version\": 1, \"env\": {\"SWOFTY_NETPLAY\": \"armed\"}}", now, o) == so::Verdict::no_expiry);
    CHECK(so::parse(file("0", "{\"SWOFTY_NETPLAY\": \"armed\"}"), now, o) == so::Verdict::no_expiry);
    CHECK(so::parse(file("\"1794659584\"", "{\"SWOFTY_NETPLAY\": \"armed\"}"), now, o) == so::Verdict::no_expiry);
    CHECK(so::parse(file("1794660785", "{\"SWOFTY_NETPLAY\": \"armed\"}"), now, o) == so::Verdict::too_far);
    CHECK(so::parse(file("1794660784", "{\"SWOFTY_NETPLAY\": \"armed\"}"), now, o) == so::Verdict::ok);
    CHECK(o.env.size() == 1);

    CHECK(so::parse("", now, o) == so::Verdict::unusable);
    CHECK(so::parse("[]", now, o) == so::Verdict::unusable);
    CHECK(so::parse("{\"version\": 1, \"expires\": 1794659584", now, o) == so::Verdict::unusable);
    CHECK(so::parse("{\"version\": 2, \"expires\": 1794659584, \"env\": {\"SWOFTY_NETPLAY\": \"armed\"}}", now, o) ==
          so::Verdict::version);
    CHECK(so::parse("{\"expires\": 1794659584, \"env\": {\"SWOFTY_NETPLAY\": \"armed\"}}", now, o) == so::Verdict::version);

    // Only the listed variables are taken, and only with the arming variable among them.
    CHECK(so::parse(file("1794659584", "{\"SWOFTY_NETPLAY\": \"armed\", \"PATH\": \"C:\\\\x\", \"SWOFTY_POSTFX\": \"0\"}"), now,
                    o) == so::Verdict::ok);
    CHECK(o.env.size() == 1 && o.env[0].first == "SWOFTY_NETPLAY");
    CHECK(so::parse(file("1794659584", "{\"SWOFTY_NET_TRACE\": \"1\"}"), now, o) == so::Verdict::not_armed && o.env.empty());
    CHECK(so::parse(file("1794659584", "{}"), now, o) == so::Verdict::not_armed);
    CHECK(so::parse(file("1794659584", "[]"), now, o) == so::Verdict::not_armed);
    CHECK(so::parse("{\"version\": 1, \"expires\": 1794659584}", now, o) == so::Verdict::not_armed);

    CHECK(so::parse(file("1794659584", "{\"SWOFTY_NETPLAY\": 1}"), now, o) == so::Verdict::bad_value);
    CHECK(so::parse(file("1794659584", "{\"SWOFTY_NETPLAY\": \"\"}"), now, o) == so::Verdict::bad_value);
    CHECK(so::parse(file("1794659584", "{\"SWOFTY_NETPLAY\": \"armed\\nX=1\"}"), now, o) == so::Verdict::bad_value);
    CHECK(so::parse(file("1794659584", ("{\"SWOFTY_NETPLAY\": \"" + std::string(1001, 'a') + "\"}").c_str()), now, o) ==
          so::Verdict::bad_value);
    CHECK(so::parse(file("1794659584", ("{\"SWOFTY_NETPLAY\": \"" + std::string(1000, 'a') + "\"}").c_str()), now, o) ==
          so::Verdict::ok);

    // A game the launcher started itself has the variable already: the file is not read for it.
    CHECK(so::from_file(false) && !so::from_file(true));

    CHECK(so::room_start("{\"target\": \"room\", \"role\": \"host\"}"));
    CHECK(!so::room_start("{\"target\": \"session\"}") && !so::room_start("") && !so::room_start("{"));

    for (auto v : {so::Verdict::ok, so::Verdict::unusable, so::Verdict::version, so::Verdict::no_expiry, so::Verdict::expired,
                   so::Verdict::too_far, so::Verdict::bad_value, so::Verdict::not_armed})
        CHECK(so::why(v)[0] && so::why(v)[0] != '?');

    std::puts("session options: ok");
    return 0;
}
