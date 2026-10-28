// SPDX-License-Identifier: AGPL-3.0-or-later

#include "telemetry.hpp"
#include "telemetry_parse.hpp"

#include <winsock2.h>
#include <ws2tcpip.h>

#include <atomic>
#include <chrono>
#include <mutex>
#include <thread>

namespace steward {
namespace {

std::mutex g_mu;
Model g_model;
std::thread g_thread;
std::atomic<bool> g_run{false};
std::atomic<uintptr_t> g_sock{static_cast<uintptr_t>(INVALID_SOCKET)};
uint16_t g_port = 0;

double now_s()
{
    using namespace std::chrono;
    return duration<double>(steady_clock::now().time_since_epoch()).count();
}

void run(SOCKET s)
{
    std::vector<uint8_t> buf(2048);
    while (g_run.load()) {
        const int n = recv(s, reinterpret_cast<char *>(buf.data()), static_cast<int>(buf.size()), 0);
        if (n == SOCKET_ERROR) {
            const int e = WSAGetLastError();
            if (e == WSAETIMEDOUT || e == WSAEWOULDBLOCK || e == WSAEMSGSIZE) continue;
            break;
        }
        if (n <= 0) continue;
        std::lock_guard<std::mutex> lk(g_mu);
        g_model.feed(buf.data(), static_cast<size_t>(n), now_s());
    }
}

}

bool telemetry_start(uint16_t port)
{
    if (g_run.load()) return g_port == port;
    WSADATA wsa;
    if (WSAStartup(MAKEWORD(2, 2), &wsa) != 0) return false;
    SOCKET s = socket(AF_INET, SOCK_DGRAM, IPPROTO_UDP);
    if (s == INVALID_SOCKET) return false;
    BOOL reuse = TRUE;
    setsockopt(s, SOL_SOCKET, SO_REUSEADDR, reinterpret_cast<const char *>(&reuse), sizeof reuse);
    sockaddr_in a{};
    a.sin_family = AF_INET;
    a.sin_port = htons(port);
    a.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
    if (bind(s, reinterpret_cast<sockaddr *>(&a), sizeof a) != 0) {
        closesocket(s);
        return false;
    }
    DWORD timeout = 250;
    setsockopt(s, SOL_SOCKET, SO_RCVTIMEO, reinterpret_cast<const char *>(&timeout), sizeof timeout);
    g_port = port;
    g_sock = static_cast<uintptr_t>(s);
    g_run = true;
    g_thread = std::thread(run, s);
    return true;
}

void telemetry_stop()
{
    if (!g_run.exchange(false)) return;
    if (g_thread.joinable()) g_thread.join();
    const SOCKET s = static_cast<SOCKET>(g_sock.exchange(static_cast<uintptr_t>(INVALID_SOCKET)));
    if (s != INVALID_SOCKET) closesocket(s);
}

Snapshot telemetry_snapshot()
{
    std::lock_guard<std::mutex> lk(g_mu);
    return g_model.snapshot(now_s());
}

}
