// Host unit test for the plain save store (plain_saves_logic.hpp): settings, manifests, the generation and CURRENT protocol,
// the put-back decision, inject requests taken once, the replace of a registered mode before its only read and the SHA-256
// the manifests carry. native/test/run_tests.sh plain_saves builds and runs it.
#include "core/career/plain_saves_logic.hpp"

#include <array>
#include <cstdio>
#include <cstring>
#include <string>
#include <vector>

namespace ps = career::plain_saves;
namespace sp = career::save_splice;
namespace fs = std::filesystem;

static int fails;

static void expect(bool ok, const std::string &what)
{
    printf("%s: %s\n", ok ? "ok  " : "FAIL", what.c_str());
    fails += !ok;
}

static std::shared_ptr<const std::vector<uint8_t>> bytes_of(const std::string &s, uint8_t salt = 0)
{
    auto v = std::make_shared<std::vector<uint8_t>>(s.begin(), s.end());
    for (auto &b : *v)
        b ^= salt;
    return v;
}

static ps::Body body(const std::string &name, const std::string &data, const char *source, size_t careers = 0, bool reg = false)
{
    ps::Body b;
    b.name = name;
    b.bytes = bytes_of(data);
    b.source = source;
    b.registered = reg;
    for (size_t i = 0; i < careers; i++) {
        ps::Slot s;
        s.slot = i, s.guid = 0x0123456789abcdefull + i, s.name = "Driver \"" + std::to_string(i) + "\"", s.season = 2,
        s.round = 5;
        s.team = 123, s.driver = 456, s.last_played = 1795000000, s.flags = 0;
        b.slots.push_back(s);
    }
    return b;
}

static ps::Header header(const char *event, const char *session = "s1")
{
    ps::Header h;
    h.event = event;
    h.core_version = "1.10.0";
    h.exe_sha256 = "ab";
    h.profile = "default";
    h.session = session;
    h.written_at = 1795000000;
    h.loaded_addons = {"swofty.progressive"};
    return h;
}

static void sha()
{
    expect(ps::Sha256::of("", 0) == "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855", "sha256: empty");
    expect(ps::Sha256::of("abc", 3) == "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad", "sha256: abc");
    std::string m = "abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq";
    expect(ps::Sha256::of(m.data(), m.size()) == "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1",
           "sha256: two-block message");
    std::string big(1000000, 'a');
    expect(ps::Sha256::of(big.data(), big.size()) == "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0",
           "sha256: a million a");
}

static void settings()
{
    expect(!ps::parse_settings("").present && !ps::parse_settings("{}").present, "settings: no key, nothing written");
    auto ok = ps::parse_settings(R"({"plain_saves":{"root":"Z:\\data\\saves\\default\\plain","schema":1}})");
    expect(ok.present && ok.ok && ok.root == "Z:\\data\\saves\\default\\plain", "settings: a good key");
    expect(ps::parse_settings(R"({"plain_saves":{"root":"x","schema":2}})").present &&
               !ps::parse_settings(R"({"plain_saves":{"root":"x","schema":2}})").ok,
           "settings: another schema is refused");
    expect(!ps::parse_settings(R"({"plain_saves":{"schema":1}})").ok, "settings: no root is refused");
    expect(!ps::parse_settings(R"({"plain_saves":{"root":"Z:\\a\\..\\b","schema":1}})").ok, "settings: a .. part is refused");
    expect(!ps::parse_settings(R"({"plain_saves":"yes"})").ok, "settings: a non-object is refused");
}

static void generations(const fs::path &tmp)
{
    fs::remove_all(tmp);
    ps::Root root(tmp / "plain");
    expect(!root.current() && root.next_number() == 1, "store: empty store starts at generation 1");
    std::string why;
    auto b1 = std::vector<ps::Body>{body("driver_career_progressive", "career-bytes-1", "kept", 2),
                                    body("swofty_registry", "reg", "kept"), body("career_mode", "mode-bytes", "game", 1, true)};
    uint64_t g1 = root.write_generation(header("load"), b1, why);
    expect(g1 == 1 && root.current() == 1u, "store: first generation moves CURRENT");
    auto cur = ps::read_text(root.path() / ps::CURRENT);
    expect(cur == "1\n", "store: CURRENT is decimal text");
    expect(fs::is_regular_file(root.generation_dir(1) / "manifest.json"), "store: manifest.json is there");
    expect(root.generation_dir(1).filename() == "000000000001", "store: generation folder is zero-padded to 12 digits");
    std::string f = ps::container_file("driver_career_progressive");
    expect(f.size() == 20 && f.substr(16) == ".bag", "store: container file is 16 hex digits and .bag");
    expect(fs::file_size(root.generation_dir(1) / "containers" / f) == 14, "store: the bag file holds the bytes as given");

    auto m = ps::read_generation(root, 1);
    expect(m && m->event == "load" && m->generation == 1 && m->containers.size() == 3, "manifest: reads back");
    expect(m && m->containers[0].careers == 2 && m->containers[0].source == "kept" && !m->containers[0].registered,
           "manifest: careers and source of a kept container");
    expect(m && m->containers[2].registered && m->containers[2].source == "game", "manifest: a registered container");
    auto raw = ps::read_text(root.generation_dir(1) / "manifest.json");
    expect(raw.find("\"guid\": \"0x0123456789abcdef\"") != std::string::npos, "manifest: slot guid is 0x plus 16 hex digits");
    expect(raw.find("Driver \\\"0\\\"") != std::string::npos, "manifest: names are escaped");
    expect(raw.find("\"loaded_addons\": [\"swofty.progressive\"]") != std::string::npos, "manifest: loaded addons");
    expect(raw.find("\"fnv64\": \"" + f.substr(0, 16) + "\"") != std::string::npos, "manifest: fnv64 names the bag file");
    auto e = m->containers[0];
    expect(e.sha256 == ps::Sha256::of(ps::read_bytes(root.generation_dir(1) / "containers" / f).data(), 14),
           "manifest: sha256 is of the .bag file");
    expect(ps::read_container(root, 1, e).has_value(), "store: an intact container reads");
    auto nulls = ps::manifest_json(9, header("save", ""), {});
    expect(nulls.find("\"session\": null") != std::string::npos, "manifest: no session is null");

    bool tmp_left = false;
    for (auto &p : fs::recursive_directory_iterator(root.path()))
        tmp_left |= p.path().string().find(".swofty-tmp") != std::string::npos;
    expect(!tmp_left, "store: no tmp file is left behind");

    fs::path bag = root.generation_dir(1) / "containers" / f;
    {
        FILE *fp = fopen(bag.string().c_str(), "r+b");
        fputc('X', fp);
        fclose(fp);
    }
    expect(!ps::read_container(root, 1, e).has_value(), "store: a container that no longer matches its sha256 is refused");

    fs::create_directories(root.path() / "generations" / "000000000007");
    expect(root.next_number() == 8, "store: a folder past CURRENT is never reused");
    fs::remove_all(root.path() / "generations" / "000000000007");
    fs::create_directories(root.path() / "generations");
    {
        FILE *fp = fopen((root.path() / "generations" / "000000000002").string().c_str(), "wb");
        fclose(fp);
    }
    uint64_t bad = root.write_generation(header("save"), b1, why);
    expect(bad == 0 && root.current() == 1u && !why.empty(), "store: a failed generation leaves CURRENT where it was");
    fs::remove(root.path() / "generations" / "000000000002");
}

static void putback(const fs::path &tmp)
{
    fs::remove_all(tmp);
    ps::Root root(tmp / "plain");
    std::string why;
    expect(!ps::newest_core_save(root).has_value(), "put back: an empty store has nothing");
    root.write_generation(
        header("save"),
        {body("A_mode", "a1", "game", 1, true), body("B_career", "b1", "kept", 2), body("C_career", "c1", "kept", 1)}, why);
    root.write_generation(header("load"), {body("A_mode", "a1", "game", 1, true)}, why);
    root.write_generation(header("save", ""), {body("D_old", "d", "kept")}, why);
    auto newest = ps::newest_core_save(root);
    expect(newest && newest->generation == 1 && newest->event == "save",
           "put back: newest save of a core session, skipping loads and sessionless ones");

    auto picks = ps::put_back(*newest, {{"A_mode", ""}}).picks;
    expect(picks.size() == 2 && picks[0].entry.name == "B_career" && picks[1].entry.name == "C_career" &&
               picks[0].generation == 1,
           "put back: only the containers the tree lacks come back");
    expect(ps::put_back(*newest, {{"A_mode", ""}, {"B_career", ""}, {"C_career", ""}}).picks.empty(),
           "put back: a container the tree has is never replaced");
    expect(ps::put_back_line("B_career", 2, 1) == "[saves] put back B_career: 2 career(s) from generation 1",
           "put back: the log line");

    root.write_generation(header("save"), {body("A_mode", "a2", "game", 1, true), body("C_career", "c2", "kept", 1)}, why);
    newest = ps::newest_core_save(root);
    expect(newest && newest->generation == 4, "deletions: the newer save is the one to put back from");
    picks = ps::put_back(*newest, {{"A_mode", ""}}).picks;
    expect(picks.size() == 1 && picks[0].entry.name == "C_career",
           "deletions: B_career, gone from the newest save, is not brought back from an older one");

    fs::remove(root.path() / ps::CURRENT);
    expect(!ps::newest_core_save(root).has_value(), "put back: with no CURRENT nothing is trusted");
    FILE *fp = fopen((root.path() / ps::CURRENT).string().c_str(), "wb");
    fputs("3\n", fp);
    fclose(fp);
    newest = ps::newest_core_save(root);
    expect(newest && newest->generation == 1, "put back: a generation above CURRENT (an unfinished write) is ignored");
}

static void pairs(const fs::path &tmp)
{
    fs::remove_all(tmp);
    ps::Root root(tmp / "plain");
    std::string why;
    expect(ps::pair_of("gm_driver_career_progressive") == "driver_career_progressive" &&
               ps::pair_of("driver_career_progressive_SaveSlotGameModeComponent") == "driver_career_progressive" &&
               ps::pair_of("gm_my_team_career_25") == "my_team_career_25" && ps::pair_of("settings").empty() &&
               ps::pair_of("gm_").empty(),
           "pairs: a mode's two halves name the same pair");
    auto g = body("gm_driver_career_1990", "gm-bytes", "game", 0, true);
    auto sl = body("driver_career_1990_SaveSlotGameModeComponent", "slot-bytes", "game", 2, true);
    root.write_generation(header("save"), {g, sl, body("settings", "s", "game", 0, true)}, why);
    auto raw = ps::read_text(root.generation_dir(1) / "manifest.json");
    expect(raw.find("\"pair\": \"driver_career_1990\"") != std::string::npos && raw.find("\"pair\": null") != std::string::npos,
           "pairs: the manifest names the pair of each half and null for the rest");
    auto m = *ps::newest_core_save(root);
    auto sha = [&](const char *s) { return ps::Sha256::of(s, strlen(s)); };
    auto none = ps::put_back(m, {});
    expect(none.picks.size() == 3 && none.skipped.empty(), "pairs: with nothing loaded both halves come back");
    auto both =
        ps::put_back(m, {{"gm_driver_career_1990", sha("gm-bytes")}, {"driver_career_1990_SaveSlotGameModeComponent", "x"}});
    expect(both.picks.size() == 1 && both.picks[0].entry.name == "settings", "pairs: a pair the tree holds whole is left alone");
    auto same = ps::put_back(m, {{"gm_driver_career_1990", sha("gm-bytes")}});
    expect(same.picks.size() == 2 && same.picks[0].entry.name == "driver_career_1990_SaveSlotGameModeComponent" &&
               same.skipped.empty(),
           "pairs: an absent half comes back when the present half is the one in the generation");
    auto diff = ps::put_back(m, {{"gm_driver_career_1990", sha("other")}});
    expect(diff.picks.size() == 1 && diff.picks[0].entry.name == "settings" && diff.skipped.size() == 1 &&
               diff.skipped[0].reason.find("differs") != std::string::npos,
           "pairs: an absent half is skipped, with the reason, when the present half differs");
    auto unknown = ps::put_back(m, {{"driver_career_1990_SaveSlotGameModeComponent", ""}});
    expect(unknown.skipped.size() == 1 && unknown.skipped[0].name == "gm_driver_career_1990",
           "pairs: a present half whose bytes are not known counts as different");
    auto grp = ps::Request{"r", "gm_driver_career_1990", 1, true};
    (void)grp;
    expect(ps::decide(grp, ps::Held::registered, true).action == ps::Action::replace,
           "pairs: a replace request for one half decides as replace");
}

static void requests(const fs::path &tmp)
{
    fs::remove_all(tmp);
    ps::Root root(tmp / "plain");
    auto rs = ps::parse_requests(
        R"({"schema":1,"requests":[{"id":"r1","generation":37,"container":"driver_career_progressive","replace":true},{"id":"r2","generation":3,"container":"x"}]})");
    expect(rs.size() == 2 && rs[0].id == "r1" && rs[0].generation == 37 && rs[0].replace && !rs[1].replace,
           "inject: requests parse");
    expect(ps::parse_requests("not json").empty() && ps::parse_requests(R"({"schema":2,"requests":[]})").empty(),
           "inject: bad files give no requests");

    using ps::Held;
    ps::Request q{"r1", "driver_career_progressive", 37, true};
    auto d = ps::decide(q, Held::none, true);
    expect(d.action == ps::Action::add && d.result.ok, "inject: an absent container is added");
    d = ps::decide(q, Held::kept, true);
    expect(d.action == ps::Action::replace && d.result.ok, "inject: replace swaps out a kept container");
    d = ps::decide(q, Held::registered, true);
    expect(d.action == ps::Action::replace && d.result.ok, "inject: replace also swaps a registered mode's container");
    q.replace = false;
    d = ps::decide(q, Held::kept, true);
    expect(d.action == ps::Action::none && !d.result.ok, "inject: a held container without replace is left alone");
    d = ps::decide(q, Held::none, false);
    expect(d.action == ps::Action::none && !d.result.ok && d.result.detail.find("no intact copy") != std::string::npos,
           "inject: a missing or damaged source fails");
    q.container = std::string(70, 'x');
    expect(!ps::decide(q, Held::none, true).result.ok, "inject: an invalid name fails");

    auto based = ps::parse_requests(
        R"({"schema":1,"requests":[{"id":"r3","generation":9,"container":"gm_driver_career_1990","replace":true,"base_sha256":"abc"}]})");
    expect(based.size() == 1 && based[0].base_sha256 == "abc",
           "inject: a request carries the hash of the container it was made from");
    expect(rs[0].base_sha256.empty(), "inject: a request without a base has none");
    expect(ps::base_matches(based[0], "abc"), "inject: a loaded container with the base hash matches");
    expect(!ps::base_matches(based[0], "abd") && !ps::base_matches(based[0], ""),
           "inject: a changed or unhashed loaded container does not match a base");
    expect(ps::base_matches(rs[0], "anything") && ps::base_matches(rs[0], ""), "inject: no base takes any loaded container");
    expect(ps::changed_since_detail(based[0]) == "the loaded gm_driver_career_1990 changed since generation 9; not replaced",
           "inject: the refusal says the loaded container changed since the generation");

    fs::create_directories(root.path());
    runtime::durable::write_file(root.path() / ps::INJECT, std::string("{\"schema\":1,\"requests\":[]}"));
    expect(ps::take_requests(root).taken, "inject: the load takes inject.json");
    expect(ps::answer(root, {{"r1", true, "ok \"quoted\""}, {"r2", false, "no"}}), "inject: answers are written");
    expect(!fs::exists(root.path() / ps::INJECT) && !fs::exists(root.path() / ps::INJECT_APPLYING),
           "inject: the taken requests are gone after the answer");
    json::Value doc;
    expect(json::parse(ps::read_text(root.path() / ps::INJECTED), doc) && doc["results"].a.size() == 2 &&
               doc["results"].a[0]["ok"].b && !doc["results"].a[1]["ok"].b && doc["results"].a[0]["detail"].s == "ok \"quoted\"",
           "inject: injected.json holds each result");
}

static const char *TWO_HALVES =
    R"({"schema":1,"requests":[{"id":"r1-gm","generation":3,"container":"gm_my_team_career_26","replace":true},)"
    R"({"id":"r1-slots","generation":3,"container":"my_team_career_26_SaveSlotGameModeComponent","replace":true}]})";

static void one_shot(const fs::path &tmp)
{
    fs::remove_all(tmp);
    ps::Root root(tmp / "plain");
    fs::create_directories(root.path());
    expect(!ps::take_requests(root).taken, "one shot: no inject.json, nothing taken");
    runtime::durable::write_file(root.path() / ps::INJECT, std::string(TWO_HALVES));
    ps::Taken t = ps::take_requests(root);
    expect(t.taken && !t.left_over && t.requests.size() == 2 &&
               t.requests[1].container == "my_team_career_26_SaveSlotGameModeComponent",
           "one shot: the load takes both requests");
    expect(!fs::exists(root.path() / ps::INJECT) && fs::exists(root.path() / ps::INJECT_APPLYING),
           "one shot: inject.json becomes inject.applying.json before anything is applied");

    t = ps::take_requests(root);
    expect(!t.taken && t.left_over && t.requests.empty(),
           "one shot: after a load that stopped before its answer, the next load applies nothing again");
    expect(fs::exists(root.path() / ps::INJECT_APPLYING), "one shot: the leftover stays for the launcher to answer");

    runtime::durable::write_file(root.path() / ps::INJECT, std::string(TWO_HALVES));
    t = ps::take_requests(root);
    expect(t.taken && t.left_over && t.requests.size() == 2, "one shot: a new inject.json is taken over a leftover");
    runtime::durable::write_file(root.path() / ps::INJECT, std::string("{\"schema\":1,\"requests\":[]}"));
    expect(ps::answer(root, {{"r1-gm", true, "replaced"}, {"r1-slots", true, "replaced"}}), "one shot: the load answers");
    expect(!fs::exists(root.path() / ps::INJECT_APPLYING) && fs::exists(root.path() / ps::INJECTED),
           "one shot: the answer removes inject.applying.json");
    expect(fs::exists(root.path() / ps::INJECT), "one shot: an inject.json written while the game ran stays for the next load");
}

static void swaps()
{
    auto rs = ps::parse_requests(
        R"({"schema":1,"requests":[{"id":"a","generation":3,"container":"gm_my_team_career_26","replace":true},)"
        R"({"id":"b","generation":3,"container":"my_team_career_26_SaveSlotGameModeComponent","replace":true},)"
        R"({"id":"c","generation":5,"container":"driver_career_progressive","replace":true},)"
        R"({"id":"d","generation":5,"container":"gm_driver_career_1990","replace":false},)"
        R"({"id":"e","generation":7,"container":"gm_f1_world","replace":true,"base_sha256":"abc"}]})");
    auto sw = ps::plan_swaps(rs);
    expect(sw.size() == 3 && sw[0].key == "my_team_career_26" && sw[0].requests == std::vector<size_t>{0, 1} &&
               sw[0].generation == 3 && sw[1].key == "driver_career_progressive" && sw[2].key == "f1_world",
           "swaps: replace requests group by mode; a request that is not a replace is no swap");
    expect(ps::swap_for(sw, "my_team_career_26_SaveSlotGameModeComponent") == &sw[0] &&
               ps::swap_for(sw, "gm_my_team_career_26") == &sw[0] && !ps::swap_for(sw, "gm_driver_career_1990"),
           "swaps: either half finds its mode's swap");
    expect(ps::swap_refusal(rs, sw[0], "gm_my_team_career_26", "").empty(),
           "swaps: a restore without a base takes the loaded half");
    expect(ps::swap_refusal(rs, sw[2], "gm_f1_world", "abd") ==
                   "the loaded gm_f1_world changed since generation 7; not replaced" &&
               ps::swap_refusal(rs, sw[2], "gm_f1_world", "").size() && ps::swap_refusal(rs, sw[2], "gm_f1_world", "abc").empty(),
           "swaps: a based request refuses a loaded half that changed or was not captured");
    expect(ps::swap_refusal(rs, sw[2], "f1_world_SaveSlotGameModeComponent", "zzz").empty(),
           "swaps: a half no request names has no base to check");

    ps::Swap s = sw[0];
    s.halves = {"gm_my_team_career_26", "my_team_career_26_SaveSlotGameModeComponent"};
    s.state = ps::Swap::State::applying;
    s.read = {"gm_my_team_career_26", "my_team_career_26_SaveSlotGameModeComponent"};
    expect(ps::swap_absent(s).empty(), "swaps: halves the game read from our bag are not read again after the load");
    s.read = {"gm_my_team_career_26"};
    expect(ps::swap_absent(s) == std::vector<std::string>{"my_team_career_26_SaveSlotGameModeComponent"},
           "swaps: only a half the file lacked is read after the load");
    s.state = ps::Swap::State::refused;
    expect(ps::swap_absent(s).empty(), "swaps: a refused mode puts nothing back");

    ps::ReadOnce once;
    expect(once.may_read(0x1000) && !once.may_read(0), "read once: an object no read reached may be read");
    once.mark(0x1000);
    expect(!once.may_read(0x1000) && once.may_read(0x2000), "read once: an object the game read is not read again");
}

// The table crash-20261120-172027 faulted in (F1_25.exe+0xbecf60): 26 entries of 28 bytes and a u64 count right after them,
// appended to without a clear or a bound. The dump held 24 entries and the second read's first three at 24, 25 and 26.
struct Entry28 {
    uint32_t key, slot, index;
    float a, b;
    uint32_t zero;
    uint8_t c, d, pad[2];
};
static_assert(sizeof(Entry28) == 28);
constexpr size_t TABLE_COUNT = 0x2d8;

struct ModeTable {
    std::array<uint8_t, TABLE_COUNT + 0x40> mem{};
    uint64_t count() const
    {
        uint64_t n;
        memcpy(&n, mem.data() + TABLE_COUNT, 8);
        return n;
    }
    bool read(const std::vector<Entry28> &entries, uint64_t &fault)
    {
        for (const Entry28 &e : entries) {
            uint64_t at = count() * sizeof(Entry28);
            if (at > mem.size() - sizeof(Entry28))
                return fault = at, false;
            memcpy(mem.data() + at, &e, sizeof e);
            uint64_t n = count() + 1;
            memcpy(mem.data() + TABLE_COUNT, &n, 8);
        }
        return true;
    }
};

static std::vector<Entry28> dump_entries()
{
    std::vector<Entry28> v = {{0x003b0000, 0xc, 0, 104.556f, 1049.38f, 0, 3, 2, {}},
                              {0x000f0000, 0x14, 1, 99.95f, 1051.28f, 0, 3, 2, {}},
                              {0x000e0000, 0x15, 2, 98.28f, 1054.39f, 0, 3, 2, {}},
                              {0x00000000, 0xb, 0, 105.11f, 1055.4f, 0, 3, 2, {}}};
    for (uint32_t i = 4; i < 24; i++)
        v.push_back({i << 16, i, i, 100.f, 1000.f, 0, 3, 2, {}});
    return v;
}

static void table_model()
{
    std::vector<Entry28> loaded = dump_entries(), ours = dump_entries();
    uint64_t fault = 0;
    ModeTable twice;
    bool first = twice.read(loaded, fault), second = twice.read(ours, fault);
    expect(first && !second && fault == 0x24c0188001cull,
           "model: a second read of the loaded mode faults at the dump's offset (count 0x15000e0001 x 28)");
    Entry28 at24;
    memcpy(&at24, twice.mem.data() + 24 * sizeof(Entry28), sizeof at24);
    expect(at24.key == ours[0].key && at24.slot == ours[0].slot,
           "model: the second read's first entry lands at 24, as in the dump");

    auto rs = ps::parse_requests(TWO_HALVES);
    auto sw = ps::plan_swaps(rs);
    ModeTable once;
    ps::ReadOnce reads;
    uintptr_t mode = 0x342ec7840;
    ps::Swap *s = ps::swap_for(sw, "gm_my_team_career_26");
    s->state = ps::Swap::State::applying;
    s->halves = {"gm_my_team_career_26", "my_team_career_26_SaveSlotGameModeComponent"};
    s->read = {"gm_my_team_career_26", "my_team_career_26_SaveSlotGameModeComponent"};
    bool ok = once.read(ours, fault);
    reads.mark(mode);
    expect(ok && once.count() == ours.size() && ps::swap_absent(*s).empty() && !reads.may_read(mode),
           "model: our bag read in place of the loaded one fills the table once and nothing reads it again");
}

int main(int argc, char **argv)
{
    fs::path tmp = argc > 1 ? fs::path(argv[1]) : fs::temp_directory_path() / "plain_saves_test";
    sha();
    settings();
    generations(tmp / "gen");
    putback(tmp / "put");
    pairs(tmp / "pair");
    requests(tmp / "req");
    one_shot(tmp / "once");
    swaps();
    table_model();
    fs::remove_all(tmp);
    printf("%s (%d failure(s))\n", fails ? "SOME FAILED" : "ALL PASSED", fails);
    return fails ? 1 : 0;
}
