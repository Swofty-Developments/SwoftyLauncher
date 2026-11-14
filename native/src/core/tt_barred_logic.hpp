#pragma once

#include "main_menu_logic.hpp"

#include <swofty/json.hpp>

#include <cstddef>
#include <cstdint>
#include <string>
#include <string_view>
#include <vector>

// The Time Trial barred-addons warning: swofty\tt.json's "tt_barred" (the wrapper writes it from the boards server's
// GET /v1/tt/barred and the launched profile's enabled addons) names the addons the server rejects laps with. While the
// game is in Time Trial, the overlay paints one red line naming them at the top of the screen.
namespace runtime::tt_barred {

constexpr size_t MAX_NAMES = 8;
constexpr size_t MAX_NAME = 48;
constexpr uint64_t SHOW_AFTER_MS = 300;
constexpr uint64_t HIDE_AFTER_MS = 750;
constexpr const char *LEAD = "Not allowed in Time Trial, laps won't count: ";

inline std::string clean(std::string_view text)
{
    std::string out;
    for (char c : text) {
        unsigned char u = static_cast<unsigned char>(c);
        if (u < 0x20 || u == 0x7f)
            c = ' ';
        if (c == ' ' && (out.empty() || out.back() == ' '))
            continue;
        out.push_back(c);
    }
    while (!out.empty() && out.back() == ' ')
        out.pop_back();
    if (out.size() > MAX_NAME) {
        size_t cut = MAX_NAME;
        while (cut > 0 && (static_cast<unsigned char>(out[cut]) & 0xc0) == 0x80)
            cut--;
        out.resize(cut);
        out += "...";
    }
    return out;
}

// The names of the barred addons in tt.json's text, in its order: an entry's "name", else its "id", or the entry
// itself when it is a string. Blank and repeated names are left out.
inline std::vector<std::string> names_of(const std::string &text)
{
    std::vector<std::string> out;
    swofty::json::Value doc = swofty::json::parse(text);
    if (!doc.is_object())
        return out;
    const swofty::json::Value &list = doc["tt_barred"];
    if (!list.is_array())
        return out;
    for (size_t i = 0; i < list.size() && out.size() < MAX_NAMES; i++) {
        const swofty::json::Value &e = list[i];
        std::string name;
        if (e.is_string())
            name = clean(e.as_string());
        else if (e.is_object()) {
            name = clean(e["name"].as_string());
            if (name.empty())
                name = clean(e["id"].as_string());
        }
        if (name.empty())
            continue;
        bool seen = false;
        for (const auto &n : out)
            seen = seen || n == name;
        if (!seen)
            out.push_back(std::move(name));
    }
    return out;
}

inline std::string line_of(const std::vector<std::string> &names)
{
    if (names.empty())
        return {};
    std::string s = LEAD;
    for (size_t i = 0; i < names.size(); i++) {
        if (i)
            s += ", ";
        s += names[i];
    }
    return s;
}

// The line shows once the game has been in Time Trial for SHOW_AFTER_MS and goes once it has been out of it for
// HIDE_AFTER_MS, so the mode read's short blanks while a Time Trial screen loads do not blink it. The main menu hides it
// at once.
class Gate {
  public:
    bool step(bool in_time_trial, bool home, uint64_t now_ms)
    {
        if (home) {
            shown_ = false;
            timing_ = false;
            return false;
        }
        if (in_time_trial != want_ || !timing_) {
            want_ = in_time_trial;
            since_ = now_ms;
            timing_ = true;
        }
        uint64_t held = now_ms - since_;
        if (want_ && !shown_ && held >= SHOW_AFTER_MS)
            shown_ = true;
        else if (!want_ && shown_ && held >= HIDE_AFTER_MS)
            shown_ = false;
        return shown_;
    }

    bool shown() const { return shown_; }

  private:
    bool want_ = false, shown_ = false, timing_ = false;
    uint64_t since_ = 0;
};

}
