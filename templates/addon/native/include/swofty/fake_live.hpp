// SPDX-License-Identifier: MIT
#pragma once
#include <cstring>
#include <map>
#include <optional>
#include <string>
#include <vector>
#include <swofty/live.hpp>

// A fake core live state for host tests: set the session and cars the addon should see, install it, and
// swofty::live::* answers from it. One fake is installed at a time.
//
//   swofty::fake::Live fake;
//   fake.session().type = 15;
//   fake.add_car(1, 1).speed = 80.f;
//   fake.set_order({1, 0});
//   fake.install();
//   assert(swofty::live::car(1)->position == 1);
//   assert(swofty::live::order().ids.size() == 2);

namespace swofty::fake {

class Live {
public:
    Live()
    {
        std::memset(&session_, 0, sizeof session_);
        session_.version = SWOFTY_LIVE_VERSION;
        session_.size = sizeof session_;
        session_.player = session_.watched = session_.subject = -1;
        session_.safety_car = 0xff;
        session_.have = SWOFTY_LIVE_SHAVE_TYPE | SWOFTY_LIVE_SHAVE_CARS;
        std::memset(cars_, 0, sizeof cars_);
        std::memset(details_, 0, sizeof details_);
        std::memset(present_, 0, sizeof present_);
        table_ = {SWOFTY_LIVE_VERSION, &Live::t_available, &Live::t_session, &Live::t_car, &Live::t_subject,
                  &Live::t_turn,       &Live::t_track_turns, &Live::t_badge, &Live::t_seq,
                  &Live::t_driver_name, &Live::t_badge_texture, &Live::t_images, &Live::t_tyre_icon_texture,
                  &Live::t_car_detail, &Live::t_driver_portrait};
        images_ = {2, &Live::t_start, &Live::t_sources, &Live::t_image, &Live::t_clear, &Live::t_start_portraits,
                   &Live::t_image_full};
    }

    ~Live() { uninstall(); }
    Live(const Live &) = delete;
    Live &operator=(const Live &) = delete;

    void install()
    {
        current() = this;
        live::detail::set_table(&table_);
    }

    void uninstall()
    {
        if (current() == this) {
            current() = nullptr;
            live::detail::set_table(nullptr);
        }
    }

    void set_available(bool on) { available_ = on; }
    SwoftyLiveSession &session() { return session_; }

    SwoftyLiveCar &add_car(uint32_t vehicle, int32_t position = 0)
    {
        SwoftyLiveCar &c = cars_[vehicle];
        std::memset(&c, 0, sizeof c);
        c.version = SWOFTY_LIVE_VERSION;
        c.size = sizeof c;
        c.vehicle = vehicle;
        c.participant = 0xffffffffu;
        c.position = position;
        c.gap_leader_ms = c.gap_ahead_ms = SWOFTY_LIVE_NO_GAP;
        c.have = SWOFTY_LIVE_HAVE_IDENT | (position ? SWOFTY_LIVE_HAVE_CLASS : 0);
        c.flags = SWOFTY_LIVE_CAR_RACER;
        SwoftyLiveCarDetail &d = details_[vehicle];
        std::memset(&d, 0, sizeof d);
        d.version = SWOFTY_LIVE_VERSION;
        d.size = sizeof d;
        d.vehicle = vehicle;
        present_[vehicle] = true;
        session_.cars = 0;
        for (bool p : present_)
            session_.cars += p;
        return c;
    }

    SwoftyLiveCar &car(uint32_t vehicle) { return cars_[vehicle]; }
    // The car's detail; set its fields and their have bits (car_detail answers detail.have | car.have).
    SwoftyLiveCarDetail &detail(uint32_t vehicle) { return details_[vehicle]; }

    void set_player(uint32_t vehicle)
    {
        session_.player = static_cast<int32_t>(vehicle);
        cars_[vehicle].flags |= SWOFTY_LIVE_CAR_PLAYER;
        session_.subject = session_.watched >= 0 ? session_.watched : session_.player;
        session_.have |= SWOFTY_LIVE_SHAVE_PLAYER;
    }

    void set_watched(uint32_t vehicle)
    {
        session_.watched = session_.subject = static_cast<int32_t>(vehicle);
        cars_[vehicle].flags |= SWOFTY_LIVE_CAR_WATCHED;
        session_.subject_generation++;
        session_.have |= SWOFTY_LIVE_SHAVE_WATCHED;
    }

    void set_order(const std::vector<uint32_t> &ids)
    {
        session_.classified = static_cast<uint32_t>(ids.size());
        for (size_t i = 0; i < ids.size() && i < SWOFTY_LIVE_MAX_CARS; i++) {
            session_.order[i] = ids[i];
            cars_[ids[i]].position = static_cast<int32_t>(i + 1);
            cars_[ids[i]].have |= SWOFTY_LIVE_HAVE_CLASS;
        }
        session_.have |= SWOFTY_LIVE_SHAVE_ORDER;
    }

    void set_turns(const std::vector<SwoftyLiveTurn> &turns) { turns_ = turns; }
    void set_badge(uint32_t vehicle, const std::string &key) { badges_[vehicle] = key; }
    void set_driver_name(uint32_t vehicle, const std::string &name) { names_[vehicle] = name; }
    // Answer as an older core does: version 1 has no driver_name member, version 2 no team_badge_texture member,
    // version 5 no car_detail member, version 6 no driver_portrait member.
    void set_table_version(uint32_t v) { table_.version = v; }
    // The images table: version 1 has no start_portraits or image_full member.
    void set_images_version(uint32_t v) { images_.version = v; }
    void set_driver_portrait(uint32_t vehicle, const std::string &reference) { portraits_[vehicle] = reference; }
    // Pixels for a reference, tight RGBA8 rows of width * height * 4 bytes: image_full answers them, and image too when
    // they are at most 64 on a side.
    void set_image(const std::string &reference, uint32_t width, uint32_t height, std::vector<uint8_t> rgba)
    {
        pixels_[reference] = Pixels{width, height, ++revision_, std::move(rgba)};
    }
    bool portraits_started() const { return portraits_started_; }
    void set_badge_texture(uint32_t vehicle, const std::string &path) { textures_[vehicle] = path; }
    void set_tyre_icon_texture(uint32_t vehicle, const std::string &path) { tyre_textures_[vehicle] = path; }
    void advance() { frame_++; }

private:
    static Live *&current()
    {
        static Live *p = nullptr;
        return p;
    }

    static int t_available() { return current() && current()->available_ ? 1 : 0; }

    static int t_session(SwoftyLiveSession *out)
    {
        if (!t_available())
            return SWOFTY_ASSET_UNAVAILABLE;
        *out = current()->session_;
        return SWOFTY_ASSET_OK;
    }

    static int t_car(uint32_t vehicle, SwoftyLiveCar *out)
    {
        if (!t_available())
            return SWOFTY_ASSET_UNAVAILABLE;
        if (vehicle >= SWOFTY_LIVE_MAX_CARS)
            return SWOFTY_ASSET_INVALID;
        if (!current()->present_[vehicle])
            return SWOFTY_ASSET_UNAVAILABLE;
        *out = current()->cars_[vehicle];
        return SWOFTY_ASSET_OK;
    }

    static int t_subject(SwoftyLiveCar *out)
    {
        if (!t_available() || current()->session_.subject < 0)
            return SWOFTY_ASSET_UNAVAILABLE;
        return t_car(static_cast<uint32_t>(current()->session_.subject), out);
    }

    static int t_turn(uint32_t vehicle, SwoftyLiveTurn *out)
    {
        if (!t_available() || vehicle >= SWOFTY_LIVE_MAX_CARS || !current()->present_[vehicle])
            return SWOFTY_ASSET_UNAVAILABLE;
        float lap_m = current()->cars_[vehicle].lap_m;
        const SwoftyLiveTurn *best = nullptr;
        float best_d = 0;
        for (const SwoftyLiveTurn &t : current()->turns_) {
            float d = t.apex_lap_m - lap_m;
            if (d < -15.f || d > 220.f)
                continue;
            if (!best || d < best_d) {
                best = &t;
                best_d = d;
            }
        }
        if (!best)
            return SWOFTY_ASSET_UNAVAILABLE;
        *out = *best;
        out->distance_ahead_m = best_d;
        return SWOFTY_ASSET_OK;
    }

    static int t_track_turns(uint32_t *count, SwoftyLiveTurn *out, uint32_t cap)
    {
        if (!t_available() || current()->turns_.empty())
            return SWOFTY_ASSET_UNAVAILABLE;
        *count = static_cast<uint32_t>(current()->turns_.size());
        for (uint32_t i = 0; out && i < cap && i < *count; i++)
            out[i] = current()->turns_[i];
        return SWOFTY_ASSET_OK;
    }

    static int t_badge(uint32_t vehicle, char *key, uint32_t cap)
    {
        if (!t_available())
            return SWOFTY_ASSET_UNAVAILABLE;
        auto it = current()->badges_.find(vehicle);
        if (it == current()->badges_.end() || it->second.size() + 1 > cap)
            return SWOFTY_ASSET_UNAVAILABLE;
        std::memcpy(key, it->second.c_str(), it->second.size() + 1);
        return SWOFTY_ASSET_OK;
    }

    static int t_driver_name(uint32_t vehicle, char *out, uint32_t cap)
    {
        if (!t_available())
            return SWOFTY_ASSET_UNAVAILABLE;
        if (!out || cap < 2 || vehicle >= SWOFTY_LIVE_MAX_CARS)
            return SWOFTY_ASSET_INVALID;
        auto it = current()->names_.find(vehicle);
        if (it == current()->names_.end())
            return SWOFTY_ASSET_UNAVAILABLE;
        size_t n = it->second.size() < cap - 1 ? it->second.size() : cap - 1;
        std::memcpy(out, it->second.data(), n);
        out[n] = 0;
        return SWOFTY_ASSET_OK;
    }

    static int t_badge_texture(uint32_t vehicle, char *path, uint32_t cap)
    {
        if (!t_available())
            return SWOFTY_ASSET_UNAVAILABLE;
        auto it = current()->textures_.find(vehicle);
        std::string s;
        if (it != current()->textures_.end())
            s = it->second;
        else if (auto key = current()->badges_.find(vehicle); key != current()->badges_.end())
            s = "#overrides.osd.timing_tower.team_badges_hud." + key->second;
        if (s.empty() || s.size() + 1 > cap)
            return SWOFTY_ASSET_UNAVAILABLE;
        std::memcpy(path, s.c_str(), s.size() + 1);
        return SWOFTY_ASSET_OK;
    }

    static int t_seq(uint64_t *frame)
    {
        if (!t_available())
            return SWOFTY_ASSET_UNAVAILABLE;
        *frame = current()->frame_;
        return SWOFTY_ASSET_OK;
    }

    static int t_tyre_icon_texture(uint32_t vehicle, char *path, uint32_t cap)
    {
        if (!path || cap < 2 || vehicle >= SWOFTY_LIVE_MAX_CARS)
            return SWOFTY_ASSET_INVALID;
        path[0] = 0;
        if (!t_available() || !current()->present_[vehicle] ||
            !(current()->cars_[vehicle].have & SWOFTY_LIVE_HAVE_TYRE))
            return SWOFTY_ASSET_UNAVAILABLE;
        const auto it = current()->tyre_textures_.find(vehicle);
        if (it == current()->tyre_textures_.end() || it->second.empty())
            return SWOFTY_ASSET_UNAVAILABLE;
        if (it->second.size() + 1 > cap)
            return SWOFTY_ASSET_INVALID;
        std::memcpy(path, it->second.c_str(), it->second.size() + 1);
        return SWOFTY_ASSET_OK;
    }

    static const SwoftyLiveImages *t_images() { return current() ? &current()->images_ : nullptr; }
    static int t_start() { return SWOFTY_ASSET_OK; }
    static int t_sources(char *, uint32_t, uint64_t *) { return SWOFTY_ASSET_UNAVAILABLE; }
    static void t_clear() {}

    static int t_start_portraits(uint32_t flags)
    {
        if (flags)
            return SWOFTY_ASSET_INVALID;
        if (current())
            current()->portraits_started_ = true;
        return SWOFTY_ASSET_OK;
    }

    static int t_pixels(const char *reference, uint8_t *rgba, uint32_t cap, uint32_t *width, uint32_t *height,
                        uint64_t *revision, uint32_t max_side)
    {
        if (!reference || !width || !height || !revision)
            return SWOFTY_ASSET_INVALID;
        if (!current())
            return SWOFTY_ASSET_UNAVAILABLE;
        const auto it = current()->pixels_.find(reference);
        if (it == current()->pixels_.end() || it->second.width > max_side || it->second.height > max_side)
            return SWOFTY_ASSET_UNAVAILABLE;
        *width = it->second.width;
        *height = it->second.height;
        *revision = it->second.revision;
        if (!rgba || cap < it->second.rgba.size())
            return SWOFTY_ASSET_INVALID;
        std::memcpy(rgba, it->second.rgba.data(), it->second.rgba.size());
        return SWOFTY_ASSET_OK;
    }

    static int t_image(const char *reference, uint8_t *rgba, uint32_t cap, uint32_t *width, uint32_t *height,
                       uint64_t *revision)
    {
        return t_pixels(reference, rgba, cap, width, height, revision, 64);
    }

    static int t_image_full(const char *reference, uint8_t *rgba, uint32_t cap, uint32_t *width, uint32_t *height,
                            uint64_t *revision)
    {
        return t_pixels(reference, rgba, cap, width, height, revision, SWOFTY_LIVE_PORTRAIT_MAX_SIDE);
    }

    static int t_driver_portrait(uint32_t vehicle, char *reference, uint32_t cap)
    {
        if (!reference || cap < 2 || vehicle >= SWOFTY_LIVE_MAX_CARS)
            return SWOFTY_ASSET_INVALID;
        reference[0] = 0;
        if (!t_available() || !current()->present_[vehicle])
            return SWOFTY_ASSET_UNAVAILABLE;
        const auto it = current()->portraits_.find(vehicle);
        if (it == current()->portraits_.end() || it->second.empty())
            return SWOFTY_ASSET_UNAVAILABLE;
        if (it->second.size() + 1 > cap)
            return SWOFTY_ASSET_INVALID;
        std::memcpy(reference, it->second.c_str(), it->second.size() + 1);
        return SWOFTY_ASSET_OK;
    }

    static int t_car_detail(uint32_t vehicle, SwoftyLiveCarDetail *out)
    {
        if (!t_available())
            return SWOFTY_ASSET_UNAVAILABLE;
        if (!out || vehicle >= SWOFTY_LIVE_MAX_CARS)
            return SWOFTY_ASSET_INVALID;
        if (!current()->present_[vehicle])
            return SWOFTY_ASSET_UNAVAILABLE;
        *out = current()->details_[vehicle];
        out->have |= current()->cars_[vehicle].have;
        return SWOFTY_ASSET_OK;
    }

    bool available_ = true;
    uint64_t frame_ = 1;
    SwoftyLiveSession session_;
    SwoftyLiveCar cars_[SWOFTY_LIVE_MAX_CARS];
    SwoftyLiveCarDetail details_[SWOFTY_LIVE_MAX_CARS];
    bool present_[SWOFTY_LIVE_MAX_CARS];
    std::vector<SwoftyLiveTurn> turns_;
    std::map<uint32_t, std::string> badges_, textures_;
    std::map<uint32_t, std::string> tyre_textures_;
    std::map<uint32_t, std::string> names_;
    std::map<uint32_t, std::string> portraits_;
    struct Pixels {
        uint32_t width, height;
        uint64_t revision;
        std::vector<uint8_t> rgba;
    };
    std::map<std::string, Pixels> pixels_;
    uint64_t revision_ = 0;
    bool portraits_started_ = false;
    SwoftyLive table_;
    SwoftyLiveImages images_;
};

}
