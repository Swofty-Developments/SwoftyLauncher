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


    constexpr uint64_t MGR = 0xa10000, MAP = 0xa20000, ITEMS = 0xa30000, CTRL_END = 0xa31000, VT = 0xa40000, KEY = 0x1460bed80,
                       NS2 = 0xa50000, SYS = 0xa60000, MODES = 0xa70000, ENTRIES = 0xa80000, ENTRY = 0xa90000,
                       HOLDER = 0xaa0000, ENTITY = 0xab0000, MODE = 0xac0000;
    struct Scan {
        Image *image;
        uint64_t calls = 0;
    };
    auto services = [&]() {
        Image w;
        w.set<uint64_t>(MGR + room::MGR_SERVICES, MAP);
        w.set<uint64_t>(MAP + room::MAP_ITEMS, ITEMS);
        w.set<uint64_t>(MAP + room::MAP_END, ITEMS + 2 * room::MAP_ENTRY_SIZE);
        w.set<uint64_t>(ITEMS, 0x1234);
        w.set<uint64_t>(ITEMS + 8, 0x5678);
        w.set<uint64_t>(ITEMS + room::MAP_ENTRY_SIZE, KEY);
        w.set<uint64_t>(ITEMS + room::MAP_ENTRY_SIZE + 8, NS2);
        w.set<uint64_t>(NS2, VT);
        w.set<uint64_t>(VT, 0xb00000);
        w.set<uint64_t>(NS2 + room::NS_NET_SYSTEM, SYS);
        w.set<uint8_t>(SYS + room::NET_SYSTEM_MODE, 0);
        w.set<uint64_t>(MGR + room::MGR_MODES, MODES);
        w.set<uint64_t>(MODES + room::MODES_CURRENT, 0xad0000);
        w.set<uint64_t>(MODES + room::MODES_ENTRIES, ENTRIES);
        w.set<uint64_t>(ENTRIES + 3 * 8, ENTRY);
        w.set<uint64_t>(ENTRY + room::ENTRY_HOLDER, HOLDER);
        w.set<uint64_t>(HOLDER + room::HOLDER_ENTITY, ENTITY);
        w.set<uint64_t>(ENTITY + room::ENTITY_MODE, MODE);
        return w;
    };
    static Image *scan_image;
    auto scan = [](void *, uint64_t map, uint64_t key) -> uint64_t {
        uint64_t items = scan_image->get<uint64_t>(map + room::MAP_ITEMS), end = scan_image->get<uint64_t>(map + room::MAP_END);
        uint64_t at = items;
        while (at != end) {
            if (scan_image->get<uint64_t>(at) == key)
                return (at - items) / room::MAP_ENTRY_SIZE;
            at += room::MAP_ENTRY_SIZE;
        }
        return (end - items) / room::MAP_ENTRY_SIZE;
    };
    Image sv = services();
    scan_image = &sv;
    uint64_t found = 0;
    expect(room::find_net_subsystem(sv.memory(), MGR, KEY, VT, scan, nullptr, found).empty() && found == NS2,
           "the services map lookup finds the network subsystem by its key");
    expect(room::find_net_subsystem(sv.memory(), MGR, 0x999, VT, scan, nullptr, found).find("no network subsystem") != std::string::npos &&
               found == 0,
           "a key the map lacks is named");
    expect(room::find_net_subsystem(sv.memory(), 0, KEY, VT, scan, nullptr, found).find("not created") != std::string::npos,
           "no game manager is named");
    sv.drop(MGR + room::MGR_SERVICES, 8);
    expect(room::find_net_subsystem(sv.memory(), MGR, KEY, VT, scan, nullptr, found).find("services map") != std::string::npos,
           "a manager without a services map is named");
    sv = services();
    sv.set<uint64_t>(NS2, 0xbad000);
    std::string refused = room::find_net_subsystem(sv.memory(), MGR, KEY, VT, scan, nullptr, found);
    expect(refused.find("not the network subsystem") != std::string::npos && refused.find("0xa50000") != std::string::npos &&
               refused.find("0xbad000") != std::string::npos && refused.find("0xa40000") != std::string::npos && found == 0,
           "an entry whose vtable is not the network subsystem's is refused, naming the pointer, the vtable read and the expected one");
    sv = services();
    expect(room::find_net_subsystem(sv.memory(), MGR, KEY, 0, scan, nullptr, found).find("not the network subsystem") !=
               std::string::npos,
           "an unresolved expected vtable refuses every entry");
    sv = services();
    sv.drop(NS2, 8);
    expect(room::find_net_subsystem(sv.memory(), MGR, KEY, VT, scan, nullptr, found).find("vtable is unreadable") != std::string::npos,
           "an entry that is not an object is refused");

    sv = services();
    room::ModePlan mp;
    expect(room::plan_mode_switch(sv.memory(), MGR, NS2, mp).empty() && !mp.already && mp.net == 0 && mp.modes == MODES,
           "the mode switch plan reads the modes context and the network mode");
    expect(room::mode_entity(sv.memory(), MODES, 3, mp).empty() && mp.entity == ENTITY && mp.mode == MODE,
           "the online_custom entry gives its entity and mode");
    expect(room::mode_entity(sv.memory(), MODES, room::MODE_INDEX_LIMIT, mp).find("no online_custom") != std::string::npos,
           "an index past the table is refused");
    expect(room::mode_entity(sv.memory(), MODES, 4, mp).find("no mode entity") != std::string::npos,
           "an entry past the filled ones is refused");
    sv.set<uint64_t>(ENTITY + room::ENTITY_MODE, 0);
    expect(room::mode_entity(sv.memory(), MODES, 3, mp).find("no mode at +0xf08") != std::string::npos, "an entity with no mode is refused");
    sv = services();
    sv.set<uint8_t>(SYS + room::NET_SYSTEM_MODE, 1);
    expect(room::plan_mode_switch(sv.memory(), MGR, NS2, mp).empty() && mp.already && room::net_mode(sv.memory(), NS2) == 1,
           "a subsystem already in LAN mode needs no switch");
    sv = services();
    sv.set<uint64_t>(NS2 + room::NS_NET_SYSTEM, 0);
    expect(room::net_mode(sv.memory(), NS2) == 0, "a null net system reads as not LAN");
    sv.drop(NS2 + room::NS_NET_SYSTEM, 8);
    expect(room::plan_mode_switch(sv.memory(), MGR, NS2, mp).find("not readable") != std::string::npos,
           "an unreadable net system stops the mode switch");
    sv = services();
    sv.drop(MGR + room::MGR_MODES, 8);
    expect(room::plan_mode_switch(sv.memory(), MGR, NS2, mp).find("game mode context") != std::string::npos,
           "a manager with no modes context stops the mode switch");
    sv = services();
    sv.drop(MODES + room::MODES_CURRENT, 8);
    expect(room::plan_mode_switch(sv.memory(), MGR, NS2, mp).empty() && mp.current == 0,
           "a modes context with no current mode yet still plans the mode switch");

    {
        room::Facts mf;
        room::Run mr;
        mr.start(room::Role::host, 0, false, true);
        mf.lan_mode = false;
        expect(mr.tick(mf, 0) == room::Do::mode_switch, "a mode-only run starts with the mode switch");
        mf.lan_mode = true;
        mr.tick(mf, 5);
        expect(mr.done(), "a mode-only host run is done once the mode reads LAN, with no CustomCreate");
    }

    {
        room::HostUi ui;
        room::UiFacts uf;
        ui.start(1000);
        expect(ui.tick(uf, 1000) == room::UiDo::write_state && !ui.released(1000), "the host writes the state first and is not released yet");
        expect(ui.released(1000 + room::UI_STATE_GRACE_MS), "the flow is released after the grace even when the write has not taken");
        expect(ui.tick(uf, 1100) == room::UiDo::write_state, "it keeps writing until it takes");
        ui.wrote(1200);
        expect(ui.written() && ui.released(1200), "a written state releases at once");
        expect(ui.tick(uf, 1300) == room::UiDo::nothing && ui.step == room::UiStep::container, "then it waits for the container");
        uf.container = true;
        expect(ui.tick(uf, 2000) == room::UiDo::nothing && ui.step == room::UiStep::settle, "a container starts the settle wait");
        expect(ui.tick(uf, 2000 + room::UI_SETTLE_MIN_MS + 1) == room::UiDo::nothing && ui.step == room::UiStep::settle,
               "it does not send before the tab list is ready");
        uf.ready = true;
        expect(ui.tick(uf, 2000 + room::UI_SETTLE_MIN_MS - 1) == room::UiDo::nothing, "a ready list is still given the settle minimum");
        expect(ui.tick(uf, 2000 + room::UI_SETTLE_MIN_MS) == room::UiDo::send_open && ui.sent(), "then open_create_custom is sent once");
        expect(ui.tick(uf, 99999) == room::UiDo::nothing && !ui.active() && !ui.failed(), "and the machine is over");

        room::HostUi lost;
        room::UiFacts lf;
        lost.start(0);
        lost.wrote(1);
        lf.container = true;
        lost.tick(lf, 2);
        lf.container = false;
        lost.tick(lf, 3);
        expect(lost.step == room::UiStep::container, "a container that closes before the send sends the machine back to waiting");

        room::HostUi no_state;
        no_state.start(0);
        no_state.tick(uf, room::UI_STATE_MS);
        expect(no_state.failed() && no_state.error.find("ui_menu_data") != std::string::npos, "a state that never takes is a named failure");

        room::HostUi no_container;
        room::UiFacts nf;
        no_container.start(0);
        no_container.wrote(0);
        no_container.tick(nf, room::UI_CONTAINER_MS);
        expect(no_container.failed() && no_container.error.find("container") != std::string::npos,
               "a container that never opens is a named failure");

        room::HostUi no_tabs;
        room::UiFacts tf;
        tf.container = true;
        no_tabs.start(0);
        no_tabs.wrote(0);
        no_tabs.tick(tf, 1);
        expect(no_tabs.tick(tf, 1 + room::UI_SETTLE_MS) == room::UiDo::send_open && no_tabs.sent() && no_tabs.sent_unready(),
               "a tab list that never reads as ready still gets open_create_custom once, marked unready");
    }

    {
        Image c;
        constexpr uint64_t CT = 0xc10000, TABS = 0xc20000;
        expect(!room::container_facts(c.memory(), CT).readable, "an unreadable container reads as not ready");
        c.set<uint64_t>(CT + room::CONTAINER_TABS_READY, TABS);
        c.set<uint8_t>(TABS + room::TABS_READY_BYTE, 1);
        c.set<uint8_t>(CT + room::CONTAINER_STATE_BYTE, 0);
        expect(room::container_facts(c.memory(), CT).ready, "the tab list byte makes the container ready");
        c.set<uint8_t>(CT + room::CONTAINER_STATE_BYTE, 5);
        expect(!room::container_facts(c.memory(), CT).ready, "state 5 makes it not ready");
        c.set<uint8_t>(CT + room::CONTAINER_STATE_BYTE, 0);
        c.set<uint8_t>(TABS + room::TABS_READY_BYTE, 0);
        expect(!room::container_facts(c.memory(), CT).ready, "a tab list byte of 0 makes it not ready");
    }

    printf(fails ? "netplay room test: FAILED\n" : "netplay room test: ALL PASSED\n");
    return fails ? 1 : 0;
}
