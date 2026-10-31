#pragma once
#include <algorithm>
#include <cctype>
#include <cstdint>
#include <set>
#include <string>
#include <string_view>
#include <vector>

#include <swofty/json.hpp>

namespace runtime::career_saves {

using swofty::json::Value;

constexpr int INDEX_VERSION = 1;
constexpr size_t MAX_DATA = size_t(8) << 20;
constexpr size_t MAX_CAREERS = 4096;

inline const char *const STATUS_PRESENT = "present";
inline const char *const STATUS_MISSING = "missing";

inline bool plain_id(std::string_view s, size_t limit = 160)
{
    if (s.empty() || s.size() > limit)
        return false;
    for (unsigned char c : s)
        if (c < 0x20 || c == '"' || c == '\\' || c == '/' || c == 0x7f)
            return false;
    return true;
}

inline bool addon_id(std::string_view s)
{
    if (s.empty() || s.size() > 96)
        return false;
    for (unsigned char c : s)
        if (!(std::isalnum(c) || c == '.' || c == '_' || c == '-'))
            return false;
    return s.front() != '.' && s.find("..") == std::string_view::npos;
}

inline std::string safe_name(std::string_view career)
{
    std::string out;
    for (unsigned char c : career) {
        if (std::isalnum(c) || c == '-' || c == '_' || c == '.')
            out += char(c);
        else {
            static const char hex[] = "0123456789abcdef";
            out += '%';
            out += hex[c >> 4];
            out += hex[c & 15];
        }
    }
    return out;
}

inline std::string data_dir(std::string_view career) { return "careers/data/" + safe_name(career); }

inline std::string data_file(std::string_view career, std::string_view addon)
{
    return data_dir(career) + "/" + std::string(addon) + ".json";
}

inline std::string wrap_data(uint32_t version, const Value &value)
{
    return swofty::json::dump(Value::object({{"version", Value(int64_t(version))}, {"data", value}}));
}

inline bool unwrap_data(const std::string &text, uint32_t &version, std::string &value)
{
    swofty::json::Error e;
    Value v = swofty::json::parse(text, &e);
    if (!e.ok() || !v.is_object() || !v.contains("version") || !v.contains("data") || !v["version"].is_int())
        return false;
    version = uint32_t(v["version"].as_int());
    value = swofty::json::dump(v["data"]);
    return true;
}

struct Need {
    std::string id, range, why;
};

class Index {
public:
    bool load(const std::string &text)
    {
        careers_.clear();
        if (text.empty())
            return true;
        swofty::json::Error e;
        Value v = swofty::json::parse(text, &e);
        if (!e.ok())
            return false;
        const swofty::json::Array *list = v.is_array() ? v.if_array() : v["careers"].if_array();
        if (!list)
            return false;
        for (const Value &c : *list)
            if (c.is_object() && plain_id(c["career"].as_string()) && careers_.size() < MAX_CAREERS)
                careers_.push_back(c);
        return true;
    }

    size_t size() const { return careers_.size(); }

    Value *find(std::string_view id)
    {
        for (Value &c : careers_)
            if (c["career"].as_string() == id)
                return &c;
        return nullptr;
    }

    const Value *find(std::string_view id) const { return const_cast<Index *>(this)->find(id); }

    bool knows(std::string_view id) const { return find(id) != nullptr; }

    Value &touch(std::string_view id)
    {
        if (Value *c = find(id))
            return *c;
        Value rec = Value::object();
        rec["career"] = Value(std::string(id));
        rec["status"] = Value(STATUS_PRESENT);
        careers_.push_back(std::move(rec));
        return careers_.back();
    }

    void set_text(std::string_view id, const char *key, std::string_view value)
    {
        if (!value.empty())
            touch(id)[key] = Value(std::string(value));
    }

    void set_int(std::string_view id, const char *key, int64_t value)
    {
        touch(id)[key] = Value(value);
    }

    void saved(std::string_view id, int64_t now_ms)
    {
        Value &c = touch(id);
        c["last_saved"] = Value(now_ms);
        c["status"] = Value(STATUS_PRESENT);
    }

    void need(std::string_view id, const Need &n)
    {
        Value &c = touch(id);
        Value &list = c["addons"];
        if (!list.is_array())
            list = Value::array();
        for (Value &a : *list.if_array())
            if (a["id"].as_string() == n.id) {
                if (!n.range.empty())
                    a["need"] = Value(n.range);
                if (!n.why.empty())
                    a["why"] = Value(n.why);
                return;
            }
        Value a = Value::object({{"id", Value(n.id)}});
        if (!n.range.empty())
            a["need"] = Value(n.range);
        if (!n.why.empty())
            a["why"] = Value(n.why);
        list.push_back(std::move(a));
    }

    std::set<std::string> found_in(const std::set<std::string> &keys) const
    {
        std::set<std::string> out;
        for (const Value &c : careers_) {
            std::string id(c["career"].as_string());
            size_t cut = id.rfind(':');
            std::string tail = cut == std::string::npos ? id : id.substr(cut + 1);
            if (tail.compare(0, 3, "new") == 0 || keys.count(tail))
                out.insert(id);
        }
        return out;
    }

    std::vector<std::string> reconcile(const std::set<std::string> &present)
    {
        std::vector<std::string> gone;
        for (Value &c : careers_) {
            std::string id(c["career"].as_string());
            bool here = present.count(id) != 0;
            bool was_missing = c["status"].as_string() == STATUS_MISSING;
            if (here) {
                c["status"] = Value(STATUS_PRESENT);
            } else if (!was_missing) {
                c["status"] = Value(STATUS_MISSING);
                gone.push_back(id);
            }
        }
        return gone;
    }

    std::vector<std::string> missing_ids() const
    {
        std::vector<std::string> out;
        for (const Value &c : careers_)
            if (c["status"].as_string() == STATUS_MISSING)
                out.emplace_back(c["career"].as_string());
        return out;
    }

    std::string to_json() const
    {
        Value list = Value::array();
        for (const Value &c : careers_)
            list.push_back(c);
        return swofty::json::dump(
            Value::object({{"version", Value(int64_t(INDEX_VERSION))}, {"careers", std::move(list)}}));
    }

    std::string only_json(const std::vector<std::string> &ids) const
    {
        Value list = Value::array();
        for (const Value &c : careers_)
            if (std::find(ids.begin(), ids.end(), std::string(c["career"].as_string())) != ids.end())
                list.push_back(c);
        return swofty::json::dump(
            Value::object({{"version", Value(int64_t(INDEX_VERSION))}, {"careers", std::move(list)}}));
    }

private:
    std::vector<Value> careers_;
};

inline std::string label_of(std::string_view content, std::string_view driver, std::string_view team, int season)
{
    std::string out = std::string(content);
    auto add = [&](std::string_view part) {
        if (!part.empty())
            out += (out.empty() ? "" : ": ") + std::string(part);
    };
    add(driver);
    if (!team.empty())
        out += (out.empty() ? "" : ", ") + std::string(team);
    if (season > 0)
        out += (out.empty() ? "" : ", ") + std::to_string(season);
    return out;
}

}
