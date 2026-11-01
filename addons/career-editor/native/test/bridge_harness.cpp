// SPDX-License-Identifier: AGPL-3.0-or-later
//
// Host unit harness for bridge.cpp: a fake storage API on a temp folder, a fake state module and fake ops.
#include <cassert>
#include <cstdarg>
#include <cstdio>
#include <cstring>
#include <filesystem>
#include <fstream>
#include <sstream>
#include <string>
#include <thread>

#include <swofty/mixin.hpp>
#include "bridge.hpp"

namespace fs = std::filesystem;
static fs::path root;
static int fails = 0;
#define CHECK(c) do { if (!(c)) { std::printf("FAIL %s:%d %s\n", __FILE__, __LINE__, #c); ++fails; } } while (0)

static void flog(const char *fmt, ...) { va_list ap; va_start(ap, fmt); std::vprintf(fmt, ap); va_end(ap); std::printf("\n"); }
static std::thread::id main_id = std::this_thread::get_id();
static int main_io = 0;
static void io() { if (std::this_thread::get_id() == main_id) ++main_io; }
static int s_avail() { return 1; }
static int s_read(const char *p, void *buf, size_t cap, size_t *len) { io();
    std::ifstream f(root / p, std::ios::binary); if (!f) return SWOFTY_ASSET_INVALID;
    std::stringstream ss; ss << f.rdbuf(); std::string d = ss.str(); if (len) *len = d.size();
    if (d.size() > cap) return SWOFTY_ASSET_INVALID; memcpy(buf, d.data(), d.size()); return SWOFTY_ASSET_OK; }
static int s_write(const char *p, const void *d, size_t n) { io();
    fs::create_directories((root / p).parent_path()); auto tmp = root / (std::string(p) + ".tmp");
    { std::ofstream f(tmp, std::ios::binary); f.write((const char *)d, n); } fs::rename(tmp, root / p); return SWOFTY_ASSET_OK; }
static int s_append(const char *p, const void *d, size_t n) { io();
    fs::create_directories((root / p).parent_path()); std::ofstream f(root / p, std::ios::binary | std::ios::app); f.write((const char *)d, n); return SWOFTY_ASSET_OK; }
static int s_remove(const char *p) { io(); return fs::remove(root / p) ? SWOFTY_ASSET_OK : SWOFTY_ASSET_INVALID; }
static int s_size(const char *p, uint64_t *o) { io(); std::error_code ec; auto n = fs::file_size(root / p, ec); if (ec) return SWOFTY_ASSET_INVALID; *o = n; return SWOFTY_ASSET_OK; }
static int s_list(const char *d, char *buf, size_t cap, size_t *len) { io();
    std::error_code ec; std::string out; for (auto &e : fs::directory_iterator(root / d, ec)) { out += e.path().filename().string(); if (e.is_directory()) out += '/'; out += '\n'; }
    if (ec) return SWOFTY_ASSET_INVALID; if (len) *len = out.size(); if (out.size() > cap) return SWOFTY_ASSET_INVALID; memcpy(buf, out.data(), out.size()); return SWOFTY_ASSET_OK; }
static SwoftyStorage storage_tab = {s_avail, s_read, s_write, s_append, s_remove, s_size, s_list};

// ---- fake state ----
static bool f_career = false, f_safe = false;
namespace ce {
Mode mode() { return f_career ? Mode::MyTeam : Mode::None; }
bool in_career() { return f_career; }
bool safe() { return f_career && f_safe; }
namespace state {
void poll(uint64_t) {}
void write_status(Writer &w, uint64_t) { w.begin_object(); w.key("career"); w.boolean(f_career); w.key("safe"); w.boolean(safe()); w.key("season"); const Json *s = bridge::section("season"); if (s) w.num((*s)["season_number"].n); else w.null(); w.end_object(); }
std::string blocked(bool anytime) { if (!f_career) return "no career is loaded"; if (anytime) return {}; return f_safe ? "" : "not in Team HQ"; }
bool sections_ok() { return f_career; }
const char *phase() { return "team_hq"; }
const char *mode_name(Mode m) { return m == Mode::MyTeam ? "my_team" : "none"; }
}
}

// ---- fake feature module ----
static int64_t cash = 1000;
static ce::OpResult cash_set(const ce::Json &a) {
    ce::OpResult r; if (a["value"].type != ce::Json::Num) { r.error = "value must be a number"; return r; }
    if (a["expect"].type == ce::Json::Num && (int64_t)a["expect"].n != cash) { r.error = "stale"; return r; }
    r.before = "{\"cash\":" + std::to_string(cash) + "}"; cash = (int64_t)a["value"].n; r.after = "{\"cash\":" + std::to_string(cash) + "}"; r.ok = true; return r; }
static ce::OpResult weather_set(const ce::Json &a) { ce::OpResult r; r.ok = true; r.before = "\"dry\""; r.after = "\"" + a["value"].str() + "\""; return r; }
static ce::OpResult thrower(const ce::Json &) { throw std::runtime_error("boom"); }
static int econ_renders = 0;
static void econ_section(ce::Writer &w) { ++econ_renders; w.begin_object(); w.key("cash"); w.integer(cash); w.key("name"); w.str("Team \"Q\"\n"); w.key("ratio"); w.num(0.1); w.key("big"); w.num(1e300); w.key("bad"); w.str(std::string("\x90\xcc*@\x01ok\xe2\x82")); w.key("\xff"); w.integer(1); w.key("arr"); w.begin_array(); w.integer(1); w.null(); w.boolean(true); w.begin_object(); w.end_object(); w.end_array(); w.end_object(); }
static void season_section(ce::Writer &w) { w.begin_object(); w.key("season_number"); w.integer(2); w.end_object(); }
static void broken_section(ce::Writer &w) { w.key("x"); w.integer(1); }
CE_OP("economy.cash.set", cash_set);
CE_OP("economy.cash.set", cash_set);
CE_OP_ANYTIME("weather.session.set", weather_set);
CE_OP("test.throw", thrower);
CE_SECTION("economy", econ_section);
CE_SECTION("season", season_section);
CE_SECTION("broken", broken_section);

static std::string slurp(const fs::path &p) { std::ifstream f(p); std::stringstream ss; ss << f.rdbuf(); return ss.str(); }
static void put(uint64_t seq, const std::string &body) {
    auto tmp = root / "inbox" / (std::to_string(seq) + ".json.tmp"); { std::ofstream f(tmp); f << body; } fs::rename(tmp, root / "inbox" / (std::to_string(seq) + ".json")); }
static bool viewer_on = false;
static void touch_viewer() { { std::ofstream f(root / "viewer.json.tmp"); f << ce::wall_ms(); } fs::rename(root / "viewer.json.tmp", root / "viewer.json"); }
static void pump(int n) { for (int i = 0; i < n; ++i) { if (viewer_on) touch_viewer(); ce::bridge::frame(); usleep(30000); } ce::bridge::flush_writes(); }
static ce::Json parse(const std::string &s) { ce::Json v; bool ok = json::parse(s, v); if (!ok) std::printf("PARSE FAIL: %s\n", s.c_str()); CHECK(ok); return v; }

int main() {
    root = fs::path(std::getenv("CE_TEST_DIR") ? std::getenv("CE_TEST_DIR") : fs::temp_directory_path().string()) / ("ce-harness-" + std::to_string(getpid()));
    fs::create_directories(root);
    static SwoftyApi api{}; api.version = 5; api.log = flog; api.storage = &storage_tab; swofty::api = &api;
    ce::bridge::start();
    CHECK(fs::exists(root / "inbox/.keep"));
    pump(2);
    auto snap = parse(slurp(root / "snapshot.json"));
    CHECK(snap["v"].n == 1);
    CHECK(snap["addon"]["version"].str() == parse(slurp("../../swofty.json"))["version"].str());
    CHECK(snap["status"]["career"].type == ce::Json::Bool && !snap["status"]["career"].b);
    CHECK(snap["bridge"]["ops"].a.size() == 3);
    CHECK(snap["bridge"]["duplicates"].a.size() == 1);

    // 1: no career: Team HQ op blocked, stays queued
    put(100, R"({"seq":100,"session":"s1","ops":[{"op":"economy.cash.set","args":{"value":5000},"expect":{"cash":1000}}]})");
    pump(40);
    CHECK(fs::exists(root / "inbox/100.json"));
    snap = parse(slurp(root / "snapshot.json"));
    CHECK(snap["bridge"]["blocked"]["seq"].n == 100);
    CHECK(snap["bridge"]["blocked"]["why"].str() == "no career is loaded");
    // 2a: career, launcher page not open: sections are not rendered, the status still is, and the frame does no file IO
    main_io = 0; f_career = true; pump(40);
    CHECK(main_io == 0);
    snap = parse(slurp(root / "snapshot.json"));
    CHECK(snap["sections"].type == ce::Json::Obj && snap["sections"].o.empty());
    CHECK(snap["bridge"]["renders"].n == 0);
    CHECK(!snap["bridge"]["viewer"].b);
    CHECK(slurp(root / "viewer.json") == "0");
    // 2: career but not safe: still blocked; the page is open, so sections render one per frame
    viewer_on = true; pump(40);
    CHECK(fs::exists(root / "inbox/100.json"));
    snap = parse(slurp(root / "snapshot.json"));
    CHECK(snap["bridge"]["blocked"]["why"].str() == "not in Team HQ");
    CHECK(snap["sections"]["economy"]["cash"].n == 1000);
    CHECK(snap["sections"]["broken"]["error"].str() == "the section wrote invalid JSON");
    CHECK(snap["status"]["season"].n == 2);
    CHECK(snap["bridge"]["viewer"].b && snap["bridge"]["renders"].n >= 1);
    CHECK(snap["sections"]["economy"]["name"].str() == "Team \"Q\"\n");
    CHECK(snap["sections"]["economy"]["ratio"].n == 0.1);
    // 2b: an unchanged section is not rendered again every second: 100 frames (3 s) cost at most 2 renders
    int before_renders = econ_renders; pump(100);
    CHECK(econ_renders - before_renders <= 2);
    // 3: safe -> runs. expect is an object here -> op sees expect object -> not number -> no stale check
    f_safe = true; pump(40);
    CHECK(!fs::exists(root / "inbox/100.json"));
    CHECK(fs::exists(root / "outbox/100.json"));
    auto ack = parse(slurp(root / "outbox/100.json"));
    CHECK(ack["ok"].b);
    CHECK(ack["results"].a[0]["before"]["cash"].n == 1000);
    CHECK(ack["results"].a[0]["after"]["cash"].n == 5000);
    CHECK(cash == 5000);
    // 4: stale expect (number) -> fails
    put(101, R"({"seq":101,"ops":[{"op":"economy.cash.set","args":{"value":1},"expect":999},{"op":"nope.op"},{"op":"bridge.ping"},{"op":"test.throw"}]})");
    pump(30);
    ack = parse(slurp(root / "outbox/101.json"));
    CHECK(!ack["ok"].b);
    CHECK(ack["results"].a.size() == 4);
    CHECK(ack["results"].a[0]["error"].str() == "stale");
    CHECK(ack["results"].a[1]["error"].str().find("unknown op") == 0);
    CHECK(ack["results"].a[2]["ok"].b);
    CHECK(ack["results"].a[3]["error"].str() == "exception: boom");
    CHECK(cash == 5000);
    // 5: anytime op runs while not safe
    f_safe = false;
    put(102, R"({"seq":102,"ops":[{"op":"weather.session.set","args":{"value":"wet"}}]})");
    pump(30);
    ack = parse(slurp(root / "outbox/102.json"));
    CHECK(ack["ok"].b && ack["results"].a[0]["after"].str() == "wet");
    // 6: mixed batch waits for safe
    put(103, R"({"seq":103,"ops":[{"op":"weather.session.set","args":{"value":"dry"}},{"op":"economy.cash.set","args":{"value":7}}]})");
    pump(30);
    CHECK(fs::exists(root / "inbox/103.json"));
    // 7: bad json + ordering: 104 bad waits behind 103? (103 blocked, so 104 waits too)
    put(104, "{not json");
    pump(30);
    CHECK(fs::exists(root / "inbox/104.json"));
    f_safe = true; pump(40);
    CHECK(fs::exists(root / "outbox/103.json") && fs::exists(root / "outbox/104.json"));
    ack = parse(slurp(root / "outbox/104.json"));
    CHECK(!ack["ok"].b && ack["error"].str() == "the batch is not a JSON object");
    CHECK(cash == 7);
    // 8: withdraw: put then delete before it runs
    f_safe = false;
    put(105, R"({"ops":[{"op":"economy.cash.set","args":{"value":9}}]})");
    pump(30); fs::remove(root / "inbox/105.json"); f_safe = true; pump(40);
    CHECK(!fs::exists(root / "outbox/105.json"));
    CHECK(cash == 7);
    // 9: already-acked seq re-appears -> dropped
    put(103, R"({"ops":[{"op":"economy.cash.set","args":{"value":11}}]})");
    pump(30);
    CHECK(cash == 7);
    // journal
    std::string j = slurp(root / "journal.jsonl");
    int lines = 0; for (char c : j) lines += c == '\n';
    CHECK(lines == 4); // 100 cash, 102 weather, 103 weather + cash
    std::istringstream js(j); std::string line; while (std::getline(js, line)) parse(line);
    // writer edge: snapshot parses, big number -> valid
    snap = parse(slurp(root / "snapshot.json"));
    CHECK(snap["sections"]["economy"]["big"].n == 1e300);
    CHECK(snap["sections"]["economy"]["bad"].str() == "\xef\xbf\xbd\xef\xbf\xbd*@\x01ok\xef\xbf\xbd\xef\xbf\xbd");
    {
        std::string raw = slurp(root / "snapshot.json");
        bool valid = true;
        for (size_t i = 0; i < raw.size();) {
            unsigned char c = raw[i];
            size_t len = c < 0x80 ? 1 : (c & 0xe0) == 0xc0 ? 2 : (c & 0xf0) == 0xe0 ? 3 : (c & 0xf8) == 0xf0 ? 4 : 0;
            if (!len || i + len > raw.size()) { valid = false; break; }
            for (size_t k = 1; k < len; ++k)
                if ((raw[i + k] & 0xc0) != 0x80) valid = false;
            if (c < 0x20) valid = false;
            i += len;
        }
        CHECK(valid);
    }
    CHECK(snap["bridge"]["applied"].n == 4);
    std::printf("%s (%d failure(s)); files in %s\n", fails ? "FAILED" : "ALL OK", fails, root.c_str());
    return fails ? 1 : 0;
}
