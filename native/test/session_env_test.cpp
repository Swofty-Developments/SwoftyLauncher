// Host-native unit test for the session environment file the loader reads (native/include/session_env.h).
// Built and run by native/test/run_tests.sh session_env.

#include "../include/session_env.h"

#include <cstdio>
#include <cstring>
#include <string>

static int failed = 0;

static void expect(bool ok, const char *what)
{
    std::printf("%s: %s\n", ok ? "ok  " : "FAIL", what);
    if (!ok)
        failed++;
}

static int read(const std::string &text, long long started, SwoftySessionEnv &e, std::string &buf)
{
    buf = text;
    return swofty_session_env_read(buf.data(), started, &e);
}

static const char *value(const SwoftySessionEnv &e, const char *name)
{
    for (int i = 0; i < e.count; i++)
        if (!std::strcmp(e.names[i], name))
            return e.values[i];
    return nullptr;
}

static std::string file(long long written, long long expires, const std::string &env = "\"SWOFTY_T\":\"1\"",
                        const std::string &extra = "")
{
    return "{\"schema\":1,\"request\":\"5f3a9c\",\"written\":" + std::to_string(written) +
           ",\"expires\":" + std::to_string(expires) + extra + ",\"env\":{" + env + "}}";
}

int main()
{
    SwoftySessionEnv e;
    std::string buf;
    const long long T = 1800000000;

    {
        int r = read("{\"schema\": 1, \"request\": \"5f3a9c\", \"written\": 1800000000, \"expires\": 1800001020,\n"
                     " \"env\": {\"SWOFTY_NET_TRACE\": \"1\", \"SWOFTY_GAME_LOG_CHANNELS\": \"default,NETWORK*:info\",\n"
                     " \"SWOFTY_SESSION_TOKEN\": \"a\\\"b\\\\c\\/d\\n\\u00fc\\u2713\\ud83d\\ude00\"}}\n",
                     T + 40, e, buf);
        expect(r == SWOFTY_SESSION_ENV_OK, "a file the CLI wrote holds for a game started 40 s later");
        expect(e.count == 3, "it names three variables");
        expect(e.request && !std::strcmp(e.request, "5f3a9c"), "and the launch request");
        expect(e.written == T && e.expires == T + 1020, "and its window");
        expect(value(e, "SWOFTY_NET_TRACE") && !std::strcmp(value(e, "SWOFTY_NET_TRACE"), "1"), "a plain value reads as written");
        expect(value(e, "SWOFTY_GAME_LOG_CHANNELS") &&
                   !std::strcmp(value(e, "SWOFTY_GAME_LOG_CHANNELS"), "default,NETWORK*:info"),
               "a value with punctuation reads as written");
        expect(value(e, "SWOFTY_SESSION_TOKEN") &&
                   !std::strcmp(value(e, "SWOFTY_SESSION_TOKEN"), "a\"b\\c/d\n\xc3\xbc\xe2\x9c\x93\xf0\x9f\x98\x80"),
               "escapes, \\u escapes and a surrogate pair decode to UTF-8");
    }
    expect(read(file(T, T + 1020), T, e, buf) == SWOFTY_SESSION_ENV_OK, "a game started the second it was written takes it");
    expect(read(file(T, T + 1020), T + 1020, e, buf) == SWOFTY_SESSION_ENV_OK, "and one started the second it expires");
    expect(read(file(T, T + 1020), T - SWOFTY_SESSION_ENV_SKEW, e, buf) == SWOFTY_SESSION_ENV_OK,
           "a game created up to the skew before the write takes it");
    expect(read(file(T, T + 1020), T + 1021, e, buf) == SWOFTY_SESSION_ENV_EXPIRED,
           "a stale file a killed session left is ignored by a later plain start");
    expect(read(file(T, T + 1020), T + 86400, e, buf) == SWOFTY_SESSION_ENV_EXPIRED, "and a day later");
    expect(read(file(T, T + 1020), T - SWOFTY_SESSION_ENV_SKEW - 1, e, buf) == SWOFTY_SESSION_ENV_LATER,
           "a file written for a later start does not arm a game that was already running");
    expect(read(file(T, T + 4000), T + 10, e, buf) == SWOFTY_SESSION_ENV_BAD_WINDOW,
           "a window longer than an hour is not one the launcher writes");
    expect(read(file(T, T), T, e, buf) == SWOFTY_SESSION_ENV_BAD_WINDOW, "nor an empty one");
    expect(read(file(0, 1020), 10, e, buf) == SWOFTY_SESSION_ENV_BAD_WINDOW, "nor one with no write time");
    {
        std::string t = file(T, T + 1020);
        t.replace(t.find("\"schema\":1"), 10, "\"schema\":2");
        expect(read(t, T + 5, e, buf) == SWOFTY_SESSION_ENV_OTHER_SCHEMA, "another schema is ignored");
        t = file(T, T + 1020);
        t.replace(t.find("\"5f3a9c\""), 8, "\"\"");
        expect(read(t, T + 5, e, buf) == SWOFTY_SESSION_ENV_NO_REQUEST, "a file naming no request is ignored");
        t = file(T, T + 1020);
        t.replace(t.find("\"request\":\"5f3a9c\","), 19, "");
        expect(read(t, T + 5, e, buf) == SWOFTY_SESSION_ENV_NO_REQUEST, "and so is one without the key");
    }
    expect(read(file(T, T + 1020, "", ",\"note\":{\"a\":[1,2.5e3,true,null,\"x\"],\"b\":false}"), T + 5, e, buf) ==
                   SWOFTY_SESSION_ENV_OK &&
               e.count == 0,
           "keys the loader does not read are skipped, and an empty env holds");
    const char *broken[] = {
        "",
        "not json",
        "[]",
        "{\"schema\":1",
        "{\"schema\":\"1\",\"request\":\"a\",\"written\":1800000000,\"expires\":1800001020,\"env\":{}}",
        "{\"schema\":1.5,\"request\":\"a\",\"written\":1800000000,\"expires\":1800001020,\"env\":{}}",
        "{\"schema\":1,\"request\":\"a\",\"written\":1800000000,\"expires\":1800001020,\"env\":{\"A\":1}}",
        "{\"schema\":1,\"request\":\"a\",\"written\":1800000000,\"expires\":1800001020,\"env\":{\"\":\"x\"}}",
        "{\"schema\":1,\"request\":\"a\",\"written\":1800000000,\"expires\":1800001020,\"env\":{\"A=B\":\"x\"}}",
        "{\"schema\":1,\"request\":\"a\",\"written\":1800000000,\"expires\":1800001020,\"env\":{\"A\":\"x\\u0000y\"}}",
        "{\"schema\":1,\"request\":\"a\",\"written\":1800000000,\"expires\":1800001020,\"env\":{\"A\":\"\\ud83d\"}}",
        "{\"schema\":1,\"request\":\"a\",\"written\":1800000000,\"expires\":1800001020,\"env\":{\"A\":\"\\q\"}}",
        "{\"schema\":1,\"request\":\"a\",\"written\":1800000000,\"expires\":1800001020,\"env\":{\"A\":\"x\"}} trailing",
        "{\"schema\":1,\"request\":\"a\",\"written\":99999999999999999999,\"expires\":1800001020,\"env\":{}}",
        ("{\"schema\":1,\"request\":\"a\",\"written\":1800000000,\"expires\":1800001020,\"env\":{\"A\":\"x\"},"
         "\"env\":{\"B\":\"y\"}}"),
    };
    for (const char *t : broken) {
        std::string what = std::string("a malformed file is unreadable: ") + t;
        expect(read(t, T + 5, e, buf) == SWOFTY_SESSION_ENV_UNREADABLE, what.c_str());
    }
    {
        std::string t = "{\"note\":";
        for (int i = 0; i < 40; i++)
            t += "[";
        for (int i = 0; i < 40; i++)
            t += "]";
        t += "}";
        expect(read(t, T, e, buf) == SWOFTY_SESSION_ENV_UNREADABLE, "nesting past the depth limit is unreadable");
    }
    {
        std::string many;
        for (int i = 0; i <= SWOFTY_SESSION_ENV_MAX; i++)
            many += (i ? ",\"V" : "\"V") + std::to_string(i) + "\":\"x\"";
        expect(read(file(T, T + 1020, many), T + 5, e, buf) == SWOFTY_SESSION_ENV_UNREADABLE,
               "more variables than the loader holds is unreadable");
    }
    expect(read("\xef\xbb\xbf" + file(T, T + 1020), T + 5, e, buf) == SWOFTY_SESSION_ENV_OK, "a UTF-8 byte order mark is fine");
    for (int code = SWOFTY_SESSION_ENV_OK; code <= SWOFTY_SESSION_ENV_LATER; code++)
        expect(std::strcmp(swofty_session_env_why(code), "unknown") != 0, "every result has a reason");
    std::printf(failed ? "session env test: FAILED\n" : "session env test: ALL PASSED\n");
    return failed ? 1 : 0;
}
