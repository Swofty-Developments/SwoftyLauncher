// SPDX-License-Identifier: AGPL-3.0-or-later

#include "server.hpp"

#include <winsock2.h>
#include <ws2tcpip.h>
#include <bcrypt.h>
#include <shellapi.h>

#include <atomic>
#include <thread>

namespace steward {
namespace {

std::atomic<bool> g_run{false};
std::atomic<uint16_t> g_port{0};
std::string g_token;
std::thread g_thread;
Backend *g_backend = nullptr;
std::span<const Asset> g_assets;

std::string random_token()
{
    uint8_t raw[16]{};
    if (BCryptGenRandom(nullptr, raw, sizeof raw, BCRYPT_USE_SYSTEM_PREFERRED_RNG) != 0)
        return {};
    static const char hex[] = "0123456789abcdef";
    std::string out;
    for (uint8_t b : raw) {
        out += hex[b >> 4];
        out += hex[b & 15];
    }
    return out;
}

bool send_all(SOCKET c, const std::string &data)
{
    size_t sent = 0;
    while (sent < data.size()) {
        int n = send(c, data.data() + sent, int(data.size() - sent), 0);
        if (n <= 0)
            return false;
        sent += size_t(n);
    }
    return true;
}

void serve(SOCKET c)
{
    DWORD timeout = 2000;
    setsockopt(c, SOL_SOCKET, SO_RCVTIMEO, reinterpret_cast<const char *>(&timeout), sizeof timeout);
    setsockopt(c, SOL_SOCKET, SO_SNDTIMEO, reinterpret_cast<const char *>(&timeout), sizeof timeout);
    std::string raw;
    char buf[4096];
    http::Request req;
    http::Parse state = http::Parse::incomplete;
    while (state == http::Parse::incomplete) {
        int n = recv(c, buf, sizeof buf, 0);
        if (n <= 0)
            break;
        raw.append(buf, size_t(n));
        state = http::parse_request(raw, req);
    }
    if (state == http::Parse::ok) {
        Reply r = route(req, g_token, g_port, *g_backend, g_assets);
        send_all(c, http::response(r.status, r.type, r.body));
    } else if (state != http::Parse::incomplete) {
        Reply r = json_reply(state == http::Parse::too_large ? 413 : 400, false, "bad request");
        send_all(c, http::response(r.status, r.type, r.body));
    }
    shutdown(c, SD_SEND);
    closesocket(c);
}

void run(SOCKET s)
{
    while (g_run.load()) {
        fd_set set;
        FD_ZERO(&set);
        FD_SET(s, &set);
        timeval tv{0, 200000};
        int ready = select(0, &set, nullptr, nullptr, &tv);
        if (ready <= 0)
            continue;
        SOCKET c = accept(s, nullptr, nullptr);
        if (c == INVALID_SOCKET)
            continue;
        serve(c);
    }
    closesocket(s);
}

}

bool server_start(Backend &backend, std::span<const Asset> assets)
{
    if (g_run.load())
        return true;
    WSADATA wsa;
    if (WSAStartup(MAKEWORD(2, 2), &wsa) != 0)
        return false;
    std::string token = random_token();
    if (token.empty())
        return false;
    SOCKET s = socket(AF_INET, SOCK_STREAM, IPPROTO_TCP);
    if (s == INVALID_SOCKET)
        return false;
    sockaddr_in a{};
    a.sin_family = AF_INET;
    a.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
    a.sin_port = 0;
    int len = sizeof a;
    if (bind(s, reinterpret_cast<sockaddr *>(&a), sizeof a) != 0 || listen(s, 8) != 0 ||
        getsockname(s, reinterpret_cast<sockaddr *>(&a), &len) != 0) {
        closesocket(s);
        return false;
    }
    g_token = token;
    g_backend = &backend;
    g_assets = assets;
    g_port = ntohs(a.sin_port);
    g_run = true;
    g_thread = std::thread(run, s);
    g_thread.detach();
    return true;
}

uint16_t server_port()
{
    return g_port;
}

const std::string &server_token()
{
    return g_token;
}

bool server_running()
{
    return g_run.load();
}

bool open_browser(const std::string &url)
{
    int n = MultiByteToWideChar(CP_UTF8, 0, url.c_str(), -1, nullptr, 0);
    if (n <= 0)
        return false;
    std::wstring wide(size_t(n), L'\0');
    MultiByteToWideChar(CP_UTF8, 0, url.c_str(), -1, wide.data(), n);
    return reinterpret_cast<INT_PTR>(ShellExecuteW(nullptr, L"open", wide.c_str(), nullptr, nullptr, SW_SHOWNORMAL)) > 32;
}

}
