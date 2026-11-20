#include "core/diagnostics/net_trace/trace.hpp"

#include <cstdint>
#include <string>

// The ws2_32 hooks of the socket trace: each one calls the original with the caller's arguments, traces the
// call and returns its result with the thread's last error restored.
namespace net_trace {

namespace {

thread_local int busy;

bool off() { return busy || capped.load(std::memory_order_relaxed); }

struct Guard {
    DWORD in = GetLastError(), err = 0;
    Guard() { busy++; }
    void pre() const { SetLastError(in); }
    void done() { err = GetLastError(); }
    ~Guard()
    {
        busy--;
        SetLastError(err);
    }
};

bool readable(int r, DWORD err) { return r != SOCKET_ERROR || err != WSAEFAULT; }

bool pending(int r, DWORD err) { return r == 0 || err == WSA_IO_PENDING; }

void *event_of(const WSAOVERLAPPED *ov) { return can_read(ov, sizeof *ov) ? ov->hEvent : nullptr; }

WSABUF first_buf(const WSABUF *bufs, DWORD n) { return n && can_read(bufs, sizeof *bufs) ? bufs[0] : WSABUF{0, nullptr}; }

int read_int(const int *p) { return can_read(p, sizeof *p) ? *p : -1; }

SOCKET WSAAPI h_socket(int af, int type, int proto)
{
    using F = SOCKET(WSAAPI *)(int, int, int);
    if (off())
        return orig<F>(C_SOCKET)(af, type, proto);
    Guard g;
    g.pre();
    SOCKET r = orig<F>(C_SOCKET)(af, type, proto);
    g.done();
    note_socket(r, af, type, proto);
    trace(C_SOCKET, r, RA, int(r == INVALID_SOCKET ? -1 : 0), r == INVALID_SOCKET ? g.err : 0, "af=%d type=%d proto=%d", af, type,
          proto);
    return r;
}

SOCKET WSAAPI h_WSASocketW(int af, int type, int proto, LPWSAPROTOCOL_INFOW info, GROUP group, DWORD flags)
{
    using F = SOCKET(WSAAPI *)(int, int, int, LPWSAPROTOCOL_INFOW, GROUP, DWORD);
    if (off())
        return orig<F>(C_WSASOCKETW)(af, type, proto, info, group, flags);
    Guard g;
    g.pre();
    SOCKET r = orig<F>(C_WSASOCKETW)(af, type, proto, info, group, flags);
    g.done();
    note_socket(r, af, type, proto);
    trace(C_WSASOCKETW, r, RA, int(r == INVALID_SOCKET ? -1 : 0), r == INVALID_SOCKET ? g.err : 0,
          "af=%d type=%d proto=%d flags=0x%lx", af, type, proto, flags);
    return r;
}

int WSAAPI h_bind(SOCKET s, const sockaddr *a, int len)
{
    using F = int(WSAAPI *)(SOCKET, const sockaddr *, int);
    if (off())
        return orig<F>(C_BIND)(s, a, len);
    Guard g;
    g.pre();
    int r = orig<F>(C_BIND)(s, a, len);
    g.done();
    if (!r)
        note_addr(s, a, len, true);
    trace(C_BIND, s, RA, r, r ? g.err : 0, "addr=%s len=%d", readable(r, g.err) ? addr(a, len).c_str() : "?", len);
    return r;
}

int WSAAPI h_connect(SOCKET s, const sockaddr *a, int len)
{
    using F = int(WSAAPI *)(SOCKET, const sockaddr *, int);
    if (off())
        return orig<F>(C_CONNECT)(s, a, len);
    Guard g;
    g.pre();
    int r = orig<F>(C_CONNECT)(s, a, len);
    g.done();
    if (readable(r, g.err))
        note_addr(s, a, len, false);
    trace(C_CONNECT, s, RA, r, r ? g.err : 0, "addr=%s len=%d", readable(r, g.err) ? addr(a, len).c_str() : "?", len);
    return r;
}

int WSAAPI h_setsockopt(SOCKET s, int level, int name, const char *val, int len)
{
    using F = int(WSAAPI *)(SOCKET, int, int, const char *, int);
    if (off())
        return orig<F>(C_SETSOCKOPT)(s, level, name, val, len);
    Guard g;
    g.pre();
    int r = orig<F>(C_SETSOCKOPT)(s, level, name, val, len);
    g.done();
    trace(C_SETSOCKOPT, s, RA, r, r ? g.err : 0, "level=0x%x name=0x%x len=%d val=%s", level, name, len,
          readable(r, g.err) ? hexdump(val, len).c_str() : "?");
    return r;
}

int WSAAPI h_ioctlsocket(SOCKET s, long cmd, u_long *arg)
{
    using F = int(WSAAPI *)(SOCKET, long, u_long *);
    if (off())
        return orig<F>(C_IOCTLSOCKET)(s, cmd, arg);
    Guard g;
    g.pre();
    int r = orig<F>(C_IOCTLSOCKET)(s, cmd, arg);
    g.done();
    trace(C_IOCTLSOCKET, s, RA, r, r ? g.err : 0, "cmd=0x%lx arg=%lu", (unsigned long)cmd, arg && readable(r, g.err) ? *arg : 0);
    return r;
}

int WSAAPI h_getsockname(SOCKET s, sockaddr *a, int *len)
{
    using F = int(WSAAPI *)(SOCKET, sockaddr *, int *);
    if (off())
        return orig<F>(C_GETSOCKNAME)(s, a, len);
    Guard g;
    g.pre();
    int r = orig<F>(C_GETSOCKNAME)(s, a, len);
    g.done();
    if (!r && len)
        note_addr(s, a, *len, true);
    trace(C_GETSOCKNAME, s, RA, r, r ? g.err : 0, "addr=%s", r ? "-" : addr(a, len ? *len : -1).c_str());
    return r;
}

int WSAAPI h_sendto(SOCKET s, const char *buf, int len, int flags, const sockaddr *to, int tolen)
{
    using F = int(WSAAPI *)(SOCKET, const char *, int, int, const sockaddr *, int);
    if (off())
        return orig<F>(C_SENDTO)(s, buf, len, flags, to, tolen);
    Guard g;
    g.pre();
    int r = orig<F>(C_SENDTO)(s, buf, len, flags, to, tolen);
    g.done();
    bool ok = readable(r, g.err);
    trace(C_SENDTO, s, RA, r, r < 0 ? g.err : 0, "to=%s len=%d flags=0x%x data=%s", ok ? addr(to, tolen).c_str() : "?", len,
          flags, ok ? hexdump(buf, len).c_str() : "?");
    return r;
}

int WSAAPI h_send(SOCKET s, const char *buf, int len, int flags)
{
    using F = int(WSAAPI *)(SOCKET, const char *, int, int);
    if (off())
        return orig<F>(C_SEND)(s, buf, len, flags);
    Guard g;
    g.pre();
    int r = orig<F>(C_SEND)(s, buf, len, flags);
    g.done();
    trace(C_SEND, s, RA, r, r < 0 ? g.err : 0, "len=%d flags=0x%x data=%s", len, flags,
          readable(r, g.err) ? hexdump(buf, len).c_str() : "?");
    return r;
}

int WSAAPI h_WSASendTo(SOCKET s, LPWSABUF bufs, DWORD n, LPDWORD sent, DWORD flags, const sockaddr *to, int tolen,
                       LPWSAOVERLAPPED ov, LPWSAOVERLAPPED_COMPLETION_ROUTINE cr)
{
    using F = int(WSAAPI *)(SOCKET, LPWSABUF, DWORD, LPDWORD, DWORD, const sockaddr *, int, LPWSAOVERLAPPED,
                            LPWSAOVERLAPPED_COMPLETION_ROUTINE);
    if (off())
        return orig<F>(C_WSASENDTO)(s, bufs, n, sent, flags, to, tolen, ov, cr);
    Guard g;
    WSABUF b0 = first_buf(bufs, n);
    std::string data = hexdump_safe(b0.buf, b0.len), dest = addr_safe(to, tolen);
    void *ev = event_of(ov);
    g.pre();
    int r = orig<F>(C_WSASENDTO)(s, bufs, n, sent, flags, to, tolen, ov, cr);
    g.done();
    trace(C_WSASENDTO, s, RA, r, r ? g.err : 0, "to=%s nbufs=%lu len0=%lu sent=%ld data=%s ov=0x%llx ev=0x%llx", dest.c_str(), n,
          b0.len, !r && sent ? long(*sent) : -1L, data.c_str(), PTR(ov), PTR(ev));
    return r;
}

int WSAAPI h_recvfrom(SOCKET s, char *buf, int len, int flags, sockaddr *from, int *fromlen)
{
    using F = int(WSAAPI *)(SOCKET, char *, int, int, sockaddr *, int *);
    if (off())
        return orig<F>(C_RECVFROM)(s, buf, len, flags, from, fromlen);
    Guard g;
    g.pre();
    int r = orig<F>(C_RECVFROM)(s, buf, len, flags, from, fromlen);
    g.done();
    if (r >= 0)
        trace(C_RECVFROM, s, RA, r, 0, "cap=%d flags=0x%x from=%s data=%s", len, flags,
              addr(from, fromlen ? *fromlen : -1).c_str(), hexdump(buf, r).c_str());
    else
        trace(C_RECVFROM, s, RA, r, g.err, "cap=%d flags=0x%x", len, flags);
    return r;
}

int WSAAPI h_recv(SOCKET s, char *buf, int len, int flags)
{
    using F = int(WSAAPI *)(SOCKET, char *, int, int);
    if (off())
        return orig<F>(C_RECV)(s, buf, len, flags);
    Guard g;
    g.pre();
    int r = orig<F>(C_RECV)(s, buf, len, flags);
    g.done();
    trace(C_RECV, s, RA, r, r < 0 ? g.err : 0, "cap=%d flags=0x%x data=%s", len, flags, r > 0 ? hexdump(buf, r).c_str() : "-");
    return r;
}

int WSAAPI h_WSARecvFrom(SOCKET s, LPWSABUF bufs, DWORD n, LPDWORD got, LPDWORD flags, sockaddr *from, LPINT fromlen,
                         LPWSAOVERLAPPED ov, LPWSAOVERLAPPED_COMPLETION_ROUTINE cr)
{
    using F = int(WSAAPI *)(SOCKET, LPWSABUF, DWORD, LPDWORD, LPDWORD, sockaddr *, LPINT, LPWSAOVERLAPPED,
                            LPWSAOVERLAPPED_COMPLETION_ROUTINE);
    if (off())
        return orig<F>(C_WSARECVFROM)(s, bufs, n, got, flags, from, fromlen, ov, cr);
    Guard g;
    WSABUF b0 = first_buf(bufs, n);
    int fl_in = fromlen ? read_int(fromlen) : -1;
    void *ev = event_of(ov);
    g.pre();
    int r = orig<F>(C_WSARECVFROM)(s, bufs, n, got, flags, from, fromlen, ov, cr);
    g.done();
    if (ov && pending(r, g.err))
        note_post(ov, Post{s, b0.buf, b0.len, from, fromlen, true});
    trace(C_WSARECVFROM, s, RA, r, r ? g.err : 0,
          "nbufs=%lu buf=0x%llx cap=%lu from_buf=0x%llx fromlen=%d flags_ptr=0x%llx ov=0x%llx ev=0x%llx cr=0x%llx got=%ld", n,
          PTR(b0.buf), b0.len, PTR(from), fl_in, PTR(flags), PTR(ov), PTR(ev), PTR(cr), !r && got ? long(*got) : -1L);
    return r;
}

int WSAAPI h_WSARecv(SOCKET s, LPWSABUF bufs, DWORD n, LPDWORD got, LPDWORD flags, LPWSAOVERLAPPED ov,
                     LPWSAOVERLAPPED_COMPLETION_ROUTINE cr)
{
    using F = int(WSAAPI *)(SOCKET, LPWSABUF, DWORD, LPDWORD, LPDWORD, LPWSAOVERLAPPED, LPWSAOVERLAPPED_COMPLETION_ROUTINE);
    if (off())
        return orig<F>(C_WSARECV)(s, bufs, n, got, flags, ov, cr);
    Guard g;
    WSABUF b0 = first_buf(bufs, n);
    void *ev = event_of(ov);
    g.pre();
    int r = orig<F>(C_WSARECV)(s, bufs, n, got, flags, ov, cr);
    g.done();
    if (ov && pending(r, g.err))
        note_post(ov, Post{s, b0.buf, b0.len, nullptr, nullptr, false});
    trace(C_WSARECV, s, RA, r, r ? g.err : 0, "nbufs=%lu buf=0x%llx cap=%lu ov=0x%llx ev=0x%llx got=%ld", n, PTR(b0.buf), b0.len,
          PTR(ov), PTR(ev), !r && got ? long(*got) : -1L);
    return r;
}

BOOL WSAAPI h_WSAGetOverlappedResult(SOCKET s, LPWSAOVERLAPPED ov, LPDWORD bytes, BOOL wait, LPDWORD flags)
{
    using F = BOOL(WSAAPI *)(SOCKET, LPWSAOVERLAPPED, LPDWORD, BOOL, LPDWORD);
    if (off())
        return orig<F>(C_WSAGETOVERLAPPEDRESULT)(s, ov, bytes, wait, flags);
    Guard g;
    void *ev = event_of(ov);
    g.pre();
    BOOL r = orig<F>(C_WSAGETOVERLAPPEDRESULT)(s, ov, bytes, wait, flags);
    g.done();
    Post p{};
    bool known = take_post(ov, p, r || g.err != WSA_IO_INCOMPLETE);
    DWORD got = r && bytes ? *bytes : 0;
    long long shown = std::min<long long>(got, p.len);
    if (r && known && p.recvfrom)
        trace(C_WSAGETOVERLAPPEDRESULT, s, RA, r, 0,
              "ov=0x%llx ev=0x%llx bytes=%lu flags=0x%lx buf=0x%llx data=%s from_buf=0x%llx from=%s", PTR(ov), PTR(ev), got,
              flags ? *flags : 0, PTR(p.buf), hexdump(p.buf, shown).c_str(), PTR(p.from),
              addr(p.from, p.fromlen ? *p.fromlen : -1).c_str());
    else if (r && known)
        trace(C_WSAGETOVERLAPPEDRESULT, s, RA, r, 0, "ov=0x%llx ev=0x%llx bytes=%lu flags=0x%lx buf=0x%llx data=%s", PTR(ov),
              PTR(ev), got, flags ? *flags : 0, PTR(p.buf), hexdump(p.buf, shown).c_str());
    else
        trace(C_WSAGETOVERLAPPEDRESULT, s, RA, r, r ? 0 : g.err, "ov=0x%llx ev=0x%llx wait=%d bytes=%lu post=%s", PTR(ov),
              PTR(ev), wait, got, known ? "seen" : "unseen");
    return r;
}

DWORD WSAAPI h_WSAWaitForMultipleEvents(DWORD n, const WSAEVENT *ev, BOOL all, DWORD timeout, BOOL alertable)
{
    using F = DWORD(WSAAPI *)(DWORD, const WSAEVENT *, BOOL, DWORD, BOOL);
    if (off())
        return orig<F>(C_WSAWAITFORMULTIPLEEVENTS)(n, ev, all, timeout, alertable);
    Guard g;
    g.pre();
    DWORD r = orig<F>(C_WSAWAITFORMULTIPLEEVENTS)(n, ev, all, timeout, alertable);
    g.done();
    char list[160] = "";
    for (DWORD i = 0, k = 0; ev && i < n && i < 6; i++)
        k += snprintf(list + k, sizeof list - k, "%s0x%llx", i ? "," : "", PTR(ev[i]));
    trace(C_WSAWAITFORMULTIPLEEVENTS, INVALID_SOCKET, RA, int(r), r == WSA_WAIT_FAILED ? g.err : 0,
          "n=%lu events=%s%s all=%d timeout=%ld signalled=0x%llx", n, list, n > 6 ? ",..." : "", all, long(timeout),
          PTR(ev && r < n ? ev[r] : nullptr));
    return r;
}

int WSAAPI h_WSAEventSelect(SOCKET s, WSAEVENT e, long mask)
{
    using F = int(WSAAPI *)(SOCKET, WSAEVENT, long);
    if (off())
        return orig<F>(C_WSAEVENTSELECT)(s, e, mask);
    Guard g;
    g.pre();
    int r = orig<F>(C_WSAEVENTSELECT)(s, e, mask);
    g.done();
    trace(C_WSAEVENTSELECT, s, RA, r, r ? g.err : 0, "event=0x%llx mask=0x%lx", PTR(e), (unsigned long)mask);
    return r;
}

WSAEVENT WSAAPI h_WSACreateEvent()
{
    using F = WSAEVENT(WSAAPI *)();
    if (off())
        return orig<F>(C_WSACREATEEVENT)();
    Guard g;
    g.pre();
    WSAEVENT r = orig<F>(C_WSACREATEEVENT)();
    g.done();
    trace(C_WSACREATEEVENT, INVALID_SOCKET, RA, r ? 0 : -1, r ? 0 : g.err, "event=0x%llx", PTR(r));
    return r;
}

int WSAAPI h_WSAIoctl(SOCKET s, DWORD code, LPVOID in, DWORD inlen, LPVOID out, DWORD outlen, LPDWORD got, LPWSAOVERLAPPED ov,
                      LPWSAOVERLAPPED_COMPLETION_ROUTINE cr)
{
    using F =
        int(WSAAPI *)(SOCKET, DWORD, LPVOID, DWORD, LPVOID, DWORD, LPDWORD, LPWSAOVERLAPPED, LPWSAOVERLAPPED_COMPLETION_ROUTINE);
    if (off())
        return orig<F>(C_WSAIOCTL)(s, code, in, inlen, out, outlen, got, ov, cr);
    Guard g;
    std::string data = hexdump_safe(in, inlen);
    g.pre();
    int r = orig<F>(C_WSAIOCTL)(s, code, in, inlen, out, outlen, got, ov, cr);
    g.done();
    trace(C_WSAIOCTL, s, RA, r, r ? g.err : 0, "code=0x%lx in=%s inlen=%lu outlen=%lu got=%ld ov=0x%llx", code, data.c_str(),
          inlen, outlen, !r && got ? long(*got) : -1L, PTR(ov));
    return r;
}

int WSAAPI h_closesocket(SOCKET s)
{
    using F = int(WSAAPI *)(SOCKET);
    if (off())
        return orig<F>(C_CLOSESOCKET)(s);
    Guard g;
    std::string before = sock_desc(s);
    g.pre();
    int r = orig<F>(C_CLOSESOCKET)(s);
    g.done();
    trace_desc(C_CLOSESOCKET, s, before, RA, r, r ? g.err : 0, "closed");
    if (!r || g.err == WSAENOTSOCK)
        drop_socket(s);
    return r;
}

}

void table()
{
    auto set = [](Call c, const char *name, void *detour) {
        hooks[c].name = name;
        hooks[c].detour = detour;
    };
    set(C_SOCKET, "socket", reinterpret_cast<void *>(&h_socket));
    set(C_WSASOCKETW, "WSASocketW", reinterpret_cast<void *>(&h_WSASocketW));
    set(C_BIND, "bind", reinterpret_cast<void *>(&h_bind));
    set(C_CONNECT, "connect", reinterpret_cast<void *>(&h_connect));
    set(C_SETSOCKOPT, "setsockopt", reinterpret_cast<void *>(&h_setsockopt));
    set(C_IOCTLSOCKET, "ioctlsocket", reinterpret_cast<void *>(&h_ioctlsocket));
    set(C_GETSOCKNAME, "getsockname", reinterpret_cast<void *>(&h_getsockname));
    set(C_SENDTO, "sendto", reinterpret_cast<void *>(&h_sendto));
    set(C_SEND, "send", reinterpret_cast<void *>(&h_send));
    set(C_WSASENDTO, "WSASendTo", reinterpret_cast<void *>(&h_WSASendTo));
    set(C_RECVFROM, "recvfrom", reinterpret_cast<void *>(&h_recvfrom));
    set(C_RECV, "recv", reinterpret_cast<void *>(&h_recv));
    set(C_WSARECVFROM, "WSARecvFrom", reinterpret_cast<void *>(&h_WSARecvFrom));
    set(C_WSARECV, "WSARecv", reinterpret_cast<void *>(&h_WSARecv));
    set(C_WSAGETOVERLAPPEDRESULT, "WSAGetOverlappedResult", reinterpret_cast<void *>(&h_WSAGetOverlappedResult));
    set(C_WSAWAITFORMULTIPLEEVENTS, "WSAWaitForMultipleEvents", reinterpret_cast<void *>(&h_WSAWaitForMultipleEvents));
    set(C_WSAEVENTSELECT, "WSAEventSelect", reinterpret_cast<void *>(&h_WSAEventSelect));
    set(C_WSACREATEEVENT, "WSACreateEvent", reinterpret_cast<void *>(&h_WSACreateEvent));
    set(C_WSAIOCTL, "WSAIoctl", reinterpret_cast<void *>(&h_WSAIoctl));
    set(C_CLOSESOCKET, "closesocket", reinterpret_cast<void *>(&h_closesocket));
}

}
