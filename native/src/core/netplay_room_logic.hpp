#pragma once
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <string>
#include <vector>

// The game-free half of the headless room set-up (netplay_room.cpp): which game objects the host and the
// join read, what they check before touching them, and the bytes they write. Memory is reached through a
// reader the caller supplies, so the host test feeds it a fake image. Every pointer hop is read through
// it, so an unreadable hop is a named failure instead of a fault.
//
// Host: the Car Category confirm (FUN_141a3f7f0, button_1 or button_2) writes the DLC domain
// (*(u16*)(ns+0x4f4)) and the cluster id (session settings +0xadc = 0xffffffff, +0xae4 = 1), then fires a
// route that ends in GameSessionUI's CustomCreate (0x1430ae3f0, this = gsui). Set up headless it writes
// the same three values and calls CustomCreate itself.
//
// Join: the Join via IP prompt (FUN_14328deb0) fills the pending request record and the per-frame pump
// (0x1443dca90) hands it to the invite manager. Set up headless it fills the same record.

namespace runtime::netplay_room {

constexpr uint16_t PRESET_2025 = 0x100;
constexpr uint16_t PRESET_2026 = 0x101;

constexpr uint64_t NS_NET_SYSTEM = 0xd8;
constexpr uint64_t NS_SOCKET_CONFIG = 0x3c8;
constexpr uint64_t NS_SETTINGS = 0x3e8;
constexpr uint64_t NS_INVITES = 0x3b8;
constexpr uint64_t NS_REQUEST = 0x3f0;
constexpr uint64_t NS_GSUI = 0x460;
constexpr uint64_t NS_PRESET = 0x4f4;
constexpr uint64_t NS_SESSION = 0x418;
constexpr uint64_t SESSION_ROLE = 8;
constexpr uint64_t SOCKET_CONFIG_PORT = 0x30;
constexpr uint64_t NET_SYSTEM_MODE = 0x750;

constexpr uint64_t SETTINGS_CLUSTER = 0xadc;
constexpr uint64_t SETTINGS_CLUSTER_SET = 0xae4;
constexpr uint32_t CLUSTER_NONE = 0xffffffffu;

constexpr uint64_t GSUI_NS = 8;
constexpr uint64_t GSUI_SELECTORS[4] = {0x380, 0x390, 0x3a0, 0x3b0};
constexpr uint64_t SELECTOR_HOLDER = 0x10;
constexpr uint64_t HOLDER_FLAG = 0x38;

constexpr uint64_t REQUEST_BUSY[4] = {0xa8, 0xb0, 0xf8, 0x100};
constexpr uint64_t REQUEST_BUSY_BYTE = 0x158;

constexpr uint64_t INVITES_TYPE = 0x18;
constexpr uint64_t INVITES_DELEGATE = 0xb78;
constexpr uint32_t INVITE_CREATE_CUSTOM = 3;
constexpr uint32_t INVITE_SESSION = 10;

constexpr uint64_t OWNER_REQUEST = 0xc0;
constexpr uint64_t REQ_PENDING = 0x8;
constexpr uint64_t REQ_KIND = 0x10;
constexpr uint64_t REQ_CLEAR_A = 0x20;
constexpr uint64_t REQ_CLEAR_B = 0x28;
constexpr uint64_t REQ_CLEAR_C = 0x2d;
constexpr uint64_t REQ_CLEAR_D = 0x2f;
constexpr uint64_t REQ_ADDRESS = 0x30;
constexpr uint64_t REQ_CONN = 0x38;
constexpr uint64_t REQ_TAIL_A = 0x3c;
constexpr uint64_t REQ_TAIL_B = 0x3d;
constexpr uint64_t REQ_TAIL_C = 0x40;
constexpr uint32_t REQUEST_LAN = 2;

enum class Role { host, join };

inline std::string hex(uint64_t v)
{
    char b[24];
    std::snprintf(b, sizeof b, "0x%llx", static_cast<unsigned long long>(v));
    return b;
}

inline const char *role_name(Role r)
{
    return r == Role::host ? "host" : "join";
}

using Read = bool (*)(uint64_t address, void *out, size_t n, void *ctx);

struct Memory {
    Read read;
    void *ctx;

    template <class T> bool get(uint64_t address, T &out) const { return read(address, &out, sizeof out, ctx); }

    bool ptr(uint64_t address, uint64_t &out) const
    {
        return get(address, out) && out >= 0x10000 && out < 0x0000800000000000ull;
    }
};

struct Write {
    uint64_t address = 0;
    std::vector<uint8_t> bytes;
};

template <class T> Write put(uint64_t address, T value)
{
    Write w;
    w.address = address;
    w.bytes.resize(sizeof value);
    std::memcpy(w.bytes.data(), &value, sizeof value);
    return w;
}

// What the Car Category writes for each box. An addon category runs the F1 2025 box's path
// (car_category.cpp press_entry), so it takes 0x100 and names its category option value on its own.
struct Category {
    uint16_t preset = PRESET_2025;
    std::string addon;
};

inline Category stock_2025() { return {PRESET_2025, {}}; }
inline Category stock_2026() { return {PRESET_2026, {}}; }
inline Category addon(const std::string &value) { return {PRESET_2025, value}; }

inline std::string find_gsui(const Memory &m, uint64_t ns, uint64_t &gsui)
{
    uint64_t back = 0;
    if (!m.ptr(ns + NS_GSUI, gsui))
        return "the network subsystem has no GameSessionUI at +0x460";
    if (!m.ptr(gsui + GSUI_NS, back) || back != ns)
        return "the GameSessionUI at +0x460 does not point back at the network subsystem";
    return {};
}

struct HostPlan {
    uint64_t ns = 0, gsui = 0, settings = 0, invites = 0;
    std::vector<Write> writes;
};

// The gsui path, the checks CustomCreate and the Car Category handler make on the way, and the writes the
// handler makes. ns is the network subsystem (the private LAN item's +0x150). Empty on success, else why
// the headless host cannot start.
inline std::string plan_host(const Memory &m, uint64_t ns, const Category &c, HostPlan &out)
{
    out = {};
    out.ns = ns;
    if (c.preset != PRESET_2025 && c.preset != PRESET_2026)
        return "the Car Category value " + hex(c.preset) + " is not a DLC domain the game sets";
    uint64_t gsui = 0, settings = 0, invites = 0, request = 0;
    if (std::string why = find_gsui(m, ns, gsui); !why.empty())
        return why;
    if (!m.ptr(ns + NS_SETTINGS, settings))
        return "the network subsystem has no session settings at +0x3e8";
    if (!m.ptr(ns + NS_INVITES, invites))
        return "the network subsystem has no invite manager at +0x3b8";
    if (!m.ptr(ns + NS_REQUEST, request))
        return "the network subsystem has no session request at +0x3f0";
    for (uint64_t off : REQUEST_BUSY) {
        uint64_t q = 0;
        if (!m.get(request + off, q))
            return "the session request is not readable";
        if (q)
            return "a session request is already pending (+" + hex(off) + ")";
    }
    uint8_t flag = 0;
    if (!m.get(request + REQUEST_BUSY_BYTE, flag))
        return "the session request is not readable";
    if (flag)
        return "a session request is already pending (+0x158)";
    for (uint64_t off : GSUI_SELECTORS) {
        uint64_t sel = 0;
        if (!m.get(gsui + off, sel))
            return "the GameSessionUI is not readable";
        if (!sel)
            continue;
        uint64_t holder = 0, byte_at = 0;
        if (!m.ptr(sel + SELECTOR_HOLDER, holder) || !m.get(holder + HOLDER_FLAG, byte_at))
            continue;
        uint8_t active = 0;
        if (byte_at && m.get(byte_at, active) && active)
            return "a Car Category selector is already active";
    }
    out.gsui = gsui;
    out.settings = settings;
    out.invites = invites;
    out.writes.push_back(put<uint16_t>(ns + NS_PRESET, c.preset));
    out.writes.push_back(put<uint64_t>(settings + SETTINGS_CLUSTER, CLUSTER_NONE));
    out.writes.push_back(put<uint8_t>(settings + SETTINGS_CLUSTER_SET, 1));
    return {};
}

// 10.77.0.1 as inet_addr returns it: the four parts in memory order, so a little-endian load gives
// 0x01004d0a. False for anything that is not four decimal parts of 0 to 255.
inline bool parse_ipv4(const std::string &s, uint32_t &out)
{
    unsigned part[4] = {};
    int n = 0;
    size_t i = 0;
    while (n < 4) {
        size_t start = i;
        unsigned v = 0;
        while (i < s.size() && s[i] >= '0' && s[i] <= '9' && i - start < 3)
            v = v * 10 + unsigned(s[i++] - '0');
        if (i == start || v > 255)
            return false;
        part[n++] = v;
        if (n < 4) {
            if (i >= s.size() || s[i] != '.')
                return false;
            ++i;
        }
    }
    if (i != s.size())
        return false;
    out = part[0] | part[1] << 8 | part[2] << 16 | part[3] << 24;
    return true;
}

// The id the prompt gives the request: the port twice in one word, xor the address.
inline uint32_t connection_id(uint32_t ip, uint16_t port)
{
    return (uint32_t(port) << 16 | port) ^ ip;
}

struct JoinPlan {
    uint64_t ns = 0, gsui = 0, request = 0, invites = 0;
    uint32_t ip = 0;
    uint16_t port = 0;
    std::vector<Write> writes;
};

// The request record's fields as the prompt writes them. The pending flag goes last, since the pump runs
// on other threads and acts on it as soon as it reads 1.
inline std::vector<Write> join_request_writes(uint64_t request, uint32_t ip, uint16_t port)
{
    std::vector<Write> w;
    w.push_back(put<uint32_t>(request + REQ_KIND, REQUEST_LAN));
    w.push_back(put<uint64_t>(request + REQ_CLEAR_A, 0));
    w.push_back(put<uint64_t>(request + REQ_CLEAR_B, 0));
    w.push_back(put<uint16_t>(request + REQ_CLEAR_C, 0));
    w.push_back(put<uint8_t>(request + REQ_CLEAR_D, 0));
    Write address;
    address.address = request + REQ_ADDRESS;
    address.bytes.assign(8, 0);
    std::memcpy(address.bytes.data(), &ip, 4);
    std::memcpy(address.bytes.data() + 4, &port, 2);
    w.push_back(address);
    w.push_back(put<uint32_t>(request + REQ_CONN, connection_id(ip, port)));
    w.push_back(put<uint8_t>(request + REQ_TAIL_A, 0));
    w.push_back(put<uint8_t>(request + REQ_TAIL_B, 0));
    w.push_back(put<uint8_t>(request + REQ_TAIL_C, 0));
    w.push_back(put<uint8_t>(request + REQ_PENDING, 1));
    return w;
}

// owner is what the registry lookup of the request key returned. Empty on success.
inline std::string plan_join(const Memory &m, uint64_t ns, uint64_t owner, const std::string &host_ip, JoinPlan &out)
{
    out = {};
    out.ns = ns;
    if (!parse_ipv4(host_ip, out.ip) || out.ip == 0 || out.ip == 0xffffffffu)
        return "'" + host_ip + "' is not a dotted IPv4 address";
    uint64_t config = 0, request = 0, invites = 0;
    uint32_t port = 0;
    if (std::string why = find_gsui(m, ns, out.gsui); !why.empty())
        return why;
    if (!m.ptr(ns + NS_SOCKET_CONFIG, config) || !m.get(config + SOCKET_CONFIG_PORT, port))
        return "the game's own socket port is not readable (network subsystem +0x3c8)";
    out.port = uint16_t(port);
    if (!out.port)
        return "the game's own socket port reads 0";
    if (!m.ptr(ns + NS_INVITES, invites))
        return "the network subsystem has no invite manager at +0x3b8";
    if (!m.ptr(owner + OWNER_REQUEST, request))
        return "the join request owner has no request record at +0xc0";
    uint8_t pending = 0;
    if (!m.get(request + REQ_PENDING, pending))
        return "the join request record is not readable";
    if (pending)
        return "a join request is already pending";
    out.request = request;
    out.invites = invites;
    out.writes = join_request_writes(request, out.ip, out.port);
    return {};
}

// Whether the invite manager holds the invite this role sent (type at +0x18) and has taken its join
// delegate (+0xb78). The invite arrives through an interrupt the game normally routes to the
// multiplayer screen; without one the delegate stays empty and the join has to be requested by hand.
struct InviteState {
    bool readable = false;
    uint32_t type = 0;
    uint64_t delegate = 0;
};

inline InviteState invite_state(const Memory &m, uint64_t invites)
{
    InviteState s;
    s.readable = m.get(invites + INVITES_TYPE, s.type) && m.get(invites + INVITES_DELEGATE, s.delegate);
    return s;
}

inline uint32_t invite_type_for(Role r)
{
    return r == Role::host ? INVITE_CREATE_CUSTOM : INVITE_SESSION;
}

// The session's role (setRole in onCreateSessionComplete and the join's completion): 0 offline, 1 host,
// 2 client. -1 when it cannot be read. This is the "session established" signal that does not depend on
// the lobby's flow state, which the front end only enters after home takes the lobby link.
inline int session_role(const Memory &m, uint64_t ns)
{
    uint64_t session = 0;
    int32_t role = 0;
    if (!m.ptr(ns + NS_SESSION, session) || !m.get(session + SESSION_ROLE, role))
        return -1;
    return role;
}

inline int established_role(Role r)
{
    return r == Role::host ? 1 : 2;
}

enum class Stage { idle, mode, call, interrupt, lobby, done, failed };

struct Facts {
    bool lobby = false;
    int session_role = -1;
    bool home = false;
    bool lan_mode = false;
    bool pending_taken = false;
    InviteState invite;
};

constexpr uint64_t MODE_MS = 3000, PENDING_MS = 5000, INTERRUPT_MS = 4000, HOST_LOBBY_MS = 120000,
                   JOIN_LOBBY_MS = 30000;

enum class Do { nothing, mode_switch, host_call, join_write, fallback_notify };

inline const char *do_name(Do d)
{
    switch (d) {
    case Do::mode_switch: return "switch to LAN mode";
    case Do::host_call: return "write the Car Category values and call CustomCreate";
    case Do::join_write: return "write the join request";
    case Do::fallback_notify: return "request the join through onInviteJoinNotification";
    default: return "nothing";
    }
}

// The steps once a plan holds, one per flow tick: the mode switch and its check, the host call or the
// request write, a wait for the invite to reach the join (with the by-hand request when it does not), and a
// wait for the lobby. Every wait has a deadline and ends in a named failure. Leaving the main menu is
// expected (the invite's screens); coming back to it before the lobby opened is a failure.
class Run {
public:
    // from_home: the game was on its main menu when the run started, so coming back to it before the lobby
    // opened is a failure. A run started before home (the room boot) never sees home.
    void start(Role r, uint64_t now, bool home_start = true)
    {
        role = r;
        from_home = home_start;
        stage = Stage::mode;
        phase = 0;
        error.clear();
        by_hand = false;
        left_home = false;
        deadline = now + MODE_MS;
        taken_at = 0;
    }

    bool active() const { return stage != Stage::idle && stage != Stage::done && stage != Stage::failed; }
    bool done() const { return stage == Stage::done; }
    bool failed() const { return stage == Stage::failed; }

    void fail(const std::string &why)
    {
        error = why;
        stage = Stage::failed;
    }

    Do tick(const Facts &f, uint64_t now)
    {
        if (!active())
            return Do::nothing;
        bool joined = f.lobby || f.session_role == established_role(role);
        if (from_home && stage >= Stage::interrupt && !joined) {
            if (!f.home)
                left_home = true;
            else if (left_home) {
                fail("the game went back to its main menu before the LAN lobby opened");
                return Do::nothing;
            }
        }
        switch (stage) {
        case Stage::mode:
            if (phase == 0) {
                phase = 1;
                return Do::mode_switch;
            }
            if (f.lan_mode) {
                stage = Stage::call;
                return Do::nothing;
            }
            if (now >= deadline)
                fail("the game did not switch to LAN mode");
            return Do::nothing;
        case Stage::call:
            stage = Stage::interrupt;
            deadline = now + PENDING_MS;
            taken_at = role == Role::host ? now : 0;
            return role == Role::host ? Do::host_call : Do::join_write;
        case Stage::interrupt:
            if (joined) {
                stage = Stage::done;
                return Do::nothing;
            }
            if (!taken_at) {
                if (f.pending_taken)
                    taken_at = now;
                else if (now >= deadline)
                    fail("the game did not take the join request");
                return Do::nothing;
            }
            if (f.invite.readable && f.invite.delegate) {
                to_lobby(now);
                return Do::nothing;
            }
            if (now >= taken_at + INTERRUPT_MS) {
                if (!f.invite.readable || f.invite.type != invite_type_for(role)) {
                    fail("the invite never reached the invite manager");
                    return Do::nothing;
                }
                by_hand = true;
                to_lobby(now);
                return Do::fallback_notify;
            }
            return Do::nothing;
        case Stage::lobby:
            if (joined)
                stage = Stage::done;
            else if (now >= deadline)
                fail(role == Role::host ? "the session was not created" : "the host's session was not joined");
            return Do::nothing;
        default:
            return Do::nothing;
        }
    }

    Role role = Role::host;
    Stage stage = Stage::idle;
    std::string error;
    bool by_hand = false;
    bool from_home = true;

private:
    void to_lobby(uint64_t now)
    {
        stage = Stage::lobby;
        deadline = now + (role == Role::host ? HOST_LOBBY_MS : JOIN_LOBBY_MS);
    }

    int phase = 0;
    bool left_home = false;
    uint64_t deadline = 0, taken_at = 0;
};

}
