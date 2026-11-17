#pragma once

#include <cstddef>
#include <cstdint>
#include <map>
#include <memory>
#include <string>
#include <vector>

namespace runtime::era {

// A box's DRS rule (content.package rules.drs) applies through the host setup mode's DRS group. A game setup mode
// object keeps each string field as a holder whose +0x10 is the char * label (re/db structs/GameSetupMode.h); its
// +0x1e0 holder names the drs.drs_allowed_lap group the DRS system looks up at session load, at a full safety car
// end and at a red flag restart. The swap points that field at a holder of the rule's group label and remembers the
// game's holder; put_back restores it wherever the field still holds the swap's holder.
constexpr size_t MODE_NAME = 0xd8;
constexpr size_t MODE_DRS_GROUP = 0x1e0;
constexpr size_t HOLDER_TEXT = 0x10;
constexpr size_t DRS_LABEL_MAX = 63;

struct DrsHolder {
    void *unused[2] = {};
    const char *text = nullptr;
    uint8_t pad[0x40] = {};
};
static_assert(offsetof(DrsHolder, text) == HOLDER_TEXT);

class DrsSwaps {
public:
    // A holder per label, at a stable address for the life of the process.
    DrsHolder *holder(const std::string &label)
    {
        if (label.empty() || label.size() > DRS_LABEL_MAX || label.find('\0') != std::string::npos)
            return nullptr;
        auto &h = holders_[label];
        if (!h) {
            h = std::make_unique<Entry>();
            h->label = label;
            h->holder.text = h->label.c_str();
        }
        return &h->holder;
    }

    void swap(uint8_t *mode, DrsHolder *holder)
    {
        if (!mode || !holder)
            return;
        void *&field = *reinterpret_cast<void **>(mode + MODE_DRS_GROUP);
        if (field == holder)
            return;
        bool known = false;
        for (auto &s : swaps_)
            known = known || s.mode == mode;
        if (!known)
            swaps_.push_back({mode, field});
        field = holder;
    }

    size_t put_back()
    {
        size_t n = 0;
        for (auto &s : swaps_) {
            void *&field = *reinterpret_cast<void **>(s.mode + MODE_DRS_GROUP);
            if (!owns(field))
                continue;
            field = s.original;
            n++;
        }
        swaps_.clear();
        return n;
    }

    // Drops the swaps without reading or writing the modes, which the game may have freed.
    size_t forget()
    {
        size_t n = swaps_.size();
        swaps_.clear();
        return n;
    }

    size_t active() const { return swaps_.size(); }

    bool owns(const void *holder) const
    {
        for (auto &[label, e] : holders_)
            if (&e->holder == holder)
                return true;
        return false;
    }

private:
    struct Entry {
        std::string label;
        DrsHolder holder;
    };
    struct Swap {
        uint8_t *mode;
        void *original;
    };
    std::map<std::string, std::unique_ptr<Entry>> holders_;
    std::vector<Swap> swaps_;
};

}
