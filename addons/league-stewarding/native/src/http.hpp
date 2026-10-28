// SPDX-License-Identifier: AGPL-3.0-or-later
#pragma once

#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <string>
#include <string_view>

namespace steward::http {

struct Request {
    std::string method, path, query, token_header, host, body;
    size_t content_length = 0;
};

enum class Parse { incomplete, ok, bad, too_large };

inline constexpr size_t kMaxRequest = 64 * 1024;

inline std::string lower(std::string_view s)
{
    std::string out(s);
    for (char &c : out)
        if (c >= 'A' && c <= 'Z')
            c = char(c - 'A' + 'a');
    return out;
}

inline std::string_view trim(std::string_view s)
{
    while (!s.empty() && (s.front() == ' ' || s.front() == '\t'))
        s.remove_prefix(1);
    while (!s.empty() && (s.back() == ' ' || s.back() == '\t' || s.back() == '\r'))
        s.remove_suffix(1);
    return s;
}

inline Parse parse_request(std::string_view raw, Request &out)
{
    if (raw.size() > kMaxRequest)
        return Parse::too_large;
    size_t head_end = raw.find("\r\n\r\n");
    if (head_end == std::string_view::npos)
        return Parse::incomplete;
    std::string_view head = raw.substr(0, head_end);
    size_t line_end = head.find("\r\n");
    std::string_view line = head.substr(0, line_end);
    size_t sp1 = line.find(' ');
    size_t sp2 = sp1 == std::string_view::npos ? sp1 : line.find(' ', sp1 + 1);
    if (sp2 == std::string_view::npos)
        return Parse::bad;
    out = Request{};
    out.method = std::string(line.substr(0, sp1));
    std::string_view target = line.substr(sp1 + 1, sp2 - sp1 - 1);
    if (target.empty() || target[0] != '/')
        return Parse::bad;
    size_t q = target.find('?');
    out.path = std::string(target.substr(0, q));
    if (q != std::string_view::npos)
        out.query = std::string(target.substr(q + 1));
    while (line_end != std::string_view::npos) {
        size_t start = line_end + 2;
        line_end = head.find("\r\n", start);
        std::string_view h = head.substr(start, line_end == std::string_view::npos ? std::string_view::npos : line_end - start);
        size_t colon = h.find(':');
        if (colon == std::string_view::npos)
            continue;
        std::string key = lower(h.substr(0, colon));
        std::string_view value = trim(h.substr(colon + 1));
        if (key == "x-steward-token")
            out.token_header = std::string(value);
        else if (key == "host")
            out.host = std::string(value);
        else if (key == "content-length") {
            char *end = nullptr;
            std::string v(value);
            unsigned long long n = std::strtoull(v.c_str(), &end, 10);
            if (v.empty() || *end || n > kMaxRequest)
                return n > kMaxRequest ? Parse::too_large : Parse::bad;
            out.content_length = size_t(n);
        }
    }
    size_t body_start = head_end + 4;
    if (raw.size() < body_start + out.content_length)
        return Parse::incomplete;
    out.body = std::string(raw.substr(body_start, out.content_length));
    return Parse::ok;
}

inline std::string query_param(std::string_view query, std::string_view key)
{
    size_t pos = 0;
    while (pos <= query.size()) {
        size_t amp = query.find('&', pos);
        std::string_view pair = query.substr(pos, amp == std::string_view::npos ? std::string_view::npos : amp - pos);
        size_t eq = pair.find('=');
        if (pair.substr(0, eq) == key)
            return eq == std::string_view::npos ? std::string() : std::string(pair.substr(eq + 1));
        if (amp == std::string_view::npos)
            break;
        pos = amp + 1;
    }
    return {};
}

inline bool same_text(std::string_view a, std::string_view b)
{
    if (a.size() != b.size())
        return false;
    unsigned char diff = 0;
    for (size_t i = 0; i < a.size(); i++)
        diff |= static_cast<unsigned char>(a[i] ^ b[i]);
    return diff == 0;
}

inline bool token_ok(const Request &r, std::string_view token)
{
    if (token.empty())
        return false;
    std::string given = !r.token_header.empty() ? r.token_header : query_param(r.query, "t");
    return same_text(given, token);
}

inline bool host_ok(const Request &r, uint16_t port)
{
    std::string h = lower(r.host);
    std::string p = std::to_string(port);
    return h == "127.0.0.1:" + p || h == "localhost:" + p;
}

inline std::string reason(int status)
{
    switch (status) {
    case 200: return "OK";
    case 400: return "Bad Request";
    case 403: return "Forbidden";
    case 404: return "Not Found";
    case 405: return "Method Not Allowed";
    case 413: return "Payload Too Large";
    default: return "Error";
    }
}

inline std::string response(int status, std::string_view type, std::string_view body)
{
    std::string out = "HTTP/1.1 " + std::to_string(status) + " " + reason(status) + "\r\n";
    out += "Content-Type: " + std::string(type) + "\r\n";
    out += "Content-Length: " + std::to_string(body.size()) + "\r\n";
    out += "Cache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nReferrer-Policy: no-referrer\r\nConnection: close\r\n\r\n";
    out += body;
    return out;
}

inline std::string json_escape(std::string_view s)
{
    std::string out;
    out.reserve(s.size() + 2);
    for (unsigned char c : s) {
        switch (c) {
        case '"': out += "\\\""; break;
        case '\\': out += "\\\\"; break;
        case '\n': out += "\\n"; break;
        case '\r': out += "\\r"; break;
        case '\t': out += "\\t"; break;
        default:
            if (c < 0x20) {
                char buf[8];
                std::snprintf(buf, sizeof buf, "\\u%04x", c);
                out += buf;
            } else
                out += char(c);
        }
    }
    return out;
}

inline size_t json_value_start(std::string_view body, std::string_view key)
{
    std::string needle = "\"" + std::string(key) + "\"";
    size_t pos = 0;
    while ((pos = body.find(needle, pos)) != std::string_view::npos) {
        size_t i = pos + needle.size();
        while (i < body.size() && (body[i] == ' ' || body[i] == '\t' || body[i] == '\n' || body[i] == '\r'))
            i++;
        if (i < body.size() && body[i] == ':') {
            i++;
            while (i < body.size() && (body[i] == ' ' || body[i] == '\t' || body[i] == '\n' || body[i] == '\r'))
                i++;
            return i;
        }
        pos += needle.size();
    }
    return std::string_view::npos;
}

inline bool json_string(std::string_view body, std::string_view key, std::string &out)
{
    size_t i = json_value_start(body, key);
    if (i == std::string_view::npos || i >= body.size() || body[i] != '"')
        return false;
    out.clear();
    for (i++; i < body.size() && body[i] != '"'; i++) {
        if (body[i] == '\\' && i + 1 < body.size()) {
            char n = body[++i];
            out += n == 'n' ? '\n' : n == 't' ? '\t' : n == 'r' ? '\r' : n;
        } else
            out += body[i];
    }
    return i < body.size();
}

inline bool json_number(std::string_view body, std::string_view key, double &out)
{
    size_t i = json_value_start(body, key);
    if (i == std::string_view::npos)
        return false;
    std::string rest(body.substr(i, 32));
    char *end = nullptr;
    double v = std::strtod(rest.c_str(), &end);
    if (end == rest.c_str())
        return false;
    out = v;
    return true;
}

inline bool json_bool(std::string_view body, std::string_view key, bool &out)
{
    size_t i = json_value_start(body, key);
    if (i == std::string_view::npos)
        return false;
    if (body.substr(i, 4) == "true")
        return out = true, true;
    if (body.substr(i, 5) == "false")
        return out = false, true;
    return false;
}

}
