// SPDX-License-Identifier: AGPL-3.0-or-later
#pragma once

#include <cstdint>
#include <string>
#include <string_view>
#include <utility>
#include <vector>

namespace f1990 {

// One language of data/flavio-strings.json: {"<code>": {"<lng key>": "<text>", ...}, ...}.
struct AgentStrings {
    std::string code;
    std::vector<std::pair<std::string, std::string>> texts;
};

namespace detail {

struct JsonReader {
    std::string_view s;
    size_t i = 0;

    void space()
    {
        while (i < s.size() && (s[i] == ' ' || s[i] == '\t' || s[i] == '\n' || s[i] == '\r'))
            i++;
    }

    bool eat(char c)
    {
        space();
        if (i < s.size() && s[i] == c) {
            i++;
            return true;
        }
        return false;
    }

    bool hex4(uint32_t &out)
    {
        if (i + 4 > s.size())
            return false;
        out = 0;
        for (int n = 0; n < 4; n++) {
            char c = s[i++];
            out <<= 4;
            if (c >= '0' && c <= '9')
                out |= uint32_t(c - '0');
            else if (c >= 'a' && c <= 'f')
                out |= uint32_t(c - 'a' + 10);
            else if (c >= 'A' && c <= 'F')
                out |= uint32_t(c - 'A' + 10);
            else
                return false;
        }
        return true;
    }

    static void utf8(std::string &out, uint32_t cp)
    {
        if (cp < 0x80)
            out += char(cp);
        else if (cp < 0x800) {
            out += char(0xc0 | (cp >> 6));
            out += char(0x80 | (cp & 0x3f));
        } else if (cp < 0x10000) {
            out += char(0xe0 | (cp >> 12));
            out += char(0x80 | ((cp >> 6) & 0x3f));
            out += char(0x80 | (cp & 0x3f));
        } else {
            out += char(0xf0 | (cp >> 18));
            out += char(0x80 | ((cp >> 12) & 0x3f));
            out += char(0x80 | ((cp >> 6) & 0x3f));
            out += char(0x80 | (cp & 0x3f));
        }
    }

    bool string(std::string &out)
    {
        if (!eat('"'))
            return false;
        out.clear();
        while (i < s.size()) {
            char c = s[i++];
            if (c == '"')
                return true;
            if (c != '\\') {
                out += c;
                continue;
            }
            if (i >= s.size())
                return false;
            switch (char e = s[i++]) {
            case '"':
            case '\\':
            case '/':
                out += e;
                break;
            case 'b':
                out += '\b';
                break;
            case 'f':
                out += '\f';
                break;
            case 'n':
                out += '\n';
                break;
            case 'r':
                out += '\r';
                break;
            case 't':
                out += '\t';
                break;
            case 'u': {
                uint32_t cp, lo;
                if (!hex4(cp))
                    return false;
                if (cp >= 0xd800 && cp < 0xdc00) {
                    if (i + 2 > s.size() || s[i] != '\\' || s[i + 1] != 'u')
                        return false;
                    i += 2;
                    if (!hex4(lo) || lo < 0xdc00 || lo >= 0xe000)
                        return false;
                    cp = 0x10000 + ((cp - 0xd800) << 10) + (lo - 0xdc00);
                } else if (cp >= 0xdc00 && cp < 0xe000)
                    return false;
                utf8(out, cp);
                break;
            }
            default:
                return false;
            }
        }
        return false;
    }

    bool peek(char c)
    {
        space();
        return i < s.size() && s[i] == c;
    }

    template <class Member> bool object(Member &&member)
    {
        if (!eat('{'))
            return false;
        if (eat('}'))
            return true;
        do {
            std::string key;
            if (!string(key) || !eat(':') || !member(std::move(key)))
                return false;
        } while (eat(','));
        return eat('}');
    }
};

}

// Parses data/flavio-strings.json. Returns false, with the byte offset of the first error in
// *error_at, on anything but an object of objects of strings.
inline bool parse_agent_strings(std::string_view json, std::vector<AgentStrings> &out, size_t *error_at = nullptr)
{
    detail::JsonReader r{json};
    out.clear();
    bool ok = r.object([&](std::string code) {
        AgentStrings &lang = out.emplace_back(AgentStrings{std::move(code), {}});
        return r.object([&](std::string key) {
            std::string text;
            if (!r.string(text))
                return false;
            lang.texts.emplace_back(std::move(key), std::move(text));
            return true;
        });
    });
    r.space();
    if (ok && r.i == json.size())
        return true;
    if (error_at)
        *error_at = r.i;
    out.clear();
    return false;
}

// One string of a nested JSON object, with the keys that lead to it.
struct StringLeaf {
    std::vector<std::string> path;
    std::string text;
};

namespace detail {

inline bool leaves(JsonReader &r, std::vector<std::string> &path, std::vector<StringLeaf> &out, int depth)
{
    return depth < 8 && r.object([&](std::string key) {
        path.push_back(std::move(key));
        bool ok;
        if (r.peek('{'))
            ok = leaves(r, path, out, depth + 1);
        else {
            std::string text;
            ok = r.string(text);
            if (ok)
                out.push_back({path, std::move(text)});
        }
        path.pop_back();
        return ok;
    });
}

}

// Parses an object whose values are strings or such objects, as data/rnd-art.json and
// data/rnd-strings.json are. Returns false, with the byte offset of the first error in *error_at,
// on anything else.
inline bool parse_string_leaves(std::string_view json, std::vector<StringLeaf> &out, size_t *error_at = nullptr)
{
    detail::JsonReader r{json};
    std::vector<std::string> path;
    out.clear();
    bool ok = detail::leaves(r, path, out, 0);
    r.space();
    if (ok && r.i == json.size())
        return true;
    if (error_at)
        *error_at = r.i;
    out.clear();
    return false;
}

}
