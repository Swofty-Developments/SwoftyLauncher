// Host unit test for the headless room set-up logic (netplay_room_logic.hpp): the host and join plans
// over a fake memory image, the bytes they write, and the step machine with its waits and fallback.
// run_events_test.sh builds and runs it.
#include "../src/core/netplay_room_logic.hpp"

#include <cstdio>
#include <map>
#include <string>

namespace room = runtime::netplay_room;

static int fails;

static void expect(bool ok, const std::string &what)
{
    printf("%s: %s\n", ok ? "ok  " : "FAIL", what.c_str());
    fails += !ok;
}

struct Image {
    std::map<uint64_t, uint8_t> bytes;

    template <class T> void set(uint64_t at, T v)
    {
        for (size_t i = 0; i < sizeof v; i++)
            bytes[at + i] = reinterpret_cast<const uint8_t *>(&v)[i];
    }

    void drop(uint64_t at, size_t n)
    {
        for (size_t i = 0; i < n; i++)
            bytes.erase(at + i);
    }

    static bool read(uint64_t at, void *out, size_t n, void *ctx)
    {
        auto *self = static_cast<Image *>(ctx);
        for (size_t i = 0; i < n; i++) {
            auto it = self->bytes.find(at + i);
            if (it == self->bytes.end())
                return false;
            static_cast<uint8_t *>(out)[i] = it->second;
        }
        return true;
    }

    room::Memory memory() { return {read, this}; }

    void apply(const std::vector<room::Write> &w)
    {
        for (const auto &x : w)
            for (size_t i = 0; i < x.bytes.size(); i++)
                bytes[x.address + i] = x.bytes[i];
    }

    template <class T> T get(uint64_t at)
    {
        T v{};
        read(at, &v, sizeof v, this);
        return v;
    }
};

constexpr uint64_t NS = 0x200000, GSUI = 0x300000, SETTINGS = 0x400000, INVITES = 0x500000, REQUEST = 0x600000,
                   CONFIG = 0x700000, OWNER = 0x800000, RECORD = 0x900000;

static Image world()
{
    Image m;
    m.set<uint64_t>(NS + room::NS_GSUI, GSUI);
    m.set<uint64_t>(GSUI + room::GSUI_NS, NS);
    m.set<uint64_t>(NS + room::NS_SETTINGS, SETTINGS);
    m.set<uint64_t>(NS + room::NS_INVITES, INVITES);
    m.set<uint64_t>(NS + room::NS_REQUEST, REQUEST);
    for (uint64_t off : room::REQUEST_BUSY)
        m.set<uint64_t>(REQUEST + off, 0);
    m.set<uint8_t>(REQUEST + room::REQUEST_BUSY_BYTE, 0);
    for (uint64_t off : room::GSUI_SELECTORS)
        m.set<uint64_t>(GSUI + off, 0);
    m.set<uint16_t>(NS + room::NS_PRESET, 0);
    m.set<uint64_t>(SETTINGS + room::SETTINGS_CLUSTER, 7);
    m.set<uint8_t>(SETTINGS + room::SETTINGS_CLUSTER_SET, 0);
    m.set<uint64_t>(NS + room::NS_SOCKET_CONFIG, CONFIG);
    m.set<uint32_t>(CONFIG + room::SOCKET_CONFIG_PORT, 21414);
    m.set<uint64_t>(OWNER + room::OWNER_REQUEST, RECORD);
    m.set<uint8_t>(RECORD + room::REQ_PENDING, 0);
    m.set<uint32_t>(INVITES + room::INVITES_TYPE, 0);
    m.set<uint64_t>(INVITES + room::INVITES_DELEGATE, 0);
    return m;
}

int main()
{
    room::HostPlan hp;
    Image m = world();
    expect(room::plan_host(m.memory(), NS, room::stock_2025(), hp).empty(), "a clean image gives a host plan");
    expect(hp.gsui == GSUI && hp.settings == SETTINGS && hp.invites == INVITES, "the plan names gsui, settings and invites");
    m.apply(hp.writes);
    expect(m.get<uint16_t>(NS + room::NS_PRESET) == 0x100, "F1 2025 writes DLC domain 0x100");
    expect(m.get<uint64_t>(SETTINGS + room::SETTINGS_CLUSTER) == 0xffffffffull, "the cluster id becomes -1 with the high word 0");
    expect(m.get<uint8_t>(SETTINGS + room::SETTINGS_CLUSTER_SET) == 1, "the cluster flag is set");
    expect(hp.writes.size() == 3, "a host plan is three writes");

    m = world();
    room::plan_host(m.memory(), NS, room::stock_2026(), hp);
    m.apply(hp.writes);
    expect(m.get<uint16_t>(NS + room::NS_PRESET) == 0x101, "F1 2026 writes DLC domain 0x101");
    room::plan_host(m.memory(), NS, room::addon("cs_f1_2016"), hp);
    expect(hp.writes.size() == 3 && hp.writes[0].bytes[0] == 0x00 && hp.writes[0].bytes[1] == 0x01,
           "an addon category takes the F1 2025 domain (0x100, little endian 00 01)");
    expect(!room::plan_host(m.memory(), NS, room::Category{0x102, {}}, hp).empty(), "an unknown domain is refused");

    m = world();
    m.drop(NS + room::NS_GSUI, 8);
    expect(room::plan_host(m.memory(), NS, room::stock_2025(), hp).find("GameSessionUI") != std::string::npos,
           "an unreadable gsui pointer is named");
    m = world();
    m.set<uint64_t>(GSUI + room::GSUI_NS, NS + 8);
    expect(room::plan_host(m.memory(), NS, room::stock_2025(), hp).find("point back") != std::string::npos,
           "a gsui that does not point back is refused");
    m = world();
    m.set<uint64_t>(NS + room::NS_GSUI, 0);
    expect(!room::plan_host(m.memory(), NS, room::stock_2025(), hp).empty(), "a null gsui is refused");
    m = world();
    m.drop(NS + room::NS_SETTINGS, 8);
    expect(room::plan_host(m.memory(), NS, room::stock_2025(), hp).find("session settings") != std::string::npos,
           "missing session settings are named");
    m = world();
    m.set<uint64_t>(REQUEST + 0xf8, 1);
    expect(room::plan_host(m.memory(), NS, room::stock_2025(), hp).find("already pending") != std::string::npos,
           "a pending session request stops the host");
    m = world();
    m.set<uint8_t>(REQUEST + room::REQUEST_BUSY_BYTE, 1);
    expect(room::plan_host(m.memory(), NS, room::stock_2025(), hp).find("+0x158") != std::string::npos,
           "the request busy byte stops the host");
    m = world();
    m.drop(REQUEST + 0xb0, 8);
    expect(room::plan_host(m.memory(), NS, room::stock_2025(), hp).find("not readable") != std::string::npos,
           "an unreadable request field is named");
    m = world();
    m.set<uint64_t>(GSUI + room::GSUI_SELECTORS[1], 0xa00000);
    m.set<uint64_t>(0xa00000 + room::SELECTOR_HOLDER, 0xb00000);
    m.set<uint64_t>(0xb00000 + room::HOLDER_FLAG, 0xc00000);
    m.set<uint8_t>(0xc00000, 1);
    expect(room::plan_host(m.memory(), NS, room::stock_2025(), hp).find("selector") != std::string::npos,
           "an active Car Category selector stops the host");
    m.set<uint8_t>(0xc00000, 0);
    expect(room::plan_host(m.memory(), NS, room::stock_2025(), hp).empty(), "an idle selector does not");
    m.drop(0xb00000 + room::HOLDER_FLAG, 8);
    expect(room::plan_host(m.memory(), NS, room::stock_2025(), hp).empty(), "a selector chain that ends early is idle, as in the game");

    uint32_t ip = 0;
    expect(room::parse_ipv4("10.77.0.1", ip) && ip == 0x01004d0au, "10.77.0.1 reads as 0x01004d0a, as the game stores it");
    expect(!room::parse_ipv4("10.77.0", ip) && !room::parse_ipv4("1.2.3.4.5", ip) && !room::parse_ipv4("1.2.3.256", ip) &&
               !room::parse_ipv4("a.b.c.d", ip) && !room::parse_ipv4("1.2.3.4x", ip) && !room::parse_ipv4("", ip),
           "malformed addresses do not parse");
    expect(room::connection_id(0x01004d0a, 21414) == 0x52a61eac, "the connection id matches the one the trace observed");

    room::JoinPlan jp;
    m = world();
    expect(room::plan_join(m.memory(), NS, OWNER, "10.77.0.1", jp).empty(), "a clean image gives a join plan");
    expect(jp.port == 21414 && jp.ip == 0x01004d0a && jp.request == RECORD && jp.gsui == GSUI, "the plan holds the port, address and record");
    expect(jp.writes.back().address == RECORD + room::REQ_PENDING && jp.writes.back().bytes[0] == 1,
           "the pending flag is the last write");
    m.apply(jp.writes);
    expect(m.get<uint8_t>(RECORD + 8) == 1 && m.get<uint32_t>(RECORD + 0x10) == 2, "pending 1 and kind 2 (LAN)");
    expect(m.get<uint32_t>(RECORD + 0x30) == 0x01004d0a && m.get<uint16_t>(RECORD + 0x34) == 21414, "address and port at +0x30");
    expect(m.get<uint32_t>(RECORD + 0x38) == 0x52a61eac, "connection id at +0x38");
    expect(m.get<uint64_t>(RECORD + 0x20) == 0 && m.get<uint64_t>(RECORD + 0x28) == 0 && m.get<uint8_t>(RECORD + 0x3d) == 0 &&
               m.get<uint8_t>(RECORD + 0x40) == 0,
           "the rest of the record is zero");
    expect(room::plan_join(m.memory(), NS, OWNER, "10.77.0.1", jp).find("already pending") != std::string::npos,
           "a record that is still pending stops a second join");

    m = world();
    expect(!room::plan_join(m.memory(), NS, OWNER, "10.77.0", jp).empty(), "a bad address is refused");
    expect(!room::plan_join(m.memory(), NS, OWNER, "0.0.0.0", jp).empty(), "the any address is refused");
    m.set<uint32_t>(CONFIG + room::SOCKET_CONFIG_PORT, 0);
    expect(room::plan_join(m.memory(), NS, OWNER, "10.77.0.1", jp).find("reads 0") != std::string::npos, "port 0 is refused");
    m = world();
    m.drop(NS + room::NS_SOCKET_CONFIG, 8);
    expect(room::plan_join(m.memory(), NS, OWNER, "10.77.0.1", jp).find("port is not readable") != std::string::npos,
           "an unreadable socket config is named");
    m = world();
    m.drop(OWNER + room::OWNER_REQUEST, 8);
    expect(room::plan_join(m.memory(), NS, OWNER, "10.77.0.1", jp).find("no request record") != std::string::npos,
           "an owner without a record is named");
    m = world();
    m.drop(RECORD, 0x40);
    expect(room::plan_join(m.memory(), NS, OWNER, "10.77.0.1", jp).find("not readable") != std::string::npos,
           "an unreadable record is named");

    room::Facts f;
    f.home = true;
    f.lan_mode = false;
    room::Run run;
    run.start(room::Role::host, 1000);
    expect(run.tick(f, 1000) == room::Do::mode_switch, "the run starts with the mode switch");
    expect(run.tick(f, 1100) == room::Do::nothing && run.active(), "it waits for LAN mode");
    f.lan_mode = true;
    expect(run.tick(f, 1200) == room::Do::nothing, "LAN mode seen");
    expect(run.tick(f, 1300) == room::Do::host_call, "a host then calls CustomCreate");
    f.home = false;
    f.invite = {true, room::INVITE_CREATE_CUSTOM, 0xdead};
    expect(run.tick(f, 1400) == room::Do::nothing && run.stage == room::Stage::lobby,
           "a join delegate on the invite manager means the interrupt worked");
    f.lobby = true;
    expect(run.tick(f, 5000) == room::Do::nothing && run.done() && !run.by_hand, "the lobby ends the run without the by-hand path");

    run = {};
    f = {};
    f.home = true;
    f.lan_mode = true;
    run.start(room::Role::host, 0);
    run.tick(f, 0);
    run.tick(f, 10);
    expect(run.tick(f, 20) == room::Do::host_call, "second host: call");
    f.invite = {true, room::INVITE_CREATE_CUSTOM, 0};
    expect(run.tick(f, 20 + room::INTERRUPT_MS - 1) == room::Do::nothing, "no by-hand request before the wait is over");
    expect(run.tick(f, 20 + room::INTERRUPT_MS) == room::Do::fallback_notify && run.by_hand,
           "an invite without a delegate after the wait is requested by hand");
    f.lobby = true;
    run.tick(f, 100000);
    expect(run.done() && run.by_hand, "and the lobby ends that run");

    run = {};
    f = {};
    f.home = true;
    f.lan_mode = true;
    run.start(room::Role::host, 0);
    run.tick(f, 0);
    run.tick(f, 10);
    run.tick(f, 20);
    f.invite = {true, 0, 0};
    run.tick(f, 20 + room::INTERRUPT_MS);
    expect(run.failed() && run.error.find("never reached") != std::string::npos, "no invite after the wait is a named failure");

    run = {};
    f = {};
    f.home = true;
    run.start(room::Role::host, 0);
    run.tick(f, 0);
    run.tick(f, room::MODE_MS);
    expect(run.failed() && run.error.find("LAN mode") != std::string::npos, "a mode switch that does not take times out");

    run = {};
    f = {};
    f.home = true;
    f.lan_mode = true;
    run.start(room::Role::host, 0);
    run.tick(f, 0);
    run.tick(f, 1);
    run.tick(f, 2);
    f.invite = {true, room::INVITE_CREATE_CUSTOM, 1};
    run.tick(f, 3);
    run.tick(f, 10 + room::HOST_LOBBY_MS);
    expect(run.failed() && run.error.find("session was not created") != std::string::npos, "a session that never comes up times out");

    run = {};
    f = {};
    f.home = true;
    f.lan_mode = true;
    run.start(room::Role::host, 0);
    run.tick(f, 0);
    run.tick(f, 1);
    run.tick(f, 2);
    f.home = false;
    run.tick(f, 3);
    f.home = true;
    f.invite = {true, room::INVITE_CREATE_CUSTOM, 0};
    run.tick(f, 4);
    expect(run.failed() && run.error.find("main menu") != std::string::npos, "coming back to the main menu before the lobby fails the run");

    run = {};
    f = {};
    f.home = true;
    f.lan_mode = true;
    run.start(room::Role::join, 0);
    run.tick(f, 0);
    run.tick(f, 1);
    expect(run.tick(f, 2) == room::Do::join_write, "a join writes the request");
    expect(run.tick(f, 3) == room::Do::nothing && run.active(), "it waits for the game to take the request");
    f.pending_taken = true;
    f.invite = {true, room::INVITE_SESSION, 0};
    run.tick(f, 100);
    expect(run.tick(f, 100 + room::INTERRUPT_MS) == room::Do::fallback_notify, "a taken request with no delegate is requested by hand after the wait");

    run = {};
    f = {};
    f.home = true;
    f.lan_mode = true;
    run.start(room::Role::join, 0);
    run.tick(f, 0);
    run.tick(f, 1);
    run.tick(f, 2);
    run.tick(f, 2 + room::PENDING_MS);
    expect(run.failed() && run.error.find("did not take") != std::string::npos, "a request the game never takes is a named failure");

    m = world();
    expect(room::session_role(m.memory(), NS) == -1, "an unreadable session reads as -1");
    m.set<uint64_t>(NS + room::NS_SESSION, 0xd00000);
    m.set<int32_t>(0xd00000 + room::SESSION_ROLE, 0);
    expect(room::session_role(m.memory(), NS) == 0, "an offline session reads 0");
    m.set<int32_t>(0xd00000 + room::SESSION_ROLE, 1);
    expect(room::session_role(m.memory(), NS) == room::established_role(room::Role::host), "a host session reads 1");

    run = {};
    f = {};
    f.lan_mode = true;
    run.start(room::Role::join, 0, false);
    run.tick(f, 0);
    run.tick(f, 1);
    expect(run.tick(f, 2) == room::Do::join_write, "a run before home writes the request with no home");
    f.pending_taken = true;
    f.invite = {true, room::INVITE_SESSION, 0xbeef};
    run.tick(f, 3);
    run.tick(f, 4);
    expect(run.stage == room::Stage::lobby && run.active(), "no home is not read as leaving home or failing");
    f.session_role = 2;
    run.tick(f, 5);
    expect(run.done() && !run.by_hand, "the client session role finishes a join with no lobby state");
    run = {};
    f = {};
    f.lan_mode = true;
    run.start(room::Role::host, 0, false);
    run.tick(f, 0);
    run.tick(f, 1);
    run.tick(f, 2);
    f.session_role = 2;
    run.tick(f, 3);
    expect(!run.done(), "a client role does not finish a host");
    f.session_role = 1;
    run.tick(f, 4);
    expect(run.done(), "the host session role finishes a host");

    printf(fails ? "netplay room test: FAILED\n" : "netplay room test: ALL PASSED\n");
    return fails ? 1 : 0;
}
