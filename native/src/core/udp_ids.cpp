#define WIN32_LEAN_AND_MEAN
#include <winsock2.h>
#include <ws2tcpip.h>

#include "imports.hpp"
#include "runtime.hpp"
#include "udp_ids_logic.hpp"

#include <swofty/mixin.hpp>

#include <atomic>
#include <cstdio>
#include <string>

// UDP telemetry reports the stat ids of added teams and drivers, which overlays that know EA's ids show
// as unknown. The game sends its finished telemetry packets with sendto through its ws2_32 import slots
// (both import descriptors, like the other socket hooks). This hook copies a buffer that parses as an F1
// packet (udp_ids_logic.hpp), replaces the ids swofty/udp_ids.json maps, and sends the copy; every other
// call passes through with its arguments. Datagrams to the LAN game port 21414 and to 10.77.0.0/16 are
// never looked at. SWOFTY_UDP_STOCK_IDS=0 turns it off.

namespace {

namespace ids = runtime::udp_ids;

constexpr uint16_t GAME_PORT = 21414;
constexpr size_t LINES = 12;

using SendToFn = int(WSAAPI *)(SOCKET, const char *, int, int, const sockaddr *, int);

void *detour_target;
void *orig_sendto;
ids::Map map;
std::atomic<bool> armed;
std::atomic<bool> said;
std::atomic<int> seen_formats;

bool lan_traffic(const sockaddr *to, int tolen)
{
    if (!to || tolen < int(sizeof(sockaddr_in)) || to->sa_family != AF_INET)
        return false;
    auto *in = reinterpret_cast<const sockaddr_in *>(to);
    uint16_t port = __builtin_bswap16(in->sin_port);
    uint32_t ip = __builtin_bswap32(in->sin_addr.s_addr);
    return port == GAME_PORT || (ip & 0xffff0000) == 0x0a4d0000;
}

void first(int format, int id, const std::vector<ids::Change> &changes)
{
    if (said.exchange(true))
        return;
    DWORD err = GetLastError();
    std::string what;
    for (size_t i = 0; i < changes.size() && i < 4; i++)
        what += (i ? ", " : "") + std::string(changes[i].driver ? "driver " : "team ") + std::to_string(changes[i].from) +
                " -> " + std::to_string(changes[i].to);
    swofty::log("[udp] first telemetry packet rewritten: format %d, packet %d: %s", format, id, what.c_str());
    SetLastError(err);
}

int WSAAPI h_sendto(SOCKET s, const char *buf, int len, int flags, const sockaddr *to, int tolen)
{
    auto real = reinterpret_cast<SendToFn>(orig_sendto);
    if (!armed.load(std::memory_order_relaxed) || !buf || len < int(ids::HEADER) + 1 || len > int(ids::MAX_PACKET) ||
        !ids::identify(reinterpret_cast<const uint8_t *>(buf), size_t(len)) || lan_traffic(to, tolen))
        return real(s, buf, len, flags, to, tolen);
    uint8_t copy[ids::MAX_PACKET];
    memcpy(copy, buf, size_t(len));
    std::vector<ids::Change> changes;
    int format = 0, id = 0;
    if (!ids::rewrite(map, copy, size_t(len), &changes, &format, &id))
        return real(s, buf, len, flags, to, tolen);
    first(format, id, changes);
    return real(s, reinterpret_cast<const char *>(copy), len, flags, to, tolen);
}

bool wanted_off()
{
    char v[16] = {};
    DWORD n = GetEnvironmentVariableA("SWOFTY_UDP_STOCK_IDS", v, sizeof v);
    if (!n || n >= sizeof v || (strcmp(v, "0") && strcmp(v, "off")))
        return false;
    swofty::log("[udp] stock ids off (SWOFTY_UDP_STOCK_IDS=%s)", v);
    return true;
}

void setup()
{
    if (wanted_off())
        return;
    std::string text = runtime::read_file(runtime::swofty_root() + L"udp_ids.json", size_t(4) << 20);
    if (text.empty())
        return;
    std::string why;
    if (!ids::load(text, map, why)) {
        swofty::log("[udp] swofty\\udp_ids.json is unusable (%s); telemetry ids stay as the game sends them", why.c_str());
        return;
    }
    if (!map.any)
        return;
    HMODULE ws = LoadLibraryW(L"ws2_32.dll");
    detour_target = ws ? reinterpret_cast<void *>(GetProcAddress(ws, "sendto")) : nullptr;
    int slots = 0;
    if (ws)
        runtime::each_import(ws, "ws2_32.dll", [&](const runtime::ImportSlot &s) {
            if (!s.fn || s.fn != detour_target)
                return;
            void *cur = reinterpret_cast<void *>(*s.slot);
            if (orig_sendto && orig_sendto != cur) {
                swofty::log("[udp] sendto table %d slot exe+0x%llx holds 0x%llx, not 0x%llx; left alone", s.table,
                            (unsigned long long)s.rva, (unsigned long long)uintptr_t(cur),
                            (unsigned long long)uintptr_t(orig_sendto));
                return;
            }
            void *was = orig_sendto;
            orig_sendto = cur;
            if (!runtime::write_slot(s.slot, reinterpret_cast<void *>(&h_sendto))) {
                orig_sendto = was;
                return;
            }
            slots++;
        });
    if (!slots) {
        swofty::log("[udp] no game import slot for sendto; telemetry ids stay as the game sends them");
        return;
    }
    armed = true;
    std::string head = std::to_string(map.teams.size()) + " team id(s), " + std::to_string(map.drivers.size()) +
                       " driver id(s)";
    swofty::log("[udp] stock ids on: %s, %d sendto slot(s)", head.c_str(), slots);
    std::string line;
    size_t shown = 0;
    for (auto &l : map.lines) {
        if (shown++ == LINES)
            break;
        line += (line.empty() ? "" : "; ") + l;
    }
    if (!line.empty())
        swofty::log("[udp] stock ids: %s%s", line.c_str(), map.lines.size() > LINES ? "; ..." : "");
}

}

namespace runtime {

void udp_ids_setup()
{
    setup();
}

}
