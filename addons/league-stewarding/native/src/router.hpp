// SPDX-License-Identifier: AGPL-3.0-or-later
#pragma once

#include <span>
#include <string>
#include <string_view>

#include "api_logic.hpp"
#include "http.hpp"

namespace steward {

struct Reply {
    int status = 200;
    std::string type = "application/json";
    std::string body;
};

struct Backend {
    virtual ~Backend() = default;
    virtual std::string state_json() = 0;
    virtual std::string queue_penalty(const PenaltyAction &action) = 0;
    virtual std::string queue_race_control(RcAction action) = 0;
    virtual void set_auto_penalties(bool enabled) = 0;
};

struct Asset {
    std::string_view name, mime, data;
};

inline Reply json_reply(int status, bool ok, std::string_view error = {})
{
    Reply r;
    r.status = status;
    r.body = ok ? "{\"ok\":true,\"queued\":true}" : "{\"ok\":false,\"error\":\"" + http::json_escape(error) + "\"}";
    return r;
}

inline std::string with_token(std::string_view page, std::span<const Asset> assets, std::string_view token)
{
    std::string out(page);
    for (const Asset &a : assets) {
        std::string from = "\"" + std::string(a.name) + "\"";
        std::string to = "\"" + std::string(a.name) + "?t=" + std::string(token) + "\"";
        for (size_t at = 0; (at = out.find(from, at)) != std::string::npos; at += to.size())
            out.replace(at, from.size(), to);
    }
    return out;
}

inline Reply route(const http::Request &req, std::string_view token, uint16_t port, Backend &backend,
                   std::span<const Asset> assets)
{
    if (!http::host_ok(req, port) || !http::token_ok(req, token))
        return json_reply(403, false, "forbidden");
    if (req.path == "/api/state") {
        if (req.method != "GET")
            return json_reply(405, false, "use GET");
        Reply r;
        r.body = backend.state_json();
        return r;
    }
    if (req.path.rfind("/api/", 0) == 0) {
        if (req.method != "POST")
            return json_reply(405, false, "use POST");
        std::string err;
        if (req.path == "/api/penalty") {
            PenaltyAction a;
            if (!parse_penalty(req.body, a, err))
                return json_reply(400, false, err);
            err = backend.queue_penalty(a);
            return json_reply(200, err.empty(), err);
        }
        if (req.path == "/api/race-control") {
            RcAction a;
            if (!parse_rc(req.body, a, err))
                return json_reply(400, false, err);
            err = backend.queue_race_control(a);
            return json_reply(200, err.empty(), err);
        }
        if (req.path == "/api/auto-penalties") {
            bool enabled = true;
            if (!http::json_bool(req.body, "enabled", enabled))
                return json_reply(400, false, "enabled must be true or false");
            backend.set_auto_penalties(enabled);
            return json_reply(200, true);
        }
        return json_reply(404, false, "no such endpoint");
    }
    if (req.method != "GET")
        return json_reply(405, false, "use GET");
    std::string_view want = req.path == "/" ? "index.html" : std::string_view(req.path).substr(1);
    for (const Asset &a : assets) {
        if (a.name != want)
            continue;
        Reply r;
        r.type = std::string(a.mime);
        r.body = a.mime.rfind("text/html", 0) == 0 ? with_token(a.data, assets, token) : std::string(a.data);
        return r;
    }
    return json_reply(404, false, "not found");
}

}
