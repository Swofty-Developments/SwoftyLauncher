#pragma once
#include "core/hud/drawlist/frame.hpp"
#include "swofty_api.h"

#include <algorithm>
#include <cstddef>
#include <cstdint>
#include <string>
#include <vector>

namespace hud::drawlist {

// ----- registry ---------------------------------------------------------------------------------

struct Paintsub {
    uint32_t token;
    SwoftyPaintFn fn;
    void *user;
    std::string owner;
};

struct Image {
    std::string id;
    uint32_t width = 0, height = 0;
    std::vector<uint8_t> rgba;
    uint32_t generation = 0;
    std::string source;
};

class Registry {
public:
    uint32_t add(SwoftyPaintFn fn, void *user, std::string owner)
    {
        if (!fn)
            return 0;
        uint32_t token = next_++;
        subs_.push_back({token, fn, user, std::move(owner)});
        return token;
    }

    bool remove(uint32_t token)
    {
        for (size_t i = 0; i < subs_.size(); i++)
            if (subs_[i].token == token) {
                subs_.erase(subs_.begin() + i);
                return true;
            }
        return false;
    }

    size_t count() const { return subs_.size(); }

    const std::vector<Paintsub> &subs() const { return subs_; }

    int intern_image(const std::string &id)
    {
        int at = find_image(id);
        if (at >= 0)
            return at;
        Image im;
        im.id = id;
        images_.push_back(std::move(im));
        return int(images_.size() - 1);
    }

    int set_image(const std::string &id, uint32_t width, uint32_t height, std::vector<uint8_t> rgba)
    {
        int at = intern_image(id);
        Image &im = images_[size_t(at)];
        im.width = width;
        im.height = height;
        im.rgba = std::move(rgba);
        im.generation++;
        return at;
    }

    void set_source(int slot, const std::string &source)
    {
        if (slot >= 0 && size_t(slot) < images_.size())
            images_[size_t(slot)].source = source;
    }

    int find_image(const std::string &id) const
    {
        for (size_t i = 0; i < images_.size(); i++)
            if (images_[i].id == id)
                return int(i);
        return -1;
    }

    const Image *image(int slot) const { return slot >= 0 && size_t(slot) < images_.size() ? &images_[size_t(slot)] : nullptr; }

    Image *image(int slot) { return slot >= 0 && size_t(slot) < images_.size() ? &images_[size_t(slot)] : nullptr; }

    size_t image_count() const { return images_.size(); }

private:
    std::vector<Paintsub> subs_;
    std::vector<Image> images_;
    uint32_t next_ = 1;
};

inline bool listed(const std::vector<std::string> &ids, const std::string &id)
{
    return std::find(ids.begin(), ids.end(), id) != ids.end();
}

inline void build(const std::vector<Paintsub> &subs, Frame &f, float width, float height,
                  const std::vector<std::string> &off = {})
{
    begin(f, width, height);
    for (const auto &s : subs)
        if (s.fn && !listed(off, s.owner)) {
            reset_state(f);
            f.owner = s.owner;
            s.fn(reinterpret_cast<SwoftyPaint *>(&f), s.user);
        }
}

}
