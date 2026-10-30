// SPDX-License-Identifier: AGPL-3.0-or-later
#include <cstdio>
#include <fstream>
#include <map>
#include <sstream>
#include <string>

#include "json_strings.hpp"

static int failures;

static void check(const char *name, bool ok)
{
    std::printf("%s %s\n", ok ? "ok  " : "FAIL", name);
    failures += !ok;
}

static const std::string *text(const std::vector<f1990::AgentStrings> &langs, const char *code, const char *key)
{
    for (auto &l : langs)
        if (l.code == code)
            for (auto &[k, t] : l.texts)
                if (k == key)
                    return &t;
    return nullptr;
}

int main(int argc, char **argv)
{
    std::vector<f1990::AgentStrings> langs;
    size_t at = 0;

    check("parses an empty object", f1990::parse_agent_strings(" {} \n", langs) && langs.empty());
    check("parses languages and escapes",
          f1990::parse_agent_strings(R"({"eng": {"a": "x\r\ny \"q\" \\ \/ \t", "b": ""}, "jap": {}, "zhs": {"c": "弗😀"}})",
                                     langs) &&
              langs.size() == 3 && langs[0].texts.size() == 2 && *text(langs, "eng", "a") == "x\r\ny \"q\" \\ / \t" &&
              text(langs, "eng", "b")->empty() && langs[1].texts.empty() &&
              *text(langs, "zhs", "c") == "\xe5\xbc\x97\xf0\x9f\x98\x80");
    check("keeps raw UTF-8", f1990::parse_agent_strings("{\"jap\": {\"k\": \"\xe3\x83\x95\"}}", langs) &&
                                 *text(langs, "jap", "k") == "\xe3\x83\x95");
    check("rejects a number value", !f1990::parse_agent_strings(R"({"eng": {"a": 1}})", langs, &at) && at == 14 &&
                                        langs.empty());
    check("rejects a lone low surrogate", !f1990::parse_agent_strings(R"({"eng": {"a": "\udc00"}})", langs));
    check("rejects a trailing comma", !f1990::parse_agent_strings(R"({"eng": {"a": "b",}})", langs));
    check("rejects trailing data", !f1990::parse_agent_strings(R"({} x)", langs, &at) && at == 3);
    check("rejects an unterminated string", !f1990::parse_agent_strings(R"({"eng": {"a": "b)", langs));

    std::vector<f1990::StringLeaf> leaves;
    check("parses nested string leaves",
          f1990::parse_string_leaves(R"({"scope": "f1_career_1990", "strings": {"*": {"a": "A", "b": "B"}}, "x": {}})", leaves) &&
              leaves.size() == 3 && leaves[0].path.size() == 1 && leaves[0].path[0] == "scope" &&
              leaves[0].text == "f1_career_1990" && leaves[2].path.size() == 3 && leaves[2].path[1] == "*" &&
              leaves[2].path[2] == "b" && leaves[2].text == "B");
    check("rejects a number leaf", !f1990::parse_string_leaves(R"({"a": {"b": 2}})", leaves) && leaves.empty());

    if (argc > 2) {
        std::ifstream f(argv[2], std::ios::binary);
        std::stringstream ss;
        ss << f.rdbuf();
        bool ok = f1990::parse_string_leaves(ss.str(), leaves, &at);
        size_t frames = 0;
        for (auto &l : leaves)
            frames += l.path.size() == 2 && l.path[0] == "frames";
        check("parses data/rnd-art.json with its frame map", ok && frames > 0);
    }
    if (argc > 3) {
        std::ifstream f(argv[3], std::ios::binary);
        std::stringstream ss;
        ss << f.rdbuf();
        bool ok = f1990::parse_string_leaves(ss.str(), leaves, &at);
        check("parses data/rnd-strings.json", ok && !leaves.empty() && leaves[0].path[0] == "scope");
    }

    if (argc > 5) {
        std::ifstream f(argv[5], std::ios::binary);
        std::stringstream ss;
        ss << f.rdbuf();
        bool ok = f1990::parse_string_leaves(ss.str(), leaves, &at);
        size_t n = 0;
        const std::string *q1 = nullptr, *q2 = nullptr, *results = nullptr;
        for (auto &l : leaves) {
            if (l.path.size() != 3 || l.path[0] != "strings")
                continue;
            n++;
            if (l.path[1] == "eng" && l.path[2] == "lng_sprintshootout_single")
                q1 = &l.text;
            if (l.path[1] == "eng" && l.path[2] == "lng_qualifying")
                q2 = &l.text;
            if (l.path[1] == "jap" && l.path[2] == "lng_qualifying_results")
                results = &l.text;
        }
        check("parses data/session-strings.json", ok && leaves[0].path[0] == "scope" && leaves[0].text == "f1_career_1990");
        check("has 12 session strings in each of the eleven languages", n == 132);
        check("names the Friday session Qualifying 1", q1 && *q1 == "Qualifying 1");
        check("names the Saturday session Qualifying 2", q2 && *q2 == "Qualifying 2");
        check("gives the results headers English text", results && *results == "Qualifying 2 Results");
    }

    if (argc > 6) {
        std::ifstream f(argv[6], std::ios::binary);
        std::stringstream ss;
        ss << f.rdbuf();
        bool ok = f1990::parse_string_leaves(ss.str(), leaves, &at);
        size_t n = 0;
        const std::string *engine = nullptr, *quali = nullptr, *clutch = nullptr;
        for (auto &l : leaves) {
            if (l.path.size() != 3 || l.path[0] != "strings")
                continue;
            n++;
            if (l.path[1] == "*" && l.path[2] == "lng_internal_combustion_engine_title")
                engine = &l.text;
            if (l.path[1] == "*" && l.path[2] == "lng_motor_generator_unit_kinetic_title")
                quali = &l.text;
            if (l.path[1] == "ger" && l.path[2] == "lng_energy_store_title")
                clutch = &l.text;
        }
        check("parses data/pu-strings.json", ok && leaves[0].path[0] == "scope" && leaves[0].text == "f1_career_1990");
        check("has 10 part strings for every language and 5 in each of the eleven", n == 10 + 55);
        check("names the ICE slot Engine", engine && *engine == "Engine");
        check("names the MGU-K slot Qualifying Engine", quali && *quali == "Qualifying Engine");
        check("names the ES slot Kupplung in ger", clutch && *clutch == "Kupplung");
    }

    if (argc > 1) {
        std::ifstream f(argv[1], std::ios::binary);
        std::stringstream ss;
        ss << f.rdbuf();
        bool ok = f1990::parse_agent_strings(ss.str(), langs, &at);
        check("parses data/flavio-strings.json", ok);
        size_t n = 0;
        for (auto &l : langs)
            n += l.texts.size();
        std::printf("     %zu languages, %zu texts\n", langs.size(), n);
        check("has the eleven game languages", langs.size() == 11 && langs[0].code == "bra" && langs.back().code == "zhs");
        const std::string *eng = text(langs, "eng", "lng_elena_name"), *jap = text(langs, "jap", "lng_elena_name");
        check("names the agent Flavio in eng", eng && *eng == "Flavio");
        check("names the agent in jap", jap && *jap == "\xe3\x83\x95\xe3\x83\xa9\xe3\x83\x93\xe3\x82\xaa");
        const std::string *mail = text(langs, "eng", "lng_email_championship_points_tier_1_completed_message");
        check("keeps CR LF in email bodies", mail && mail->find("\r\n\r\nFlavio") != std::string::npos);
    }

    std::printf("%s (%d failure(s))\n", failures ? "FAILED" : "ALL PASSED", failures);
    return failures != 0;
}
