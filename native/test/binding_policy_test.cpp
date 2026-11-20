// Host-native unit test for the binding set's drop policy (native/src/core/runtime/loader/binding_policy.hpp): a
// deprecated binding, a dropped one, the load line, the one [bind] line per id, and malformed entries.
// Built and run by native/test/run_tests.sh binding_policy.

#include "core/runtime/loader/binding_policy.hpp"

#include <cstdio>
#include <string>

static int g_fail;

#define CHECK(c)                                                                                                                 \
    do {                                                                                                                         \
        if (!(c)) {                                                                                                              \
            std::printf("FAIL %s:%d %s\n", __FILE__, __LINE__, #c);                                                              \
            g_fail++;                                                                                                            \
        }                                                                                                                        \
    } while (0)

using runtime::loader::BindingPolicy;

static BindingPolicy read(const std::string &text)
{
    json::Value doc;
    CHECK(json::parse(text, doc));
    return BindingPolicy::read(doc);
}

static bool has(const std::vector<std::string> &lines, const std::string &line)
{
    for (auto &l : lines)
        if (l == line)
            return true;
    return false;
}

static void deprecated()
{
    BindingPolicy p = read(R"({"bindings": [
        {"id": "Game_Old", "kind": "func", "deprecated": {"since": "1.10.0", "reason": "split in two by the 1.0.145 patch",
         "replacement": "Game_New"}},
        {"id": "g_Kept", "kind": "data", "deprecated": {"since": "1.9.60", "reason": "read the manager's copy"}},
        {"id": "Game_New", "kind": "func"}]})");
    CHECK(p.deprecated_count() == 2 && p.dropped_count() == 0 && p.malformed().empty());
    CHECK(!p.dropped("Game_Old"));
    CHECK(p.use("Game_Old") == "[bind] id=Game_Old deprecated since=1.10.0 replacement=Game_New reason=\"split in two by "
                               "the 1.0.145 patch\"");
    CHECK(p.use("Game_Old").empty());
    CHECK(p.use("g_Kept") == "[bind] id=g_Kept deprecated since=1.9.60 replacement=- reason=\"read the manager's copy\"");
    CHECK(p.use("Game_New").empty());
    CHECK(p.load_line("bindings.json") == "[core] bindings.json: 2 binding(s) deprecated, 0 dropped");
}

static void dropped()
{
    BindingPolicy p = read(R"({"bindings": [{"id": "Game_New", "kind": "func"}],
        "dropped": [{"id": "Game_Gone", "since": "1.10.0", "reason": "the game no longer has it", "replacement": "Game_New"},
                    {"id": "k_Gone", "since": "1.10.0", "reason": "folded into the layout"}]})");
    CHECK(p.deprecated_count() == 0 && p.dropped_count() == 2 && p.malformed().empty());
    CHECK(p.dropped("Game_Gone") && p.dropped("k_Gone") && !p.dropped("Game_New") && !p.dropped("Game_Unknown"));
    CHECK(p.use("Game_Gone") ==
          "[bind] id=Game_Gone dropped since=1.10.0 replacement=Game_New reason=\"the game no longer has it\"");
    CHECK(p.use("Game_Gone").empty());
    CHECK(p.use("k_Gone") == "[bind] id=k_Gone dropped since=1.10.0 replacement=- reason=\"folded into the layout\"");
    CHECK(p.use("Game_Unknown").empty());
    CHECK(p.load_line("bindings-steam.json") == "[core] bindings-steam.json: 0 binding(s) deprecated, 2 dropped");
}

static void neither()
{
    BindingPolicy p = read(R"({"bindings": [{"id": "Game_New", "kind": "func"}]})");
    CHECK(p.load_line("bindings.json").empty() && p.malformed().empty() && p.use("Game_New").empty());
    BindingPolicy none = read("{}");
    CHECK(none.load_line("bindings.json").empty() && none.malformed().empty());
}

static void reasons()
{
    CHECK(BindingPolicy::reason_fault("plain words, digits 1.0.145 and (marks)").empty());
    CHECK(BindingPolicy::reason_fault(std::string(300, 'x')).empty());
    CHECK(BindingPolicy::reason_fault(std::string(301, 'x')) == "is longer than 300 characters");
    CHECK(BindingPolicy::reason_fault("") == "is empty");
    CHECK(BindingPolicy::reason_fault("say \"no\"") == "has a '\"' or '\\'");
    CHECK(BindingPolicy::reason_fault("a\\b") == "has a '\"' or '\\'");
    CHECK(BindingPolicy::reason_fault("tab\there") == "has a character outside printable ASCII");
    CHECK(BindingPolicy::reason_fault("caf\xc3\xa9") == "has a character outside printable ASCII");
}

static void malformed()
{
    std::string text = R"({"bindings": [
        {"id": "Game_Quote", "deprecated": {"since": "1.10.0", "reason": "a \"quoted\" word"}},
        {"id": "Game_Long", "deprecated": {"since": "1.10.0", "reason": "LONG"}},
        {"id": "Game_NoSince", "deprecated": {"reason": "no version"}},
        {"id": "Game_Number", "deprecated": 7},
        {"id": "Game_Spaced", "deprecated": {"since": "1.10 rc", "reason": "a spaced version"}},
        {"id": "Game_BadReplacement", "deprecated": {"since": "1.10.0", "reason": "x", "replacement": 3}},
        {"id": "Game_Listed"}],
        "dropped": [7, {"since": "1.10.0", "reason": "no id"}, {"id": "Game_Listed", "since": "1.10.0", "reason": "x"},
                    {"id": "Game_Twice", "since": "1.10.0", "reason": "first"},
                    {"id": "Game_Twice", "since": "1.10.0", "reason": "second"},
                    {"id": "Game_NoReason", "since": "1.10.0"}]})";
    text.replace(text.find("LONG"), 4, std::string(301, 'r'));
    BindingPolicy p = read(text);
    const auto &m = p.malformed();
    CHECK(m.size() == 11);
    CHECK(has(m, "Game_Quote: malformed deprecated entry (reason has a '\"' or '\\'); it resolves with no notice"));
    CHECK(has(m, "Game_Long: malformed deprecated entry (reason is longer than 300 characters); it resolves with no "
                 "notice"));
    CHECK(has(m, "Game_NoSince: malformed deprecated entry (since is missing or not a printable token); it resolves "
                 "with no notice"));
    CHECK(has(m, "Game_Number: malformed deprecated entry (it is not an object); it resolves with no notice"));
    CHECK(has(m, "Game_Spaced: malformed deprecated entry (since is missing or not a printable token); it resolves "
                 "with no notice"));
    CHECK(has(m, "Game_BadReplacement: malformed deprecated entry (replacement is not a printable token); it resolves "
                 "with no notice"));
    CHECK(has(m, "dropped entry 0: malformed (no id that is a printable token); left out"));
    CHECK(has(m, "dropped entry 1: malformed (no id that is a printable token); left out"));
    CHECK(has(m, "dropped entry 2 (Game_Listed): malformed (the set still lists it); left out"));
    CHECK(has(m, "dropped entry 4 (Game_Twice): malformed (an earlier entry drops it); left out"));
    CHECK(has(m, "dropped entry 5 (Game_NoReason): malformed (reason is missing); left out"));
    CHECK(p.deprecated_count() == 0 && p.dropped_count() == 1 && p.dropped("Game_Twice") && !p.dropped("Game_Listed"));
    CHECK(p.use("Game_Quote").empty() && p.use("Game_Listed").empty());
    CHECK(p.use("Game_Twice") == "[bind] id=Game_Twice dropped since=1.10.0 replacement=- reason=\"first\"");

    BindingPolicy odd = read(R"({"bindings": [{"id": "bad id", "deprecated": {"since": "1", "reason": "x"}}],
                                 "dropped": {"id": "Game_Obj"}})");
    CHECK(odd.malformed().size() == 2);
    CHECK(has(odd.malformed(),
              "a binding: malformed deprecated entry (its id is not a printable token); it resolves with no notice"));
    CHECK(has(odd.malformed(), "dropped is not an array; no binding is dropped"));
    CHECK(odd.load_line("bindings.json").empty());
}

int main()
{
    deprecated();
    dropped();
    neither();
    reasons();
    malformed();
    if (g_fail) {
        std::printf("%d check(s) failed\n", g_fail);
        return 1;
    }
    std::printf("binding_policy: all checks passed\n");
    return 0;
}
