// SPDX-License-Identifier: MIT
#pragma once
#include <cstdarg>
#include <cstdio>
#include <cstring>
#include <map>
#include <string>
#include <string_view>
#include <vector>

#include <swofty/lang.hpp>
#include <swofty/systems/speech.hpp>

// A fake core for host tests of an addon that follows voice lines: it installs a SwoftyApi whose hooks table runs the
// handlers attached to speech.line, speech.line.timed and speech.line.end, and whose lang table (version 2) answers
// swofty::lang::text and language from the strings you give it. One fake is installed at a time; it restores the
// previous swofty::api when it goes away.
//
//   swofty::fake::Speech fake;
//   fake.text("lng_subtitle_eng_radiocheck_02", "Radio check, how do you hear me?");
//   fake.install();
//   my_addon_subscribe();                                  // swofty::events::on<speech::events::Line>().after(...)
//   fake.post(7, "engineer1", "eng_radiocheck_02");        // speech.line
//   fake.timed(7, 2100);                                   // speech.line.timed
//   fake.end(7);                                           // speech.line.end, reason finished

namespace swofty::fake {

class Speech {
public:
    Speech()
    {
        hooks_ = SwoftyHooks{};
        hooks_.version = SWOFTY_HOOKS_VERSION;
        hooks_.available = &h_available;
        hooks_.live = &h_live;
        hooks_.attach = &h_attach;
        hooks_.detach = &h_detach;
        hooks_.attach_in = &h_attach_in;
        hooks_.fire = &h_fire;
        lang_ = SwoftyLang{};
        lang_.available = &l_available;
        lang_.text = &l_text;
        lang_.language = &l_language;
        api_ = SwoftyApi{};
        api_.version = 9;
        api_.log = &f_log;
        api_.hooks = &hooks_;
        api_.lang = &lang_;
    }

    ~Speech() { uninstall(); }
    Speech(const Speech &) = delete;
    Speech &operator=(const Speech &) = delete;

    void install()
    {
        if (current() == this)
            return;
        previous_ = swofty::api;
        current() = this;
        swofty::api = &api_;
        swofty::lang::detail::set_version(lang_version_);
    }

    void uninstall()
    {
        if (current() != this)
            return;
        current() = nullptr;
        swofty::api = previous_;
        swofty::lang::detail::set_version(0);
    }

    // Make speech::live() report the events unavailable, as a core without the speech layout does.
    void set_live(bool on) { live_ = on; }
    // Answer as a core whose SwoftyLang has this version: 1 makes text() and language() unavailable.
    void set_lang_version(uint32_t v)
    {
        lang_version_ = v;
        if (current() == this)
            swofty::lang::detail::set_version(v);
    }

    void text(std::string key, std::string value) { texts_[std::move(key)] = std::move(value); }
    void language(std::string code) { language_ = std::move(code); }

    size_t handlers(std::string_view point) const
    {
        size_t n = 0;
        for (auto &h : handlers_)
            n += h.point == point;
        return n;
    }

    // Fires speech.line for a new line and returns its payload.
    speech::events::Line post(uint32_t id, std::string_view speaker, std::string_view stem,
                              uint8_t category = speech::CATEGORY_RADIO, uint32_t speaker_index = 0)
    {
        speech::events::Line p = make(id, speaker, stem, category, speaker_index);
        lines_[id] = p;
        fire(p);
        return p;
    }

    // Fires speech.line.timed for a posted line.
    bool timed(uint32_t id, uint32_t duration_ms, uint32_t played_ms = 50)
    {
        auto it = lines_.find(id);
        if (it == lines_.end())
            return false;
        it->second.duration_ms = duration_ms;
        speech::events::Timed p;
        static_cast<speech::events::Line &>(p) = it->second;
        p.played_ms = played_ms;
        fire(p);
        return true;
    }

    // Fires speech.line.end for a posted line and forgets it.
    bool end(uint32_t id, speech::End reason = speech::End::finished, uint32_t played_ms = 0)
    {
        auto it = lines_.find(id);
        if (it == lines_.end())
            return false;
        speech::events::Ended p;
        static_cast<speech::events::Line &>(p) = it->second;
        p.reason = reason;
        p.played_ms = played_ms ? played_ms : p.duration_ms;
        lines_.erase(it);
        fire(p);
        return true;
    }

private:
    struct Handler {
        std::string point;
        uint32_t phase;
        SwoftyHookFn fn;
        void *user;
        uint32_t token;
    };

    static Speech *&current()
    {
        static Speech *c = nullptr;
        return c;
    }

    static speech::events::Line make(uint32_t id, std::string_view speaker, std::string_view stem, uint8_t category,
                                     uint32_t speaker_index)
    {
        speech::events::Line p{};
        p.size = sizeof p;
        p.payload_version = speech::events::Line::version;
        p.id = id;
        p.category = category;
        p.speaker_index = speaker_index;
        p.reason = speech::End::finished;
        std::snprintf(p.speaker, sizeof p.speaker, "%.*s", int(speaker.size()), speaker.data());
        std::snprintf(p.stem, sizeof p.stem, "%.*s", int(stem.size()), stem.data());
        return p;
    }

    template <class E> void fire(E &payload)
    {
        payload.size = uint32_t(sizeof(E));
        payload.payload_version = E::version;
        std::vector<Handler> hs = handlers_;
        for (uint32_t phase : {uint32_t(SWOFTY_PHASE_BEFORE), uint32_t(SWOFTY_PHASE_AFTER)})
            for (auto &h : hs)
                if (h.point == E::point && h.phase == phase) {
                    SwoftyHookCall call{};
                    call.size = sizeof call;
                    call.phase = phase;
                    call.point = E::point;
                    call.payload = &payload;
                    h.fn(h.user, &call);
                }
    }

    static bool speech_point(const char *point)
    {
        return point && (!std::strcmp(point, speech::events::Line::point) || !std::strcmp(point, speech::events::Timed::point) ||
                         !std::strcmp(point, speech::events::Ended::point));
    }

    static void f_log(const char *fmt, ...)
    {
        va_list a;
        va_start(a, fmt);
        std::vfprintf(stderr, fmt, a);
        va_end(a);
        std::fputc('\n', stderr);
    }

    static int h_available() { return current() ? 1 : 0; }

    static int h_live(const char *point, uint32_t *version)
    {
        Speech *c = current();
        if (!c || !speech_point(point) || !c->live_)
            return SWOFTY_ASSET_UNAVAILABLE;
        if (version)
            *version = speech::events::Line::version;
        return SWOFTY_ASSET_OK;
    }

    static int h_attach(const char *point, uint32_t phase, int32_t, SwoftyHookFn fn, void *user, uint32_t *token)
    {
        Speech *c = current();
        if (!c || !point || !fn)
            return SWOFTY_ASSET_INVALID;
        uint32_t t = ++c->next_token_;
        c->handlers_.push_back({point, phase, fn, user, t});
        if (token)
            *token = t;
        return SWOFTY_ASSET_OK;
    }

    static int h_attach_in(const char *point, uint32_t phase, int32_t priority, SwoftyText, SwoftyHookFn fn, void *user,
                           uint32_t *token)
    {
        return h_attach(point, phase, priority, fn, user, token);
    }

    static int h_detach(uint32_t token)
    {
        Speech *c = current();
        if (!c)
            return SWOFTY_ASSET_INVALID;
        for (auto it = c->handlers_.begin(); it != c->handlers_.end(); ++it)
            if (it->token == token) {
                c->handlers_.erase(it);
                return SWOFTY_ASSET_OK;
            }
        return SWOFTY_ASSET_INVALID;
    }

    static int h_fire(const char *, void *, int *replaced)
    {
        if (replaced)
            *replaced = 0;
        return SWOFTY_ASSET_OK;
    }

    static int l_available() { return current() ? 1 : 0; }

    static int copy(const std::string &s, char *out, size_t cap, size_t *len)
    {
        if (len)
            *len = s.size();
        if (!out || s.size() >= cap)
            return SWOFTY_TOO_SMALL;
        std::memcpy(out, s.c_str(), s.size() + 1);
        return SWOFTY_ASSET_OK;
    }

    static int l_text(const char *key, char *out, size_t cap, size_t *len)
    {
        Speech *c = current();
        if (!c)
            return SWOFTY_ASSET_UNAVAILABLE;
        if (!key || !*key)
            return SWOFTY_ASSET_INVALID;
        auto it = c->texts_.find(key);
        return it == c->texts_.end() ? SWOFTY_ASSET_INVALID : copy(it->second, out, cap, len);
    }

    static int l_language(char *out, size_t cap, size_t *len)
    {
        Speech *c = current();
        return c ? copy(c->language_, out, cap, len) : SWOFTY_ASSET_UNAVAILABLE;
    }

    SwoftyApi api_{};
    SwoftyHooks hooks_{};
    SwoftyLang lang_{};
    const SwoftyApi *previous_ = nullptr;
    std::vector<Handler> handlers_;
    std::map<uint32_t, speech::events::Line> lines_;
    std::map<std::string, std::string, std::less<>> texts_;
    std::string language_ = "eng";
    uint32_t next_token_ = 0;
    uint32_t lang_version_ = SWOFTY_LANG_VERSION;
    bool live_ = true;
};

}
