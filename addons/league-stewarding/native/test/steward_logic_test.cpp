// SPDX-License-Identifier: AGPL-3.0-or-later

#include "router.hpp"

#include <cstdio>
#include <string>
#include <vector>

using namespace steward;

namespace {

int failures = 0;

void expect(bool ok, const char *what, int line)
{
    if (!ok) {
        std::printf("FAIL line %d: %s\n", line, what);
        ++failures;
    }
}

#define EXPECT(x) expect((x), #x, __LINE__)

struct Mock final : Backend {
    std::vector<PenaltyAction> penalties;
    std::vector<RcAction> actions;
    int auto_calls = 0;
    bool auto_value = true;
    std::string refuse;

    std::string state_json() override
    {
        StateView v;
        v.role = "host";
        v.snap.track = "Mon\"za";
        Event b;
        b.t = 12.5;
        b.code = "PENA";
        v.snap.events.push_back(b);
        Event l;
        l.t = 10;
        l.code = "LOCAL";
        v.local_events.push_back(l);
        Driver d;
        d.index = 3;
        d.name = "A";
        d.penalty_s = 5;
        v.snap.drivers.push_back(d);
        return steward::state_json(v);
    }
    std::string queue_penalty(const PenaltyAction &a) override
    {
        if (!refuse.empty())
            return refuse;
        penalties.push_back(a);
        return {};
    }
    std::string queue_race_control(RcAction a) override
    {
        actions.push_back(a);
        return {};
    }
    void set_auto_penalties(bool e) override
    {
        auto_calls++;
        auto_value = e;
    }
};

const char kToken[] = "0123456789abcdef0123456789abcdef";
constexpr uint16_t kPort = 41234;

const Asset kAssets[] = {
    {"index.html", "text/html; charset=utf-8", "<link href=\"style.css\"><script src=\"app.js\"></script>"},
    {"app.js", "text/javascript", "x"},
    {"style.css", "text/css", "y"},
};

std::string raw(const std::string &method, const std::string &target, const std::string &body = {}, bool token_header = false,
                const std::string &host = "127.0.0.1:41234")
{
    std::string r = method + " " + target + " HTTP/1.1\r\nHost: " + host + "\r\n";
    if (token_header)
        r += std::string("X-Steward-Token: ") + kToken + "\r\n";
    if (!body.empty())
        r += "Content-Length: " + std::to_string(body.size()) + "\r\n";
    return r + "\r\n" + body;
}

Reply call(Mock &m, const std::string &request)
{
    http::Request req;
    http::Parse p = http::parse_request(request, req);
    if (p != http::Parse::ok)
        return Reply{500, "text/plain", "parse"};
    return route(req, kToken, kPort, m, kAssets);
}

std::string q(const std::string &path)
{
    return path + (path.find('?') == std::string::npos ? "?t=" : "&t=") + kToken;
}

void test_parse()
{
    http::Request r;
    EXPECT(http::parse_request("GET /x HTTP/1.1\r\nHost: a", r) == http::Parse::incomplete);
    EXPECT(http::parse_request("GET\r\n\r\n", r) == http::Parse::bad);
    EXPECT(http::parse_request("GET x HTTP/1.1\r\n\r\n", r) == http::Parse::bad);
    EXPECT(http::parse_request("POST /a?b=1&t=zz HTTP/1.1\r\nhost: h\r\nContent-Length: 4\r\n\r\nab", r) == http::Parse::incomplete);
    EXPECT(http::parse_request("POST /a?b=1&t=zz HTTP/1.1\r\nhost: h\r\nContent-Length: 4\r\n\r\nabcd", r) == http::Parse::ok);
    EXPECT(r.method == "POST" && r.path == "/a" && r.query == "b=1&t=zz" && r.body == "abcd" && r.host == "h");
    EXPECT(http::query_param(r.query, "t") == "zz" && http::query_param(r.query, "b") == "1" && http::query_param(r.query, "c").empty());
    EXPECT(http::parse_request("POST / HTTP/1.1\r\nContent-Length: 99999999\r\n\r\n", r) == http::Parse::too_large);
    EXPECT(http::parse_request("POST / HTTP/1.1\r\nContent-Length: x\r\n\r\n", r) == http::Parse::bad);
}

void test_json()
{
    std::string s;
    double n = 0;
    bool b = true;
    std::string body = "{\"car\": 4, \"kind\":\"time\",\"reason\":\"a \\\"b\\\"\", \"enabled\" :false}";
    EXPECT(http::json_number(body, "car", n) && n == 4);
    EXPECT(http::json_string(body, "kind", s) && s == "time");
    EXPECT(http::json_string(body, "reason", s) && s == "a \"b\"");
    EXPECT(http::json_bool(body, "enabled", b) && !b);
    EXPECT(!http::json_number(body, "kind", n));
    EXPECT(!http::json_string(body, "missing", s));
    EXPECT(http::json_escape("a\"\n\x01") == "a\\\"\\n\\u0001");
}

void test_parse_penalty()
{
    PenaltyAction a;
    std::string err;
    EXPECT(parse_penalty("{\"car\":2,\"kind\":\"time\",\"seconds\":10,\"reason\":\"x\"}", a, err) && a.car == 2 &&
           a.kind == Kind::time && a.seconds == 10 && a.text == "x" && a.reason == kReasonDefault);
    EXPECT(parse_penalty("{\"car\":0,\"kind\":\"warning\"}", a, err) && a.kind == Kind::warning);
    EXPECT(parse_penalty("{\"car\":0,\"kind\":\"grid\",\"places\":3}", a, err) && a.places == 3);
    EXPECT(parse_penalty("{\"car\":0,\"kind\":\"dsq\",\"infringement\":22}", a, err) && a.reason == 22);
    EXPECT(!parse_penalty("{\"car\":26,\"kind\":\"warning\"}", a, err));
    EXPECT(!parse_penalty("{\"car\":-1,\"kind\":\"warning\"}", a, err));
    EXPECT(!parse_penalty("{\"car\":1.5,\"kind\":\"warning\"}", a, err));
    EXPECT(!parse_penalty("{\"kind\":\"warning\"}", a, err));
    EXPECT(!parse_penalty("{\"car\":1,\"kind\":\"banana\"}", a, err));
    EXPECT(!parse_penalty("{\"car\":1,\"kind\":\"time\",\"seconds\":0}", a, err));
    EXPECT(!parse_penalty("{\"car\":1,\"kind\":\"time\",\"seconds\":9999}", a, err));
    EXPECT(!parse_penalty("{\"car\":1,\"kind\":\"grid\",\"places\":0}", a, err));
    EXPECT(!parse_penalty("{\"car\":1,\"kind\":\"warning\",\"infringement\":99}", a, err));
}

void test_race_control_table()
{
    EXPECT(rc_target(RcAction::sc, kScIdle) == kScDeploy);
    EXPECT(rc_target(RcAction::sc, kScVscDeploy) == kScDeploy);
    EXPECT(rc_target(RcAction::sc, kScDeploy) == -1);
    EXPECT(rc_target(RcAction::vsc, kScIdle) == kScVscDeploy);
    EXPECT(rc_target(RcAction::vsc, kScDeploy) == -1);
    EXPECT(rc_target(RcAction::end, kScDeploy) == kScReturn);
    EXPECT(rc_target(RcAction::end, kScPacing) == kScReturn);
    EXPECT(rc_target(RcAction::end, kScVscDeploy) == kScVscEnding);
    EXPECT(rc_target(RcAction::end, kScIdle) == -1);
    EXPECT(rc_target(RcAction::red_flag, kScIdle) == kScRedFlagIntro);
    EXPECT(rc_target(RcAction::red_flag, kScGroup) == kScRedFlagIntro);
    EXPECT(rc_target(RcAction::red_flag, kScRedFlagDeploy) == -1);
    EXPECT(std::string(rc_state_name(kScIdle)) == "none");
    EXPECT(std::string(rc_state_name(kScDeploy)) == "sc");
    EXPECT(std::string(rc_state_name(kScReturn)) == "sc_ending");
    EXPECT(std::string(rc_state_name(kScVscDeploy)) == "vsc");
    EXPECT(std::string(rc_state_name(kScVscEnding)) == "vsc_ending");
    EXPECT(std::string(rc_state_name(kScRedFlagDeploy)) == "red_flag");
    EXPECT(rc_refusal(RcAction::sc, -1).find("no safety car") != std::string::npos);
}

void test_routes()
{
    Mock m;
    EXPECT(call(m, raw("GET", "/api/state")).status == 403);
    EXPECT(call(m, raw("GET", "/api/state?t=wrong")).status == 403);
    EXPECT(call(m, raw("GET", q("/api/state"), {}, false, "evil.example:41234")).status == 403);
    EXPECT(call(m, raw("GET", q("/api/state"), {}, false, "127.0.0.1:1")).status == 403);
    Reply s = call(m, raw("GET", q("/api/state")));
    EXPECT(s.status == 200 && s.type == "application/json");
    EXPECT(s.body.find("\"role\":\"host\"") != std::string::npos);
    EXPECT(s.body.find("Mon\\\"za") != std::string::npos);
    EXPECT(s.body.find("\"last_packet_age_s\":null") != std::string::npos);
    EXPECT(s.body.find("\"penalty_s\":5.0") != std::string::npos);
    EXPECT(s.body.find("\"code\":\"LOCAL\"") < s.body.find("\"code\":\"PENA\""));
    EXPECT(call(m, raw("GET", "/api/state", {}, true)).status == 200);
    EXPECT(call(m, raw("POST", q("/api/state"))).status == 405);
    EXPECT(call(m, raw("GET", q("/api/penalty"))).status == 405);

    Reply p = call(m, raw("POST", q("/api/penalty"), "{\"car\":7,\"kind\":\"stop_go\"}"));
    EXPECT(p.status == 200 && p.body.find("\"ok\":true") != std::string::npos);
    EXPECT(m.penalties.size() == 1 && m.penalties[0].car == 7 && m.penalties[0].kind == Kind::stop_go);
    EXPECT(call(m, raw("POST", q("/api/penalty"), "{\"car\":99,\"kind\":\"stop_go\"}")).status == 400);
    m.refuse = "no race";
    Reply r = call(m, raw("POST", q("/api/penalty"), "{\"car\":1,\"kind\":\"warning\"}"));
    EXPECT(r.status == 200 && r.body == "{\"ok\":false,\"error\":\"no race\"}");

    EXPECT(call(m, raw("POST", q("/api/race-control"), "{\"action\":\"vsc\"}")).body.find("true") != std::string::npos);
    EXPECT(m.actions.size() == 1 && m.actions[0] == RcAction::vsc);
    EXPECT(call(m, raw("POST", q("/api/race-control"), "{\"action\":\"nope\"}")).status == 400);

    EXPECT(call(m, raw("POST", q("/api/auto-penalties"), "{\"enabled\":false}")).status == 200);
    EXPECT(m.auto_calls == 1 && !m.auto_value);
    EXPECT(call(m, raw("POST", q("/api/auto-penalties"), "{}")).status == 400);
    EXPECT(call(m, raw("POST", q("/api/nothing"), "{}")).status == 404);
}

void test_assets()
{
    Mock m;
    Reply idx = call(m, raw("GET", q("/")));
    EXPECT(idx.status == 200 && idx.type.rfind("text/html", 0) == 0);
    EXPECT(idx.body.find(std::string("\"style.css?t=") + kToken + "\"") != std::string::npos);
    EXPECT(idx.body.find(std::string("\"app.js?t=") + kToken + "\"") != std::string::npos);
    Reply js = call(m, raw("GET", q("/app.js")));
    EXPECT(js.status == 200 && js.body == "x" && js.type == "text/javascript");
    EXPECT(call(m, raw("GET", "/app.js")).status == 403);
    EXPECT(call(m, raw("GET", q("/missing.js"))).status == 404);
    std::string wire = http::response(200, "text/plain", "hi");
    EXPECT(wire.find("Content-Length: 2\r\n") != std::string::npos && wire.rfind("hi") == wire.size() - 2);
}

}

int main()
{
    test_parse();
    test_json();
    test_parse_penalty();
    test_race_control_table();
    test_routes();
    test_assets();
    if (failures)
        std::printf("%d failure(s)\n", failures);
    else
        std::printf("steward logic tests passed\n");
    return failures ? 1 : 0;
}
