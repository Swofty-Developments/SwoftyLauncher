#pragma once

#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif
#include <winsock2.h>
#include <ws2tcpip.h>

#include <safetyhook.hpp>

#include <atomic>
#include <string>

// What the socket trace's log and setup (net_trace.cpp) and its ws2_32 hooks (net_trace/hooks.cpp) share: the
// hooked calls and their originals, the socket notes and the trace line writers.
namespace net_trace {

enum Call {
    C_SOCKET,
    C_WSASOCKETW,
    C_BIND,
    C_CONNECT,
    C_SETSOCKOPT,
    C_IOCTLSOCKET,
    C_GETSOCKNAME,
    C_SENDTO,
    C_SEND,
    C_WSASENDTO,
    C_RECVFROM,
    C_RECV,
    C_WSARECVFROM,
    C_WSARECV,
    C_WSAGETOVERLAPPEDRESULT,
    C_WSAWAITFORMULTIPLEEVENTS,
    C_WSAEVENTSELECT,
    C_WSACREATEEVENT,
    C_WSAIOCTL,
    C_CLOSESOCKET,
    C_COUNT
};

struct Hook {
    const char *name;
    void *detour;
    void *orig;
    void *target;
    int slots;
    safetyhook::InlineHook inl;
};

extern Hook hooks[C_COUNT];

template <class F> F orig(Call c) { return reinterpret_cast<F>(hooks[c].orig); }

struct Post {
    SOCKET s;
    char *buf;
    ULONG len;
    sockaddr *from;
    int *fromlen;
    bool recvfrom;
};

// Set once the log reaches its size cap; the hooks stop tracing.
extern std::atomic<bool> capped;

inline bool can_read(const void *p, size_t n) { return p && n && !IsBadReadPtr(p, n); }

std::string addr(const sockaddr *a, int len = -1);
std::string addr_safe(const sockaddr *a, int len);
std::string hexdump(const void *p, long long n);
std::string hexdump_safe(const void *p, long long n);
std::string sock_desc(SOCKET s);
void trace(Call c, SOCKET s, void *ra, int ret, DWORD err, const char *fmt, ...);
void trace_desc(Call c, SOCKET s, const std::string &desc, void *ra, int ret, DWORD err, const char *fmt, ...);
void note_socket(SOCKET s, int af, int type, int proto);
void note_addr(SOCKET s, const sockaddr *a, int len, bool local);
void note_post(void *ov, const Post &p);
bool take_post(void *ov, Post &p, bool erase);
void drop_socket(SOCKET s);

// Fills the hook table with each call's name and detour (hooks.cpp).
void table();

}

#define RA __builtin_return_address(0)
#define PTR(x) static_cast<unsigned long long>(reinterpret_cast<uintptr_t>(x))
